//! Declarative application assembly, independent of concrete controls and services.
use crate::{Application, ApplicationError};
use std::time::Duration;
use zui_api::{
    action::ActionEvent, layout::LayoutEngine, text::TextService, view::View, widget::Invalidation,
};
use zui_core::{Tree, TreeError, TreeLimits};
use zui_paint::Scene;
use zui_platform::{
    ImeRequest, ImeSessionId, InputEvent, NativeWindow, WindowAction, WindowEvent, WindowMetrics,
};
use zui_types::{Color, NodeId};

/// One-shot application intent resolved against the current committed tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewFocusRequest {
    Focus(NodeId),
    Clear,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ViewAction {
    pub window: WindowAction,
    /// State affecting the View changed. Multiple updates coalesce before drawing.
    pub rebuild: bool,
}
impl ViewAction {
    pub fn rebuild() -> Self {
        Self {
            window: WindowAction::redraw(),
            rebuild: true,
        }
    }
    pub fn exit() -> Self {
        Self {
            window: WindowAction::exit(),
            rebuild: false,
        }
    }
}

/// Application-owned state and typed messages. Only unhandled window input is
/// offered to `update`; control actions are mapped after propagation finishes.
pub trait ViewApplication {
    type Message;
    fn view(&self) -> View;
    /// Called after reconciliation and successful layout, before input/drawing.
    /// Resolve pending intent using current keys/IDs and consume it when returning
    /// Some. None leaves focus unchanged. This may run multiple times per frame;
    /// do not change View-affecting model state here. Missing virtual rows may be
    /// retried by retaining intent, but this hook does not schedule another frame.
    /// An invalid request fails preparation; consumed intent is not restored.
    fn take_focus_request(&mut self, _tree: &Tree) -> Option<ViewFocusRequest> {
        None
    }
    fn map_action(&self, _action: &ActionEvent) -> Option<Self::Message> {
        None
    }
    fn message(&mut self, _message: Self::Message) -> ViewAction {
        ViewAction::default()
    }
    fn update(
        &mut self,
        event: WindowEvent,
        metrics: WindowMetrics,
        elapsed: Duration,
    ) -> ViewAction;
    fn connected(&mut self, _window: &dyn NativeWindow) {}
    fn disconnected(&mut self) {}
    fn background(&self) -> Color {
        Color::BLACK
    }
}

