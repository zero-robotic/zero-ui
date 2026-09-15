//! Deterministic multi-host backend for tests and no-display environments.

use std::{
    cell::Cell,
    collections::{HashMap, VecDeque},
    path::PathBuf,
    sync::{Arc, Mutex},
};

use zui_core::{Dip, Id, OutputId, Point, Rect, ScaleFactor, Size, WindowId};
use zui_platform::spi;
use zui_platform::{
    AppContext, AppLoop, Backend, Capabilities, Host, LoopControl, Output, Paths, PlatformError,
    PlatformEvent, SurfaceTarget, UiTask, UiTaskPoster, WindowOptions,
};

pub struct HeadlessBackend {
    next_id: u64,
    max_iterations: usize,
    paths: Result<Paths, PlatformError>,
    outputs: Vec<Output>,
    capabilities: Capabilities,
    scripted_events: VecDeque<PlatformEvent>,
}

impl Default for HeadlessBackend {
    fn default() -> Self {
        Self {
            next_id: 1,
            max_iterations: 1_024,
            paths: Ok(Paths {
                config_dir: PathBuf::from("/headless/config"),
                data_dir: PathBuf::from("/headless/data"),
                cache_dir: PathBuf::from("/headless/cache"),
                runtime_dir: Some(PathBuf::from("/headless/runtime")),
            }),
            outputs: vec![Output {
                id: OutputId(Id::new(1)),
                name: Some("headless-0".into()),
                logical_bounds: Rect {
                    origin: Point::default(),
                    size: Size {
                        width: Dip(1920.0),
                        height: Dip(1080.0),
                    },
                },
                scale_factor: ScaleFactor::default(),
            }],
            capabilities: Capabilities::new(),
            scripted_events: VecDeque::new(),
        }
    }
}

impl HeadlessBackend {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn max_iterations(mut self, value: usize) -> Self {
        self.max_iterations = value.max(1);
        self
    }

    pub fn paths(mut self, paths: Result<Paths, PlatformError>) -> Self {
        self.paths = paths;
        self
    }

    pub fn outputs(mut self, outputs: Vec<Output>) -> Self {
        self.outputs = outputs;
        self
    }

    pub fn capabilities(mut self, capabilities: Capabilities) -> Self {
        self.capabilities = capabilities;
        self
    }

    /// Adds a deterministic native-style event after initial window creation.
    pub fn event(mut self, event: PlatformEvent) -> Self {
        self.scripted_events.push_back(event);
        self
    }
}

pub struct HeadlessHost {
    id: WindowId,
    size: Cell<Size>,
    scale_factor: Cell<ScaleFactor>,
    redraws: Cell<u32>,
}

impl HeadlessHost {
    pub fn redraw_count(&self) -> u32 {
        self.redraws.get()
    }
}

impl Host for HeadlessHost {
    fn id(&self) -> WindowId {
        self.id
    }
    fn size(&self) -> Size {
        self.size.get()
    }
    fn scale_factor(&self) -> ScaleFactor {
        self.scale_factor.get()
    }
    fn surface_target(&self) -> SurfaceTarget {
        spi::headless_surface_target()
    }
    fn request_redraw(&self) -> Result<(), PlatformError> {
        self.redraws.set(self.redraws.get().saturating_add(1));
        Ok(())
    }
}

struct HeadlessContext {
    next_id: u64,
    hosts: HashMap<WindowId, HeadlessHost>,
    delivered_redraws: HashMap<WindowId, u32>,
    events: VecDeque<PlatformEvent>,
    paths: Result<Paths, PlatformError>,
    outputs: Vec<Output>,
    capabilities: Capabilities,
    tasks: Arc<Mutex<VecDeque<UiTask>>>,
}

impl HeadlessContext {
    fn update_for_event(&mut self, event: &PlatformEvent) {
        match event {
            PlatformEvent::WindowResized { window, size } => {
                if let Some(host) = self.hosts.get(window) {
                    host.size.set(*size);
                }
            }
            PlatformEvent::ScaleFactorChanged {
                window,
                size,
                scale_factor,
            } => {
                if let Some(host) = self.hosts.get(window) {
                    host.size.set(*size);
                    host.scale_factor.set(*scale_factor);
                }
            }
            PlatformEvent::OutputsChanged(outputs) => self.outputs.clone_from(outputs),
            _ => {}
        }
    }

