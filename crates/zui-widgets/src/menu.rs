//! Menu descriptions and keyboard policy. Applications own the open submenu path.
use crate::{
    ButtonView, ListBox, ListBoxView, ListItem, ListItemId, ListSelectionChanged, PopupPlacement,
    PopupView, action_button::ActionButtonView, row, stack,
};
use alloc::{collections::BTreeSet, format, string::String, vec::Vec};
use zui_api::{
    action::{ActionId, WidgetAction},
    event::*,
    layout::{Edges, LayoutError, LayoutKind, LayoutStyle},
    view::{View, ViewSpec},
    widget::*,
};
use zui_paint::Painter;
use zui_theme::Theme;
use zui_types::{Point, Size};
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct MenuId(pub u64);
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuEvent {
    Selected(MenuId),
    Open(MenuId),
    OpenPath(Vec<MenuId>),
    Back,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuShortcut {
    pub key: Key,
    pub modifiers: Modifiers,
}
#[derive(Clone, Debug, PartialEq)]
pub struct MenuItem {
    pub id: MenuId,
    pub label: String,
    pub disabled: bool,
    pub checked: bool,
    pub shortcut_label: Option<String>,
    pub shortcut: Option<MenuShortcut>,
    pub children: Vec<MenuItem>,
}
impl MenuItem {
    pub fn new(id: MenuId, label: impl Into<String>) -> Self {
        Self {
            id,
            label: label.into(),
            disabled: false,
            checked: false,
            shortcut_label: None,
            shortcut: None,
            children: Vec::new(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuError {
    DuplicateId(MenuId),
    InvalidPath(MenuId),
    TooDeep,
}
impl core::fmt::Display for MenuError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "menu error: {self:?}")
    }
}
impl core::error::Error for MenuError {}
#[derive(Clone)]
pub struct MenuView {
    pub items: Vec<MenuItem>,
    pub open_path: Vec<MenuId>,
    pub theme: Theme,
    pub families: Vec<String>,
    pub action: ActionId,
    pub width: f64,
    pub max_height: f64,
}
impl MenuView {
    pub fn themed(
        theme: &Theme,
        items: Vec<MenuItem>,
        families: Vec<String>,
        action: ActionId,
    ) -> Self {
        Self {
            items,
            open_path: Vec::new(),
            theme: theme.clone(),
            families,
            action,
            width: 240.0,
            max_height: 320.0,
        }
    }
    /// Resolve enabled commands (including closed submenus) in declaration order.
    /// The application chooses shortcut precedence relative to editors and modals.
    pub fn resolve_shortcut(&self, key: &KeyInput) -> Option<MenuId> {
        if !key.pressed || key.repeat {
            return None;
        }
        let mut pending: Vec<_> = self.items.iter().rev().collect();
        while let Some(item) = pending.pop() {
            if item.disabled {
                continue;
            }
            if item.children.is_empty()
                && item
                    .shortcut
                    .as_ref()
                    .is_some_and(|s| s.key == key.key && s.modifiers == key.modifiers)
            {
                return Some(item.id);
            }
            pending.extend(item.children.iter().rev());
        }
        None
    }
    /// Create all open levels as root-level siblings. Escape/outside dismissal
    /// is returned through `dismiss`; applications pop the path or close the menu.
    pub fn popup(&self, placement: PopupPlacement, dismiss: ActionId) -> Result<View, MenuError> {
        if self.open_path.len() > 16 {
            return Err(MenuError::TooDeep);
        }
        let mut items = &self.items;
        let mut path = Vec::new();
        let mut layers = Vec::new();
        let mut place = placement;
        for depth in 0..=self.open_path.len() {
            let mut ids = BTreeSet::new();
            for item in items {
                if !ids.insert(item.id) {
                    return Err(MenuError::DuplicateId(item.id));
                }
            }
            let list_items = items
                .iter()
                .map(|i| ListItem {
                    id: ListItemId(i.id.0),
                    label: format!(
                        "{}{}",
                        i.label,
                        i.shortcut_label
                            .as_ref()
                            .map_or(String::new(), |s| format!("    {s}"))
                    ),
                    leading_icon: i
                        .checked
                        .then(|| crate::Icon::builtin(crate::IconKind::Check)),
                    trailing_icon: (!i.children.is_empty())
                        .then(|| crate::Icon::builtin(crate::IconKind::ChevronRight)),
                    disabled: i.disabled,
                })
                .collect();
            let mut list = ListBoxView::themed(
                &self.theme,
                list_items,
                self.families.clone(),
                None,
                self.action,
            );
            list.activation_only = true;
            let height = ((items.len().max(1) as f64) * 44.0).min(self.max_height);
            let spec = MenuListView {
                list,
                items: items.clone(),
                path: path.clone(),
            };
            layers.push(
                PopupView::new(place, Size::new(self.width, height), dismiss)
                    .build(View::new(spec))
                    .keyed(depth as u64),
            );
            if let Some(id) = self.open_path.get(depth) {
                let (index, item) = items
                    .iter()
                    .enumerate()
                    .find(|(_, i)| i.id == *id && !i.disabled && !i.children.is_empty())
                    .ok_or(MenuError::InvalidPath(*id))?;
                let origin = match place {
                    PopupPlacement::Anchor(r) => Point::new(r.origin.x, r.origin.y + r.size.height),
                    PopupPlacement::Point(p) => p,
                    PopupPlacement::Center => Point::ZERO,
                };
                place = PopupPlacement::Point(Point::new(
                    origin.x + self.width,
                    origin.y + index as f64 * 44.0,
                ));
                items = &item.children;
                path.push(*id);
            }
        }
        Ok(stack(layers)
            .style(|s| {
                s.width = zui_api::layout::Dimension::Percent(1.0);
                s.height = zui_api::layout::Dimension::Percent(1.0);
            })
            .build())
    }
}
#[derive(Clone)]
struct MenuListView {
    list: ListBoxView,
    items: Vec<MenuItem>,
    path: Vec<MenuId>,
}
struct MenuList {
    list: ListBox,
    properties: MenuListView,
}
impl ViewSpec for MenuListView {
    type Widget = MenuList;
    fn build(&self) -> MenuList {
        MenuList {
            list: self.list.build(),
            properties: self.clone(),
        }
    }
    fn update(&self, w: &mut MenuList) -> Invalidation {
        let r = self.list.update(&mut w.list);
        w.properties = self.clone();
        r
    }
}
impl MenuList {
    fn chosen(&self, id: ListItemId) -> Option<MenuEvent> {
        let item = self
            .properties
            .items
            .iter()
            .find(|i| i.id.0 == id.0 && !i.disabled)?;
        Some(if item.children.is_empty() {
            MenuEvent::Selected(item.id)
        } else {
            let mut path = self.properties.path.clone();
            path.push(item.id);
            MenuEvent::OpenPath(path)
        })
    }
}
impl Widget for MenuList {
    fn enabled(&self) -> bool {
        self.list.enabled()
    }
    fn focusable(&self) -> bool {
        self.list.focusable()
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
        if c.phase == EventPhase::Target
            && let WidgetEvent::Input(InputEvent::Key(k)) = e
            && k.pressed
            && !k.repeat
            && !k.modifiers.control
            && !k.modifiers.alt
            && !k.modifiers.super_key
        {
            let command = match k.key {
                Key::ArrowLeft if !self.properties.path.is_empty() => Some(MenuEvent::Back),
                Key::ArrowRight => self
                    .list
                    .active_item()
                    .and_then(|id| self.chosen(id))
                    .filter(|e| matches!(e, MenuEvent::OpenPath(_))),
                _ => None,
            };
            if let Some(command) = command {
                return EventResponse {
                    actions: alloc::vec![WidgetAction::new(self.properties.list.action, command)],
                    handled: true,
                    ..Default::default()
                };
            }
        }
        let mut r = self.list.event(c, e);
        for a in &mut r.actions {
            if let Some(e) = a
                .payload::<ListSelectionChanged>()
                .and_then(|e| self.chosen(e.selected))
            {
                *a = WidgetAction::new(a.id, e);
            }
        }
        r
    }
}
/// Top-level menu buttons; `MenuEvent::Open(id)` selects the corresponding popup.
pub struct MenuBarView {
    pub items: Vec<MenuItem>,
    pub theme: Theme,
    pub families: Vec<String>,
    pub action: ActionId,
}
impl MenuBarView {
    pub fn themed(
        theme: &Theme,
        items: Vec<MenuItem>,
        families: Vec<String>,
        action: ActionId,
    ) -> Self {
        Self {
            items,
            theme: theme.clone(),
            families,
            action,
        }
    }
    pub fn build(self) -> View {
        row(self.items.into_iter().map(|item| {
            let mut button =
                ButtonView::themed(&self.theme, item.label, self.families.clone(), self.action);
            button.disabled = item.disabled;
            View::new(ActionButtonView {
                button,
                payload: MenuEvent::Open(item.id),
            })
            .keyed(item.id.0)
            .styled(LayoutStyle {
                padding: Edges::all(10.0),
                ..Default::default()
            })
        }))
        .build()
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContextMenuRequested {
    pub position: Point,
}
#[derive(Clone, Copy)]
pub struct ContextMenuRegion {
    pub action: ActionId,
}
impl ContextMenuRegion {
    /// Wrap the root content to obtain root-local pointer positions.
    pub fn build(self, content: View) -> View {
        View::new(self)
            .styled(LayoutStyle {
                kind: LayoutKind::Stack,
                ..Default::default()
            })
            .with_children([content])
    }
}
impl ViewSpec for ContextMenuRegion {
    type Widget = Self;
    fn build(&self) -> Self {
        *self
    }
    fn update(&self, w: &mut Self) -> Invalidation {
        *w = *self;
        Invalidation::None
    }
}
impl Widget for ContextMenuRegion {
    fn paint(&self, _: PaintContext, _: &mut Painter<'_>) {}
    fn event(&mut self, c: EventContext, e: &WidgetEvent) -> EventResponse {
        if c.phase != EventPhase::Bubble
            && let WidgetEvent::Input(InputEvent::Key(k)) = e
            && k.pressed
            && !k.repeat
            && k.key == Key::Function(10)
            && k.modifiers.shift
            && !k.modifiers.control
            && !k.modifiers.alt
            && !k.modifiers.super_key
        {
            return EventResponse {
                actions: alloc::vec![WidgetAction::new(
                    self.action,
                    ContextMenuRequested {
                        position: c.geometry.map_or(Point::ZERO, |g| g.content_bounds.origin)
                    }
                )],
                handled: true,
                stop_propagation: true,
                ..Default::default()
            };
        }
        if c.phase != EventPhase::Bubble
            && let WidgetEvent::Input(InputEvent::PointerButton {
                button: PointerButton::Secondary,
                pressed: true,
                ..
            }) = e
            && let Some(position) = c.local_position
        {
            return EventResponse {
                actions: alloc::vec![WidgetAction::new(
                    self.action,
                    ContextMenuRequested { position }
                )],
                handled: true,
                stop_propagation: true,
                ..Default::default()
            };
        }
        EventResponse::default()
    }
}
