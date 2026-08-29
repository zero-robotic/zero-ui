use zui_core::{Point, WindowId};
use zui_platform::{InputEvent, KeyState, MouseButton};
pub use zui_render::SceneUpdate;

#[derive(Clone, Debug, PartialEq)]
pub struct UiEvent {
    pub window: Option<WindowId>,
    pub position: Option<Point>,
    pub input: InputEvent,
}

impl UiEvent {
    pub fn input(input: InputEvent) -> Self {
        Self {
            window: None,
            position: None,
            input,
        }
    }
    pub fn pointer(window: Option<WindowId>, position: Point, input: InputEvent) -> Self {
        Self {
            window,
            position: Some(position),
            input,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventResult {
    Ignored,
    Handled,
    RequestRedraw,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionKind {
    Clicked,
    CheckedChanged,
    FocusRequested,
    TextChanged,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Action {
    pub source: crate::WidgetId,
    pub kind: ActionKind,
}

#[derive(Default)]
pub struct EventContext {
    actions: Vec<Action>,
    scene_update: SceneUpdate,
}

impl EventContext {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn emit(&mut self, source: crate::WidgetId, kind: ActionKind) {
        self.actions.push(Action { source, kind });
    }
    pub fn actions(&self) -> &[Action] {
        &self.actions
    }
    pub fn take_actions(&mut self) -> Vec<Action> {
        std::mem::take(&mut self.actions)
    }
    pub fn invalidate(&mut self, region: zui_core::Rect) {
        self.scene_update.add_damage(region);
        self.scene_update.request_full_rebuild();
    }
    pub fn invalidate_widget(&mut self, widget: crate::WidgetId, region: zui_core::Rect) {
        self.scene_update.invalidate_node(widget.0, region);
    }
    pub fn request_full_redraw(&mut self) {
        self.scene_update.request_full_rebuild();
    }
    pub fn take_scene_update(&mut self) -> SceneUpdate {
        std::mem::take(&mut self.scene_update)
    }
}

pub(crate) fn is_left_press(event: &UiEvent) -> bool {
    matches!(
        event.input,
        InputEvent::MouseInput {
            button: MouseButton::Left,
            state: KeyState::Pressed
        }
    )
}