    fn drain_tasks(&self) -> Result<usize, PlatformError> {
        let tasks = {
            let mut queue = self.tasks.lock().map_err(|_| {
                PlatformError::Backend("headless UI task queue was poisoned".into())
            })?;
            queue.drain(..).collect::<Vec<_>>()
        };
        let count = tasks.len();
        for task in tasks {
            task.run();
        }
        Ok(count)
    }
}

impl AppContext for HeadlessContext {
    fn host(&self, window: WindowId) -> Option<&dyn Host> {
        self.hosts.get(&window).map(|host| host as &dyn Host)
    }

    fn window_ids(&self) -> Vec<WindowId> {
        let mut ids = self.hosts.keys().copied().collect::<Vec<_>>();
        ids.sort_by_key(|id| id.0.value());
        ids
    }

    fn create_window(&mut self, options: WindowOptions) -> Result<WindowId, PlatformError> {
        let id = WindowId(Id::new(self.next_id));
        self.next_id = self.next_id.saturating_add(1);
        self.hosts.insert(
            id,
            HeadlessHost {
                id,
                size: Cell::new(options.size),
                scale_factor: Cell::new(ScaleFactor::default()),
                redraws: Cell::new(0),
            },
        );
        self.delivered_redraws.insert(id, 0);
        self.events.push_back(PlatformEvent::WindowCreated(id));
        Ok(id)
    }

    fn destroy_window(&mut self, window: WindowId) -> Result<(), PlatformError> {
        self.hosts
            .remove(&window)
            .ok_or(PlatformError::UnknownWindow(window))?;
        self.delivered_redraws.remove(&window);
        self.events
            .push_back(PlatformEvent::WindowDestroyed(window));
        Ok(())
    }

    fn paths(&self) -> Result<&Paths, PlatformError> {
        self.paths.as_ref().map_err(Clone::clone)
    }

    fn outputs(&self) -> &[Output] {
        &self.outputs
    }
    fn capabilities(&mut self) -> &mut Capabilities {
        &mut self.capabilities
    }

    fn ui_task_poster(&self) -> UiTaskPoster {
        let tasks = Arc::clone(&self.tasks);
        UiTaskPoster::new(move |task| {
            tasks
                .lock()
                .map_err(|_| PlatformError::Backend("headless UI task queue was poisoned".into()))?
                .push_back(task);
            Ok(())
        })
    }
}

impl Backend for HeadlessBackend {
    fn run(self, options: WindowOptions, app: &mut dyn AppLoop) -> Result<(), PlatformError> {
        let max_iterations = self.max_iterations;
        let initial_outputs = self.outputs.clone();
        let mut context = HeadlessContext {
            next_id: self.next_id,
            hosts: HashMap::new(),
            delivered_redraws: HashMap::new(),
            events: VecDeque::new(),
            paths: self.paths,
            outputs: self.outputs,
            capabilities: self.capabilities,
            tasks: Arc::new(Mutex::new(VecDeque::new())),
        };
        context.create_window(options)?;
        context.events.extend(self.scripted_events);
        context
            .events
            .push_back(PlatformEvent::OutputsChanged(initial_outputs));

        for _ in 0..max_iterations {
            context.drain_tasks()?;
            if let Some(event) = context.events.pop_front() {
                context.update_for_event(&event);
                let close_requested = match &event {
                    PlatformEvent::CloseRequested(window) => Some(*window),
                    _ => None,
                };
                let control = app.event(&mut context, event);
                if apply_control(&mut context, control)? {
                    return Ok(());
                }
                if let Some(window) = close_requested {
                    if context.hosts.contains_key(&window) {
                        context.destroy_window(window)?;
                    }
                }
                continue;
            }

            let redraw = context.window_ids().into_iter().find(|window| {
                let delivered = context.delivered_redraws.get(window).copied().unwrap_or(0);
                context
                    .hosts
                    .get(window)
                    .is_some_and(|host| host.redraw_count() > delivered)
            });
            if let Some(window) = redraw {
                let delivered = context.delivered_redraws.entry(window).or_default();
                *delivered = delivered.saturating_add(1);
                let control = app.event(&mut context, PlatformEvent::RedrawRequested(window));
                if apply_control(&mut context, control)? {
                    return Ok(());
                }
                continue;
            }

            if context.hosts.is_empty() {
                return Ok(());
            }
            let before = context
                .hosts
                .values()
                .map(HeadlessHost::redraw_count)
                .sum::<u32>();
            let control = app.event(&mut context, PlatformEvent::AboutToWait);
            if apply_control(&mut context, control)? {
                return Ok(());
            }
            context.drain_tasks()?;
            let after = context
                .hosts
                .values()
                .map(HeadlessHost::redraw_count)
                .sum::<u32>();
            if context.events.is_empty() && before == after {
                return Ok(());
            }
        }

        Err(PlatformError::Backend(format!(
            "headless event loop exceeded {max_iterations} iterations"
        )))
    }
}

