mod virtualized;
// Controlled hierarchical selection, composed over the ordinary list behavior.
use crate::{
    Icon, IconKind, ListBox, ListBoxView, ListItem, ListItemId, ListMultiSelectionChanged,
    ListSelectionChanged,
};
use alloc::{collections::BTreeSet, string::String, vec::Vec};
use core::time::Duration;
use zui_api::{
    action::{ActionId, WidgetAction},
    event::*,
    layout::LayoutError,
    view::{View, ViewSpec},
    widget::*,
};
use zui_paint::Painter;
use zui_theme::Theme;
use zui_types::Size;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TreeItemId(pub u64);
#[derive(Clone, Debug, PartialEq)]
pub struct TreeItem {
    /// Unloaded/Loading/Failed nodes retain an expansion affordance without children.
    pub children_state: TreeChildrenState,
    pub id: TreeItemId,
    pub label: String,
    pub disabled: bool,
    pub children: Vec<TreeItem>,
}
impl TreeItem {
    pub fn new(id: TreeItemId, label: impl Into<String>) -> Self {
        Self {
            id,
            children_state: TreeChildrenState::Loaded,
            label: label.into(),
            disabled: false,
            children: Vec::new(),
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TreeChildrenState {
    #[default]
    Loaded,
    Unloaded,
    Loading,
    Failed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TreeLoadRequested {
    pub id: TreeItemId,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeSelectionChanged {
    pub selected: BTreeSet<TreeItemId>,
}
/// Application validates business rules and commits the move; no model mutation occurs here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeDropRequested {
    pub sources: Vec<TreeItemId>,
    pub target: TreeItemId,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeEvent {
    Selected(TreeItemId),
    Expansion { id: TreeItemId, expanded: bool },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TreeViewError {
    DuplicateId(TreeItemId),
    UnknownId(TreeItemId),
    TooDeep,
    InvalidViewport,
    VirtualDragUnsupported,
}
impl core::fmt::Display for TreeViewError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "tree view error: {self:?}")
    }
}
impl core::error::Error for TreeViewError {}
#[derive(Clone)]
pub struct TreeView {
    pub items: Vec<TreeItem>,
    pub expanded: BTreeSet<TreeItemId>,
    pub selected: Option<TreeItemId>,
    pub list: ListBoxView,
    pub selection: Option<BTreeSet<TreeItemId>>,
    pub draggable: bool,
}
#[derive(Clone, PartialEq)]
struct Row {
    id: TreeItemId,
    parent: Option<TreeItemId>,
    branch: bool,
    expanded: bool,
    disabled: bool,
    load: bool,
}
impl TreeView {
    pub fn themed(
        theme: &Theme,
        items: Vec<TreeItem>,
        families: Vec<String>,
        selected: Option<TreeItemId>,
        action: ActionId,
    ) -> Self {
        Self {
            items,
            expanded: BTreeSet::new(),
            selection: None,
            draggable: false,
            selected,
            list: ListBoxView::themed(theme, Vec::new(), families, None, action),
        }
    }
    pub fn build(&self) -> Result<View, TreeViewError> {
        Ok(View::new(self.specification()?))
    }
    /// Fixed-height realized rows with persistent tree focus and logical keyboard navigation.
    /// Dragging requires the ordinary tree; virtual drop auto-scroll is not supported.
    pub fn build_virtual(
        &self,
        row_height: f64,
        viewport_height: f32,
    ) -> Result<View, TreeViewError> {
        virtualized::build(self, row_height, viewport_height)
    }
    fn specification(&self) -> Result<TreeSpec, TreeViewError> {
        let mut ids = BTreeSet::new();
        let mut pending: Vec<_> = self
            .items
            .iter()
            .rev()
            .map(|i| (i, None, 0usize, true, false))
            .collect();
        let mut rows = Vec::new();
        let mut items = Vec::new();
        while let Some((item, parent, depth, visible, ancestor_disabled)) = pending.pop() {
            if depth > 64 {
                return Err(TreeViewError::TooDeep);
            }
            if !ids.insert(item.id) {
                return Err(TreeViewError::DuplicateId(item.id));
            }
            let disabled = ancestor_disabled || item.disabled;
            let expanded = self.expanded.contains(&item.id);
            if visible {
                rows.push(Row {
                    id: item.id,
                    parent,
                    branch: !item.children.is_empty()
                        || item.children_state != TreeChildrenState::Loaded,
                    load: matches!(
                        item.children_state,
                        TreeChildrenState::Unloaded | TreeChildrenState::Failed
                    ),
                    expanded,
                    disabled,
                });
                let mut row = ListItem::new(ListItemId(item.id.0), item.label.clone());
                row.disabled = disabled;
                row.indent = depth as f32 * 20.0;
                row.leading_icon = (!item.children.is_empty()
                    || item.children_state != TreeChildrenState::Loaded)
                    .then(|| {
                        Icon::builtin(if expanded {
                            IconKind::ChevronDown
                        } else {
                            IconKind::ChevronRight
                        })
                    });
                items.push(row);
            }
            pending.extend(
                item.children
                    .iter()
                    .rev()
                    .map(|i| (i, Some(item.id), depth + 1, visible && expanded, disabled)),
            );
        }
        for id in self
            .selected
            .iter()
            .chain(self.expanded.iter())
            .chain(self.selection.iter().flatten())
        {
            if !ids.contains(id) {
                return Err(TreeViewError::UnknownId(*id));
            }
        }
        let mut list = self.list.clone();
        list.items = items;
        list.multi_selection = self.selection.as_ref().map(|ids| {
            ids.iter()
                .filter(|id| rows.iter().any(|r| r.id == **id))
                .map(|id| ListItemId(id.0))
                .collect()
        });
        list.activation_only = false;
        list.selected = self
            .selected
            .filter(|id| rows.iter().any(|r| r.id == *id))
            .map(|id| ListItemId(id.0));
        Ok(TreeSpec {
            list,
            rows,
            selection: self.selection.clone(),
            draggable: self.draggable,
            global_rows: None,
        })
    }
}
#[derive(Clone)]
struct TreeSpec {
    list: ListBoxView,
    rows: Vec<Row>,
    selection: Option<BTreeSet<TreeItemId>>,
    draggable: bool,
    global_rows: Option<alloc::rc::Rc<Vec<Row>>>,
}
struct TreeWidget {
    properties: TreeSpec,
    list: ListBox,
    armed: Option<TreeItemId>,
    drag: Option<(TreeItemId, zui_types::Point, bool)>,
    label_press: Option<(TreeItemId, zui_types::Point)>,
    last_click: Option<(TreeItemId, zui_types::Point, Option<Duration>)>,
}
impl ViewSpec for TreeSpec {
    type Widget = TreeWidget;
    fn build(&self) -> TreeWidget {
        TreeWidget {
            properties: self.clone(),
            list: self.list.build(),
            armed: None,
            drag: None,
            label_press: None,
            last_click: None,
        }
    }
    fn update(&self, w: &mut TreeWidget) -> Invalidation {
        if self.rows != w.properties.rows
            || self.list.disabled
            || self.list.action != w.properties.list.action
        {
            w.armed = None;
            w.drag = None;
            w.label_press = None;
            w.last_click = None;
        }
        let result = self.list.update(&mut w.list);
        w.properties = self.clone();
        result
    }
}
impl Widget for TreeWidget {
    fn animate(&mut self, elapsed: Duration) -> AnimationUpdate {
        let mut next_frame = None;
        if let Some((_, _, deadline)) = &mut self.last_click {
            let end = *deadline.get_or_insert(elapsed + Duration::from_millis(500));
            if elapsed >= end {
                self.last_click = None;
            } else {
                next_frame = Some(end);
            }
        }
        AnimationUpdate {
            next_frame,
            ..Default::default()
        }
    }
    fn enabled(&self) -> bool {
        self.list.enabled()
    }
    fn focusable(&self) -> bool {
        self.properties.global_rows.is_none() && self.list.focusable()
    }
    fn measure(&mut self, c: MeasureContext) -> Result<Size, LayoutError> {
        self.list.measure(c)
    }
    fn prepare(&mut self, c: PrepareContext) -> Result<(), LayoutError> {
        self.list.prepare(c)
    }
    fn paint(&self, c: PaintContext, p: &mut Painter<'_>) {
        self.list.paint(c, p)
    }
    fn event(&mut self, c: EventContext, e: &WidgetEvent) -> EventResponse {
        if matches!(
            e,
            WidgetEvent::Input(InputEvent::Key(_) | InputEvent::Scroll(_))
        ) {
            self.label_press = None;
            self.last_click = None;
        }
        if self.drag.is_some()
            && matches!(
                e,
                WidgetEvent::Input(InputEvent::Key(KeyInput {
                    key: Key::Escape,
                    pressed: true,
                    ..
                }))
            )
        {
            self.drag = None;
            self.armed = None;
            self.label_press = None;
            self.last_click = None;
            let mut response = self.list.event(c, &WidgetEvent::PointerCaptureLost);
            response.capture = CaptureRequest::Release;
            response.handled = true;
            return response;
        }
        if self.enabled()
            && c.phase == EventPhase::Target
            && let WidgetEvent::Input(InputEvent::Key(k)) = e
            && k.pressed
            && !k.modifiers.control
            && !k.modifiers.alt
            && !k.modifiers.super_key
        {
            let active = self
                .list
                .active_item()
                .and_then(|id| self.properties.rows.iter().position(|r| r.id.0 == id.0));
            if let Some(index) = active {
                let row = &self.properties.rows[index];
                let event = match k.key {
                    Key::ArrowRight if row.branch && !row.expanded => Some(TreeEvent::Expansion {
                        id: row.id,
                        expanded: true,
                    }),
                    Key::ArrowRight if row.branch => self
                        .properties
                        .rows
                        .iter()
                        .skip(index + 1)
                        .find(|r| r.parent == Some(row.id) && !r.disabled)
                        .map(|r| TreeEvent::Selected(r.id)),
                    Key::ArrowLeft if row.branch && row.expanded => Some(TreeEvent::Expansion {
                        id: row.id,
                        expanded: false,
                    }),
                    Key::ArrowLeft => row.parent.map(TreeEvent::Selected),
                    Key::Enter | Key::Space if row.branch => Some(TreeEvent::Expansion {
                        id: row.id,
                        expanded: !row.expanded,
                    }),
                    _ => None,
                };
                if let Some(event) = event {
                    if k.repeat && matches!(event, TreeEvent::Expansion { .. }) {
                        return EventResponse {
                            handled: true,
                            ..Default::default()
                        };
                    }
                    return EventResponse {
                        handled: true,
                        actions: self.actions(event),
                        ..Default::default()
                    };
                }
            }
            if matches!(k.key, Key::ArrowLeft | Key::ArrowRight) {
                return EventResponse {
                    handled: true,
                    ..Default::default()
                };
            }
        }
        let hit = self
            .list
            .hit(c)
            .and_then(|id| self.properties.rows.iter().position(|r| r.id.0 == id.0));
        let icon = hit
            .filter(|&i| self.properties.rows[i].branch)
            .filter(|&i| {
                c.local_position.is_some_and(|p| {
                    let left = c.geometry.map_or(0.0, |g| g.content_bounds.origin.x)
                        + self.properties.list.style.padding_x as f64
                        + self.properties.list.items[i].indent as f64;
                    p.x >= left
                        && p.x < left + self.properties.list.text_style.font_size as f64 * 1.4
                })
            });
        let label = hit.filter(|&i| {
            let row = &self.properties.rows[i];
            row.branch
                && !row.disabled
                && c.local_position.is_some_and(|p| {
                    let left = c.geometry.map_or(0.0, |g| g.content_bounds.origin.x)
                        + self.properties.list.style.padding_x as f64
                        + self.properties.list.items[i].indent as f64
                        + self.properties.list.text_style.font_size as f64 * 1.4;
                    p.x >= left
                })
        });
        let mut label_expansion = None;
        if self.enabled() && c.phase == EventPhase::Target {
            match e {
                WidgetEvent::Input(InputEvent::PointerButton {
                    button: PointerButton::Primary,
                    pressed: true,
                    ..
                }) => {
                    self.label_press = label
                        .and_then(|i| c.local_position.map(|p| (self.properties.rows[i].id, p)));
                    if self.label_press.is_none() || c.modifiers != Modifiers::default() {
                        self.label_press = None;
                        self.last_click = None;
                    }
                }
                WidgetEvent::Input(InputEvent::PointerMoved(_)) => {
                    if let Some(p) = c.local_position {
                        if self.label_press.is_some_and(|(_, start)| {
                            (p.x - start.x).abs() + (p.y - start.y).abs() >= 6.0
                        }) {
                            self.label_press = None;
                            self.last_click = None;
                        }
                    }
                }
                WidgetEvent::Input(InputEvent::PointerButton {
                    button: PointerButton::Primary,
                    pressed: false,
                    ..
                }) => {
                    let pressed = self.label_press.take();
                    if let Some(i) = label.filter(|&i| {
                        pressed.is_some_and(|(id, _)| id == self.properties.rows[i].id)
                    }) {
                        let (id, start) = pressed.unwrap();
                        let p = c.local_position.unwrap();
                        if (p.x - start.x).abs() + (p.y - start.y).abs() < 6.0 {
                            if self.last_click.take().is_some_and(|(previous, point, _)| {
                                previous == id
                                    && (p.x - point.x).abs() + (p.y - point.y).abs() < 6.0
                            }) {
                                label_expansion = Some(i);
                            } else {
                                self.last_click = Some((id, p, None));
                            }
                        } else {
                            self.last_click = None;
                        }
                    } else {
                        self.last_click = None;
                    }
                }
                WidgetEvent::Input(
                    InputEvent::Key(_) | InputEvent::Scroll(_) | InputEvent::PointerButton { .. },
                ) => {
                    self.label_press = None;
                    self.last_click = None;
                }
                _ => {}
            }
        }
        if self.enabled() && self.properties.draggable && c.phase == EventPhase::Target {
            match e {
                WidgetEvent::Input(InputEvent::PointerButton {
                    button: PointerButton::Primary,
                    pressed: true,
                    ..
                }) if icon.is_none() => {
                    self.drag = hit
                        .filter(|&i| !self.properties.rows[i].disabled)
                        .and_then(|i| {
                            c.local_position
                                .map(|p| (self.properties.rows[i].id, p, false))
                        });
                }
                WidgetEvent::Input(InputEvent::PointerMoved(_)) => {
                    if let (Some((_, start, moved)), Some(p)) = (&mut self.drag, c.local_position) {
                        *moved |= (p.x - start.x).abs() + (p.y - start.y).abs() >= 6.0;
                    }
                }
                WidgetEvent::Input(InputEvent::PointerButton {
                    button: PointerButton::Primary,
                    pressed: false,
                    ..
                }) => {
                    if let Some((source, _, true)) = self.drag.take() {
                        let mut r = self.list.event(c, &WidgetEvent::PointerCaptureLost);
                        r.capture = CaptureRequest::Release;
                        r.handled = true;
                        if let Some(target) = hit
                            .filter(|&i| !self.properties.rows[i].disabled)
                            .map(|i| self.properties.rows[i].id)
                        {
                            let sources: Vec<_> = self
                                .properties
                                .selection
                                .as_ref()
                                .filter(|ids| ids.contains(&source))
                                .map_or_else(
                                    || alloc::vec![source],
                                    |ids| ids.iter().copied().collect(),
                                );
                            // A node cannot be moved into itself or any visible descendant.
                            let mut ancestor = Some(target);
                            let mut cycle = false;
                            while let Some(id) = ancestor {
                                cycle |= sources.contains(&id);
                                ancestor = self
                                    .properties
                                    .rows
                                    .iter()
                                    .find(|r| r.id == id)
                                    .and_then(|r| r.parent);
                            }
                            if !cycle {
                                r.actions.push(WidgetAction::new(
                                    self.properties.list.action,
                                    TreeDropRequested { sources, target },
                                ));
                            }
                        }
                        return r;
                    }
                }
                WidgetEvent::Input(InputEvent::Key(k)) if k.key == Key::Escape => {
                    self.drag = None;
                }
                _ => {}
            }
        }
        let mut expansion = None;
        if self.enabled() && c.phase == EventPhase::Target {
            match e {
                WidgetEvent::Input(InputEvent::PointerButton {
                    button: PointerButton::Primary,
                    pressed: true,
                    ..
                }) => self.armed = icon.map(|i| self.properties.rows[i].id),
                WidgetEvent::Input(InputEvent::PointerButton {
                    button: PointerButton::Primary,
                    pressed: false,
                    ..
                }) => {
                    let armed = self.armed.take();
                    expansion = icon.filter(|&i| armed == Some(self.properties.rows[i].id));
                }
                WidgetEvent::Input(InputEvent::Key(_) | InputEvent::Scroll(_)) => self.armed = None,
                _ => {}
            }
        }
        if matches!(
            e,
            WidgetEvent::PointerCaptureLost | WidgetEvent::FocusChanged(false)
        ) {
            self.armed = None;
            self.drag = None;
            // Normal release also relinquishes capture after a completed click.
            if self.label_press.is_some() || matches!(e, WidgetEvent::FocusChanged(false)) {
                self.last_click = None;
            }
            self.label_press = None;
        }
        let mut response = self.list.event(c, e);
        for action in &mut response.actions {
            if let Some(change) = action.payload::<ListMultiSelectionChanged>() {
                let logical_rows = self
                    .properties
                    .global_rows
                    .as_deref()
                    .unwrap_or(&self.properties.rows);
                let mut selected: BTreeSet<_> = self
                    .properties
                    .selection
                    .iter()
                    .flatten()
                    .filter(|id| !logical_rows.iter().any(|r| r.id == **id))
                    .copied()
                    .collect();
                if self.properties.global_rows.is_some()
                    && (c.modifiers.control || c.modifiers.super_key)
                {
                    selected.extend(
                        self.properties
                            .selection
                            .iter()
                            .flatten()
                            .filter(|id| !self.properties.rows.iter().any(|r| r.id == **id))
                            .copied(),
                    );
                }
                selected.extend(change.selected.iter().map(|id| TreeItemId(id.0)));
                *action = WidgetAction::new(action.id, TreeSelectionChanged { selected });
            } else if let Some(change) = action.payload::<ListSelectionChanged>() {
                *action = WidgetAction::new(
                    action.id,
                    TreeEvent::Selected(TreeItemId(change.selected.0)),
                );
            }
        }
        if self.properties.global_rows.is_some()
            && response
                .actions
                .iter()
                .any(|a| a.payload::<TreeSelectionChanged>().is_some())
            && let Some(active) = self.list.active_item()
        {
            response.actions.push(WidgetAction::new(
                self.properties.list.action,
                TreeEvent::Selected(TreeItemId(active.0)),
            ));
        }
        if let Some(i) = expansion.or(label_expansion) {
            response.actions.clear();
            let row = &self.properties.rows[i];
            response.actions = self.actions(TreeEvent::Expansion {
                id: row.id,
                expanded: !row.expanded,
            });
        }
        response
    }
}

impl TreeWidget {
    fn actions(&self, event: TreeEvent) -> Vec<WidgetAction> {
        let mut actions = alloc::vec![WidgetAction::new(self.properties.list.action, event)];
        if let TreeEvent::Expansion { id, expanded: true } = event
            && self.properties.rows.iter().any(|r| r.id == id && r.load)
        {
            actions.push(WidgetAction::new(
                self.properties.list.action,
                TreeLoadRequested { id },
            ));
        }
        actions
    }
}
