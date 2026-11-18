//! Optional state-driven application adapter. Core and controls only see the
//! existing View and opaque action contracts.
mod events;
mod tasks;
pub use tasks::{PostError, TaskSender, Tasks};
mod region;
use crate::{ViewAction, ViewApplication, ViewUpdate};
pub use events::{Binding, Event, Events};
pub use region::{Region, RegionChanges};
use std::{
    cell::{Cell, RefCell},
    rc::Weak,
    time::Duration,
};
use zui_api::{action::ActionEvent, view::View};
use zui_platform::{NativeWindow, WindowEvent, WindowMetrics};
use zui_state::{Observer, Scope};
use zui_types::Color;

/// A component creates its state once, and describes its UI from that state.
pub trait Component: Sized + 'static {
    fn view(&self) -> View;
    fn update(&mut self, event: WindowEvent, _: WindowMetrics, _: Duration) -> ViewAction {
        if event == WindowEvent::CloseRequested {
            ViewAction::exit()
        } else {
            ViewAction::default()
        }
    }
    fn connected(&mut self, _window: &dyn NativeWindow) {}
    fn disconnected(&mut self) {}
    fn background(&self) -> Color {
        Color::BLACK
    }
}

/// Opt-in refresh explanation; collecting diagnostics allocates, normal dirty
/// checks do not build this report.
#[derive(Clone, Debug)]
pub struct RefreshDiagnostics {
    pub root: Vec<zui_state::StateChange>,
    pub regions: Vec<RegionChanges>,
    pub forced_root: bool,
    pub subscriptions: usize,
}

/// Compatibility adapter using the existing WidgetHost and renderer pipeline.
/// State changes are detected automatically; explicit rebuild remains available
/// for ordinary business fields that are not observable properties.
pub struct ReactiveApplication<M> {
    model: M,
    events: Events<M>,
    mailbox: tasks::Mailbox<M>,
    observer: RefCell<Observer>,
    _scope: Scope,
    force_root: Cell<bool>,
    regions: RefCell<Vec<Weak<region::RegionInner>>>,
}
impl<M: Component> ReactiveApplication<M> {
    pub fn new(create: impl FnOnce(&Scope, &mut Events<M>) -> M) -> Self {
        Self::with_tasks(|scope, events, _tasks| create(scope, events))
    }
    pub fn with_tasks(create: impl FnOnce(&Scope, &mut Events<M>, Tasks<M>) -> M) -> Self {
        let scope = Scope::new();
        let mut events = Events::new(&scope);
        let (mailbox, tasks) = tasks::Mailbox::new(&scope);
        let model = create(&scope, &mut events, tasks);
        Self {
            model,
            events,
            mailbox,
            observer: RefCell::new(Observer::new()),
            _scope: scope,
            force_root: Cell::new(false),
            regions: RefCell::new(Vec::new()),
        }
    }
    pub fn pending_refresh(&self) -> RefreshDiagnostics {
        let mut regions = Vec::new();
        region::changes(&self.regions.borrow(), &mut regions);
        RefreshDiagnostics {
            root: self.observer.borrow().changes(),
            regions,
            forced_root: self.force_root.get(),
            subscriptions: self.events.subscription_count(),
        }
    }
    pub fn component(&self) -> &M {
        &self.model
    }
}
impl<M: Component> ViewApplication for ReactiveApplication<M> {
    type Message = ();
    fn view(&self) -> View {
        let (view, regions) =
            region::collect(|| self.observer.borrow_mut().observe(|| self.model.view()));
        *self.regions.borrow_mut() = regions;
        self.force_root.set(false);
        view
    }
    fn view_is_dirty(&self) -> bool {
        self.observer.borrow().is_dirty() || region::is_dirty(&self.regions.borrow())
    }
    fn view_update(&self, tree: &zui_core::Tree) -> ViewUpdate {
        let mut dirty = Vec::new();
        region::dirty(&self.regions.borrow(), &mut dirty);
        if !self.force_root.get()
            && !self.observer.borrow().is_dirty()
            && dirty.len() == 1
            && let Some(root) = region::locate(tree, &dirty[0])
        {
            let view = dirty[0].render();
            region::validate_mounts(&self.regions.borrow());
            return ViewUpdate::Subtree { root, view };
        }
        // Multiple independently dirty branches or explicit non-observable
        // changes conservatively rebuild the root using the existing contract.
        ViewUpdate::Root(self.view())
    }
    fn action(&mut self, event: &ActionEvent, _: &zui_core::Tree) -> ViewAction {
        let response = self.events.dispatch(&mut self.model, event);
        self.force_root
            .set(self.force_root.get() || response.rebuild);
        response
    }
    fn update(
        &mut self,
        event: WindowEvent,
        metrics: WindowMetrics,
        elapsed: Duration,
    ) -> ViewAction {
        let mut response = self.mailbox.drain(&mut self.model);
        if !response.window.exit {
            tasks::merge(&mut response, self.model.update(event, metrics, elapsed));
        }
        self.force_root
            .set(self.force_root.get() || response.rebuild);
        response
    }
    fn connected(&mut self, window: &dyn NativeWindow) {
        self.mailbox.connect(window.event_waker());
        self.model.connected(window);
        self.force_root.set(true);
    }
    fn disconnected(&mut self) {
        self.mailbox.disconnect();
        self.model.disconnected();
    }
    fn background(&self) -> Color {
        self.model.background()
    }
}