fn apply_control(
    context: &mut HeadlessContext,
    control: LoopControl,
) -> Result<bool, PlatformError> {
    match control {
        LoopControl::Continue => Ok(false),
        LoopControl::RequestRedraw(window) | LoopControl::WaitUntil { window, .. } => {
            context
                .host(window)
                .ok_or(PlatformError::UnknownWindow(window))?
                .request_redraw()?;
            Ok(false)
        }
        LoopControl::Exit => Ok(true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use zui_platform::SurfaceTargetKind;

    #[derive(Default)]
    struct RecordingLoop {
        events: Vec<PlatformEvent>,
    }

    impl AppLoop for RecordingLoop {
        fn event(&mut self, context: &mut dyn AppContext, event: PlatformEvent) -> LoopControl {
            self.events.push(event.clone());
            match event {
                PlatformEvent::WindowCreated(window) => {
                    assert_eq!(
                        context.host(window).unwrap().size(),
                        WindowOptions::default().size
                    );
                    LoopControl::RequestRedraw(window)
                }
                _ => LoopControl::Continue,
            }
        }
    }

    #[test]
    fn drives_the_public_app_loop_lifecycle() {
        let mut app = RecordingLoop::default();
        HeadlessBackend::new()
            .run(WindowOptions::default(), &mut app)
            .unwrap();
        assert!(app
            .events
            .iter()
            .any(|event| matches!(event, PlatformEvent::WindowCreated(_))));
        assert!(app
            .events
            .iter()
            .any(|event| matches!(event, PlatformEvent::RedrawRequested(_))));
        assert!(app
            .events
            .iter()
            .any(|event| matches!(event, PlatformEvent::OutputsChanged(_))));
    }

    #[test]
    fn creates_drives_and_destroys_two_hosts() {
        #[derive(Default)]
        struct Multi {
            first: Option<WindowId>,
            second: Option<WindowId>,
            redrawn: Vec<WindowId>,
            destroyed: Vec<WindowId>,
        }
        impl AppLoop for Multi {
            fn event(&mut self, context: &mut dyn AppContext, event: PlatformEvent) -> LoopControl {
                match event {
                    PlatformEvent::WindowCreated(id) if self.first.is_none() => {
                        self.first = Some(id);
                        self.second = Some(
                            context
                                .create_window(WindowOptions {
                                    title: "second".into(),
                                    ..WindowOptions::default()
                                })
                                .unwrap(),
                        );
                        return LoopControl::RequestRedraw(id);
                    }
                    PlatformEvent::WindowCreated(id) if Some(id) == self.second => {
                        assert_eq!(context.window_ids().len(), 2);
                        return LoopControl::RequestRedraw(id);
                    }
                    PlatformEvent::RedrawRequested(id) => {
                        self.redrawn.push(id);
                        if self.redrawn.len() == 2 {
                            context.destroy_window(self.first.unwrap()).unwrap();
                            context.destroy_window(self.second.unwrap()).unwrap();
                        }
                    }
                    PlatformEvent::WindowDestroyed(id) => {
                        assert!(context.host(id).is_none());
                        self.destroyed.push(id);
                    }
                    _ => {}
                }
                LoopControl::Continue
            }
        }
        let mut app = Multi::default();
        HeadlessBackend::new()
            .run(WindowOptions::default(), &mut app)
            .unwrap();
        assert_eq!(app.redrawn.len(), 2);
        assert_eq!(app.destroyed.len(), 2);
    }

    #[test]
    fn rejects_an_unbounded_immediate_redraw_loop() {
        struct RedrawLoop;
        impl AppLoop for RedrawLoop {
            fn event(
                &mut self,
                _context: &mut dyn AppContext,
                event: PlatformEvent,
            ) -> LoopControl {
                match event {
                    PlatformEvent::WindowCreated(window)
                    | PlatformEvent::RedrawRequested(window) => LoopControl::RequestRedraw(window),
                    _ => LoopControl::Continue,
                }
            }
        }

        let error = HeadlessBackend::new()
            .max_iterations(6)
            .run(WindowOptions::default(), &mut RedrawLoop)
            .expect_err("an unbounded redraw loop must fail deterministically");
        assert!(error.to_string().contains("exceeded 6 iterations"));
    }

    #[test]
    fn resize_scale_outputs_paths_and_unsupported_capabilities_are_observable() {
        let window = WindowId(Id::new(1));
        let size = Size {
            width: Dip(640.0),
            height: Dip(480.0),
        };
        struct Contract {
            saw_resize: bool,
            saw_scale: bool,
            saw_outputs: bool,
        }
        impl AppLoop for Contract {
            fn event(&mut self, context: &mut dyn AppContext, event: PlatformEvent) -> LoopControl {
                assert!(context.paths().is_ok());
                assert!(context.capabilities().clipboard().is_none());
                match event {
                    PlatformEvent::WindowResized { window, size } => {
                        self.saw_resize = context.host(window).unwrap().size() == size;
                    }
                    PlatformEvent::ScaleFactorChanged {
                        window,
                        size,
                        scale_factor,
                    } => {
                        let host = context.host(window).unwrap();
                        self.saw_scale = host.size() == size && host.scale_factor() == scale_factor;
                    }
                    PlatformEvent::OutputsChanged(outputs) => {
                        self.saw_outputs = context.outputs() == outputs;
                    }
                    _ => {}
                }
                LoopControl::Continue
            }
        }
        let mut app = Contract {
            saw_resize: false,
            saw_scale: false,
            saw_outputs: false,
        };
        HeadlessBackend::new()
            .event(PlatformEvent::WindowResized { window, size })
            .event(PlatformEvent::ScaleFactorChanged {
                window,
                size,
                scale_factor: ScaleFactor(2.0),
            })
            .run(WindowOptions::default(), &mut app)
            .unwrap();
        assert!(app.saw_resize && app.saw_scale && app.saw_outputs);
    }

    #[test]
    fn injected_path_failure_remains_structured_through_app_context() {
        struct PathsLoop;
        impl AppLoop for PathsLoop {
            fn event(
                &mut self,
                context: &mut dyn AppContext,
                _event: PlatformEvent,
            ) -> LoopControl {
                assert!(matches!(
                    context.paths(),
                    Err(PlatformError::PathUnavailable {
                        variable: "HEADLESS_PATHS",
                        ..
                    })
                ));
                LoopControl::Exit
            }
        }
        HeadlessBackend::new()
            .paths(Err(PlatformError::PathUnavailable {
                variable: "HEADLESS_PATHS",
                reason: "injected contract failure".into(),
            }))
            .run(WindowOptions::default(), &mut PathsLoop)
            .unwrap();
    }

    #[test]
    fn posted_tasks_run_on_the_event_loop_thread() {
        let ran = Arc::new(AtomicBool::new(false));
        struct PostOnce {
            ran: Arc<AtomicBool>,
        }
        impl AppLoop for PostOnce {
            fn event(&mut self, context: &mut dyn AppContext, event: PlatformEvent) -> LoopControl {
                if matches!(event, PlatformEvent::WindowCreated(_)) {
                    let ran = Arc::clone(&self.ran);
                    context
                        .ui_task_poster()
                        .post(move || ran.store(true, Ordering::SeqCst))
                        .unwrap();
                }
                LoopControl::Continue
            }
        }
        HeadlessBackend::new()
            .run(
                WindowOptions::default(),
                &mut PostOnce {
                    ran: Arc::clone(&ran),
                },
            )
            .unwrap();
        assert!(ran.load(Ordering::SeqCst));
    }

    #[test]
    fn headless_target_runs_the_shared_surface_lifecycle_contract() {
        let host = HeadlessHost {
            id: WindowId(Id::new(1)),
            size: Cell::new(WindowOptions::default().size),
            scale_factor: Cell::new(ScaleFactor::default()),
            redraws: Cell::new(0),
        };
        let target = host.surface_target();
        assert_eq!(target.kind(), SurfaceTargetKind::Headless);
        assert!(target.native_source().is_none());
        zui_render::test_support::assert_surface_lifecycle_contract();
    }
}
