//! UI-thread result delivery. Worker jobs receive owned snapshots, never UI
//! properties or mutable component borrows. Applications may use any executor.
use crate::ViewAction;
use std::{
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
};
use zui_platform::EventWaker;
use zui_state::{Disposal, Scope};

const CAPACITY: usize = 256;
type Apply<M> = Box<dyn FnOnce(&mut M) -> ViewAction + Send>;
struct Completion<M> {
    cancelled: Arc<AtomicBool>,
    apply: Apply<M>,
}
#[derive(Default)]
struct WakeState {
    waker: Mutex<Option<Arc<dyn EventWaker>>>,
}
impl WakeState {
    fn wake(&self) {
        let waker = self.waker.lock().expect("UI wake lock poisoned").clone();
        if let Some(waker) = waker {
            let _ = waker.wake();
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PostError {
    Disposed,
    Full,
    Closed,
}
impl std::fmt::Display for PostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for PostError {}

/// Thread-safe sender of owned results; does not keep the component alive.
pub struct TaskSender<M> {
    sender: SyncSender<Completion<M>>,
    cancelled: Arc<AtomicBool>,
    wake: Arc<WakeState>,
}
impl<M> Clone for TaskSender<M> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            cancelled: self.cancelled.clone(),
            wake: self.wake.clone(),
        }
    }
}
impl<M: 'static> TaskSender<M> {
    /// Nonblocking, bounded delivery. A successful post is not a promise that a
    /// disposed component will apply it; cancellation is checked again on the UI.
    pub fn post(
        &self,
        apply: impl FnOnce(&mut M) -> ViewAction + Send + 'static,
    ) -> Result<(), PostError> {
        if self.cancelled.load(Ordering::Acquire) {
            return Err(PostError::Disposed);
        }
        self.sender
            .try_send(Completion {
                cancelled: self.cancelled.clone(),
                apply: Box::new(apply),
            })
            .map_err(|error| match error {
                TrySendError::Full(_) => PostError::Full,
                TrySendError::Disconnected(_) => PostError::Closed,
            })?;
        self.wake.wake();
        Ok(())
    }
}
struct TaskLifetime {
    cancelled: Arc<AtomicBool>,
    _disposal: Disposal,
}
impl Drop for TaskLifetime {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}
/// UI-side lifetime for result delivery. Clone into components that launch work;
/// use `scoped` for children. Dropping its scope discards pending/late results.
pub struct Tasks<M> {
    sender: TaskSender<M>,
    lifetime: Rc<TaskLifetime>,
}
impl<M> Clone for Tasks<M> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            lifetime: self.lifetime.clone(),
        }
    }
}
impl<M: 'static> Tasks<M> {
    fn attach(sender: SyncSender<Completion<M>>, wake: Arc<WakeState>, scope: &Scope) -> Self {
        let cancelled = Arc::new(AtomicBool::new(false));
        let cleanup = cancelled.clone();
        let disposal = scope
            .token()
            .on_dispose(move || cleanup.store(true, Ordering::Release));
        Self {
            sender: TaskSender {
                sender,
                wake,
                cancelled: cancelled.clone(),
            },
            lifetime: Rc::new(TaskLifetime {
                cancelled,
                _disposal: disposal,
            }),
        }
    }
    pub fn scoped(&self, scope: &Scope) -> Self {
        Self::attach(self.sender.sender.clone(), self.sender.wake.clone(), scope)
    }
    pub fn sender(&self) -> TaskSender<M> {
        self.sender.clone()
    }
    /// Convenience for blocking work. Async runtimes can instead clone `sender`
    /// and post their completion. Return ordinary Result values for task errors.
    pub fn spawn<T: Send + 'static>(
        &self,
        work: impl FnOnce() -> T + Send + 'static,
        apply: impl FnOnce(&mut M, T) -> ViewAction + Send + 'static,
    ) -> std::io::Result<std::thread::JoinHandle<Result<(), PostError>>> {
        let sender = self.sender();
        std::thread::Builder::new()
            .name("zui-task".into())
            .spawn(move || {
                let result = work();
                sender.post(move |model| apply(model, result))
            })
    }
}

