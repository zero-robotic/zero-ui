use zui_core::{Point, WindowId};
use zui_platform::{InputEvent, KeyState, MouseButton};

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
    dirty_region: Option<zui_core::Rect>,
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
        self.dirty_region = Some(match self.dirty_region {
            Some(current) => union_rect(current, region),
            None => region,
        });
    }
    pub fn take_dirty_region(&mut self) -> Option<zui_core::Rect> {
        self.dirty_region.take()
    }
}

fn union_rect(a: zui_core::Rect, b: zui_core::Rect) -> zui_core::Rect {
    let left = a.origin.x.0.min(b.origin.x.0);
    let top = a.origin.y.0.min(b.origin.y.0);
    let right = (a.origin.x.0 + a.size.width.0).max(b.origin.x.0 + b.size.width.0);
    let bottom = (a.origin.y.0 + a.size.height.0).max(b.origin.y.0 + b.size.height.0);
    zui_core::Rect {
        origin: zui_core::Point {
            x: zui_core::Dip(left),
            y: zui_core::Dip(top),
        },
        size: zui_core::Size {
            width: zui_core::Dip(right - left),
            height: zui_core::Dip(bottom - top),
        },
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