/// Owns the model, persistent tree, and injected layout/text services. Concrete
/// service construction and font registration belong to the caller.
pub struct WidgetHost<M, L, T> {
    model: M,
    tree: Tree,
    layout: L,
    text: T,
    rebuild: bool,
    ime: Option<ImeRequest>,
    clipboard: Box<dyn zui_api::clipboard::Clipboard>,
    next_frame: Option<Duration>,
}
impl<M: ViewApplication, L: LayoutEngine, T: TextService> WidgetHost<M, L, T> {
    pub fn new(model: M, layout: L, text: T, limits: TreeLimits) -> Result<Self, TreeError> {
        let tree = Tree::from_view(model.view(), limits)?;
        Ok(Self {
            model,
            tree,
            layout,
            text,
            rebuild: false,
            ime: None,
            clipboard: Box::new(zui_api::clipboard::UnavailableClipboard),
            next_frame: None,
        })
    }
    pub fn with_clipboard(
        mut self,
        clipboard: impl zui_api::clipboard::Clipboard + 'static,
    ) -> Self {
        self.clipboard = Box::new(clipboard);
        self
    }
    pub fn model(&self) -> &M {
        &self.model
    }
    /// Read-only runtime inspection for diagnostics; mutation remains coordinated.
    pub fn tree(&self) -> &Tree {
        &self.tree
    }
    fn prepare(&mut self, metrics: WindowMetrics) -> Result<(), TreeError> {
        if self.rebuild {
            if let Err(error) = self.tree.reconcile(self.model.view()) {
                self.ime = None;
                self.next_frame = None;
                return Err(error);
            }
            self.rebuild = false;
        }
        let result = (|| {
            self.tree
                .layout_with_text(&mut self.layout, &mut self.text, metrics.logical_size())?;
            if let Some(request) = self.model.take_focus_request(&self.tree) {
                self.tree.request_focus(match request {
                    ViewFocusRequest::Focus(id) => Some(id),
                    ViewFocusRequest::Clear => None,
                })?;
                // Focus notifications may change geometry. Finish that work before
                // input dispatch or IME queries without asking for another intent.
                if self.tree.invalidation() == Invalidation::Layout {
                    self.tree.layout_with_text(
                        &mut self.layout,
                        &mut self.text,
                        metrics.logical_size(),
                    )?;
                }
            }
            Ok(())
        })();
        if result.is_err() {
            self.ime = None;
            self.next_frame = None;
        }
        result
    }
    fn advance(&mut self, metrics: WindowMetrics, elapsed: Duration) -> Result<bool, TreeError> {
        self.next_frame = None;
        if !metrics.drawable() {
            return Ok(false);
        }
        let update = self.tree.advance_animations(elapsed)?;
        self.next_frame = update.next_frame;
        Ok(update.invalidation != Invalidation::None)
    }
    fn refresh_ime(&mut self, metrics: WindowMetrics) -> Result<(), TreeError> {
        if !metrics.drawable()
            || self
                .ime
                .is_some_and(|r| self.tree.focused().map(|id| id.0) != Some(r.session.owner))
        {
            self.ime = None;
        }
        // Keep the previous committed area while a new frame is pending. Never
        // force model reconciliation merely to query IME geometry.
        if metrics.drawable() && !self.rebuild && self.tree.invalidation() != Invalidation::Layout {
            self.ime = self.tree.text_input()?.map(|target| ImeRequest {
                session: ImeSessionId {
                    owner: target.id.0,
                    generation: target.generation,
                },
                cursor_area: target.cursor_area,
            });
        }
        Ok(())
    }
    fn apply(&mut self, response: ViewAction, action: &mut WindowAction) {
        self.rebuild |= response.rebuild;
        action.redraw |= response.window.redraw || response.rebuild;
        action.exit |= response.window.exit;
        action.wake_at = match (action.wake_at, response.window.wake_at) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
    }
}
impl<M: ViewApplication, L: LayoutEngine, T: TextService> Application for WidgetHost<M, L, T> {
    fn animation_deadline(&self) -> Option<Duration> {
        self.next_frame
    }
    fn animate(
        &mut self,
        metrics: WindowMetrics,
        elapsed: Duration,
    ) -> Result<bool, ApplicationError> {
        if metrics.drawable() {
            self.prepare(metrics)?;
        }
        let animated = self.advance(metrics, elapsed)?;
        self.refresh_ime(metrics)?;
        Ok(metrics.drawable() && (animated || self.tree.invalidation() != Invalidation::None))
    }
    fn connected(&mut self, window: &dyn NativeWindow) {
        self.ime = None;
        self.next_frame = None;
        self.model.connected(window);
        self.rebuild = true;
    }
    fn update(
        &mut self,
        event: WindowEvent,
        metrics: WindowMetrics,
        elapsed: Duration,
    ) -> Result<WindowAction, ApplicationError> {
        let metrics_changed = matches!(
            event,
            WindowEvent::Resumed | WindowEvent::Resized | WindowEvent::ScaleChanged
        );
        let mut action = WindowAction::default();
        if matches!(
            event,
            WindowEvent::Focused(false) | WindowEvent::Occluded(true)
        ) {
            self.tree.cancel_input();
            self.ime = None;
        }
        let mut handled = false;
        if let WindowEvent::Input(input) = &event {
            if !metrics.drawable() {
                self.tree.cancel_input();
                self.ime = None;
                self.next_frame = None;
                return Ok(action);
            }
            // Preserve touch as an unhandled application event until the core's
            // multi-contact path is implemented; do not emulate a mouse pointer.
            if !matches!(input, InputEvent::Touch { .. }) {
                self.prepare(metrics)?;
                // Reconciliation may replace the editor or reset its text before
                // this queued native event arrives. Do not deliver composition data
                // until the new session has been published to the window driver.
                let obsolete_ime = if matches!(
                    input,
                    InputEvent::ImePreedit { .. } | InputEvent::ImeCommit(_)
                ) {
                    let target = self.tree.text_input()?;
                    !target.is_some_and(|target| {
                        self.ime.is_some_and(|request| {
                            request.session.owner == target.id.0
                                && request.session.generation == target.generation
                        })
                    })
                } else {
                    false
                };
                if obsolete_ime {
                    handled = true;
                } else {
                    let result = self.tree.dispatch_input_with_services(
                        input.clone(),
                        &mut zui_api::event::EventServices {
                            clipboard: self.clipboard.as_mut(),
                        },
                    )?;
                    handled = result.handled;
                    action.redraw |= result.invalidation != Invalidation::None;
                    for event in result.actions {
                        if let Some(message) = self.model.map_action(&event) {
                            let response = self.model.message(message);
                            self.apply(response, &mut action);
                        }
                    }
                }
            }
        }
        if !handled {
            let response = self.model.update(event, metrics, elapsed);
            self.apply(response, &mut action);
        }
        action.redraw |= self.advance(metrics, elapsed)?;
        action.redraw |=
            self.rebuild || metrics_changed || self.tree.invalidation() != Invalidation::None;
        self.refresh_ime(metrics)?;
        Ok(action)
    }
    fn draw(
        &mut self,
        scene: &mut Scene,
        metrics: WindowMetrics,
        elapsed: Duration,
    ) -> Result<(), ApplicationError> {
        if !metrics.drawable() {
            self.ime = None;
            self.next_frame = None;
            return Ok(());
        }
        self.prepare(metrics)?;
        self.advance(metrics, elapsed)?;
        if self.tree.invalidation() == Invalidation::Layout {
            self.prepare(metrics)?;
        }
        self.refresh_ime(metrics)?;
        // Replace the caller's scene only after reconciliation, layout, and
        // preparation all succeed. The driver will not present on an error.
        *scene = self.tree.scene()?;
        Ok(())
    }
    fn background(&self) -> Color {
        self.model.background()
    }
    fn ime_request(&self) -> Option<ImeRequest> {
        self.ime
    }
    fn disconnected(&mut self) {
        self.ime = None;
        self.next_frame = None;
        self.tree.cancel_input();
        self.model.disconnected();
        self.rebuild = true;
    }
}