pub(super) struct Mailbox<M> {
    receiver: Receiver<Completion<M>>,
    wake: Arc<WakeState>,
    // Keep the application result scope active even if the model uses only senders.
    _tasks: Tasks<M>,
}
impl<M: 'static> Mailbox<M> {
    pub(super) fn new(scope: &Scope) -> (Self, Tasks<M>) {
        let (sender, receiver) = mpsc::sync_channel(CAPACITY);
        let wake = Arc::new(WakeState::default());
        let tasks = Tasks::attach(sender, wake.clone(), scope);
        (
            Self {
                receiver,
                wake,
                _tasks: tasks.clone(),
            },
            tasks,
        )
    }
    pub(super) fn connect(&self, waker: Option<Arc<dyn EventWaker>>) {
        *self.wake.waker.lock().expect("UI wake lock poisoned") = waker;
        // Also flush results queued before a native window became available.
        self.wake.wake();
    }
    pub(super) fn disconnect(&self) {
        *self.wake.waker.lock().expect("UI wake lock poisoned") = None;
    }
    pub(super) fn drain(&self, model: &mut M) -> ViewAction {
        let mut result = ViewAction::default();
        for _ in 0..CAPACITY {
            let Ok(completion) = self.receiver.try_recv() else {
                break;
            };
            if !completion.cancelled.load(Ordering::Acquire) {
                let action = (completion.apply)(model);
                merge(&mut result, action);
                if result.window.exit {
                    break;
                }
            }
        }
        result
    }
}
pub(super) fn merge(target: &mut ViewAction, action: ViewAction) {
    target.rebuild |= action.rebuild;
    target.window.redraw |= action.window.redraw;
    target.window.exit |= action.window.exit;
    target.window.wake_at = match (target.window.wake_at, action.window.wake_at) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    struct Waker(AtomicUsize);
    impl EventWaker for Waker {
        fn wake(&self) -> Result<(), zui_platform::PlatformError> {
            self.0.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }
    }
    #[test]
    fn worker_results_run_on_ui_thread_in_order_and_wake_without_polling() {
        let scope = Scope::new();
        // The model deliberately is not Send.
        let (mailbox, tasks) = Mailbox::<Rc<RefCell<Vec<usize>>>>::new(&scope);
        let waker = Arc::new(Waker(AtomicUsize::new(0)));
        mailbox.connect(Some(waker.clone()));
        let ui_thread = std::thread::current().id();
        let sender = tasks.sender();
        std::thread::spawn(move || {
            for number in 0..3 {
                sender
                    .post(move |model| {
                        assert_eq!(std::thread::current().id(), ui_thread);
                        model.borrow_mut().push(number);
                        ViewAction::default()
                    })
                    .unwrap();
            }
        })
        .join()
        .unwrap();
        let mut model = Rc::new(RefCell::new(Vec::new()));
        mailbox.drain(&mut model);
        assert_eq!(*model.borrow(), [0, 1, 2]);
        assert!(waker.0.load(Ordering::Relaxed) >= 3);
    }
    use std::cell::RefCell;
    #[test]
    fn disposed_children_discard_queued_and_late_results() {
        let scope = Scope::new();
        let (mailbox, tasks) = Mailbox::<usize>::new(&scope);
        let child = Scope::new();
        let child_tasks = tasks.scoped(&child);
        let sender = child_tasks.sender();
        sender
            .post(|value| {
                *value += 1;
                ViewAction::default()
            })
            .unwrap();
        drop(child);
        let mut model = 0;
        mailbox.drain(&mut model);
        assert_eq!(model, 0);
        assert_eq!(
            sender.post(|_| ViewAction::default()),
            Err(PostError::Disposed)
        );
        tasks
            .sender()
            .post(|value| {
                *value += 2;
                ViewAction::default()
            })
            .unwrap();
        mailbox.drain(&mut model);
        assert_eq!(model, 2);
    }
    #[test]
    fn queue_is_bounded_and_backpressure_is_explicit() {
        let scope = Scope::new();
        let (mailbox, tasks) = Mailbox::<usize>::new(&scope);
        for _ in 0..CAPACITY {
            tasks
                .sender()
                .post(|value| {
                    *value += 1;
                    ViewAction::default()
                })
                .unwrap();
        }
        assert_eq!(
            tasks.sender().post(|_| ViewAction::default()),
            Err(PostError::Full)
        );
        let mut model = 0;
        mailbox.drain(&mut model);
        assert_eq!(model, CAPACITY);
    }
}
