use crate::{WidgetId, WidgetRuntimeTable};
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
    SelectionChanged,
    FocusRequested,
    TextChanged,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Action {
    pub source: crate::WidgetId,
    pub kind: ActionKind,
}

enum RuntimeStorage<'a> {
    Borrowed(&'a mut WidgetRuntimeTable),
    Detached(WidgetRuntimeTable),
}

impl RuntimeStorage<'_> {
    fn table(&self) -> &WidgetRuntimeTable {
        match self {
            Self::Borrowed(table) => table,
            Self::Detached(table) => table,
        }
    }

    fn table_mut(&mut self) -> &mut WidgetRuntimeTable {
        match self {
            Self::Borrowed(table) => table,
            Self::Detached(table) => table,
        }
    }

    fn is_tree_owned(&self) -> bool {
        matches!(self, Self::Borrowed(_))
    }
}

pub struct EventContext<'a> {
    actions: Vec<Action>,
    scene_update: SceneUpdate,
    runtime: RuntimeStorage<'a>,
    focus_changed: Vec<WidgetId>,
}

impl EventContext<'static> {
    /// Standalone context for direct widget tests and embeddings that do not
    /// own a WidgetTree. Tree dispatch uses `with_runtime` instead.
    pub fn new() -> Self {
        Self {
            actions: Vec::new(),
            scene_update: SceneUpdate::default(),
            runtime: RuntimeStorage::Detached(WidgetRuntimeTable::default()),
            focus_changed: Vec::new(),
        }
    }
}

impl<'a> EventContext<'a> {
    pub(crate) fn with_runtime(runtime: &'a mut WidgetRuntimeTable) -> Self {
        Self {
            actions: Vec::new(),
            scene_update: SceneUpdate::default(),
            runtime: RuntimeStorage::Borrowed(runtime),
            focus_changed: Vec::new(),
        }
    }
    pub fn emit(&mut self, source: WidgetId, kind: ActionKind) {
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
    pub fn invalidate_widget(&mut self, widget: WidgetId, region: zui_core::Rect) {
        self.runtime.table_mut().mark_dirty(widget, region);
        self.scene_update.invalidate_node(widget.value(), region);
    }

    pub fn is_hovered(&self, widget: WidgetId) -> bool {
        self.runtime.table().is_hovered(widget)
    }

    pub fn set_hovered(&mut self, widget: WidgetId, hovered: bool) -> bool {
        self.runtime.table_mut().set_hovered(widget, hovered)
    }

    pub fn is_focused(&self, widget: WidgetId) -> bool {
        self.runtime.table().is_focused(widget)
    }

    /// Whether focus comes from a WidgetTree-owned runtime table. Detached
    /// contexts retain the direct-widget API used by embedders and tests.
    pub fn has_tree_runtime(&self) -> bool {
        self.runtime.is_tree_owned()
    }

    pub fn request_focus(&mut self, widget: WidgetId) -> bool {
        let changed = !self.is_focused(widget);
        self.focus_changed
            .extend(self.runtime.table_mut().clear_focus_except(widget));
        changed
    }

    pub(crate) fn take_focus_changed(&mut self) -> Vec<WidgetId> {
        std::mem::take(&mut self.focus_changed)
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
