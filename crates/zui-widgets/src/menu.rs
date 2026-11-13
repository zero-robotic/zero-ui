//! Menu descriptions and keyboard policy. Applications own the open submenu path.
use crate::{
    ButtonView, ListBox, ListBoxView, ListItem, ListItemId, ListSelectionChanged, PopupPlacement,
    PopupView, action_button::ActionButtonView, row,
};
use alloc::{collections::BTreeSet, format, string::String, vec::Vec};
use core::time::Duration;
use zui_api::{
    action::{ActionId, WidgetAction},
    event::*,
    layout::{Edges, LayoutError, LayoutKind, LayoutStyle},
    view::{View, ViewSpec},
    widget::*,
};
use zui_paint::Painter;
use zui_theme::Theme;
use zui_types::{Affine, Point, Rect, Size};
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
    pub separator: bool,
    pub shortcut_label: Option<String>,
    pub shortcut: Option<MenuShortcut>,
    pub children: Vec<MenuItem>,
}
impl MenuItem {
    pub fn separator(id: MenuId) -> Self {
        let mut item = Self::new(id, "");
        item.separator = true;
        item.disabled = true;
        item
    }
    pub fn new(id: MenuId, label: impl Into<String>) -> Self {
        Self {
            id,
            label: label.into(),
            disabled: false,
            checked: false,
            separator: false,
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
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MenuBehavior {
    pub hover_delay: Duration,
    pub aim_delay: Duration,
    pub pointer_threshold: f64,
}
impl Default for MenuBehavior {
    fn default() -> Self {
        Self {
            hover_delay: Duration::from_millis(180),
            aim_delay: Duration::from_millis(350),
            pointer_threshold: 4.0,
        }
    }
}
#[derive(Clone)]
pub struct MenuView {
    pub items: Vec<MenuItem>,
    pub open_path: Vec<MenuId>,
    pub theme: Theme,
    pub families: Vec<String>,
    pub action: ActionId,
    pub width: f64,
    pub max_height: f64,
    pub behavior: MenuBehavior,
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
            behavior: MenuBehavior::default(),
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
            if item.disabled || item.separator {
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
    /// Create all open levels in one focus scope. Escape/outside dismissal
    /// is returned through `dismiss`; applications pop the path or close the menu.
    pub fn popup(&self, placement: PopupPlacement, dismiss: ActionId) -> Result<View, MenuError> {
        if self.open_path.len() > 16 {
            return Err(MenuError::TooDeep);
        }
        let levels = self.levels()?;
        Ok(View::new(MenuPopupView {
            levels,
            placement,
            dismiss,
            width: self.width,
            max_height: self.max_height,
            behavior: self.behavior,
        })
        .styled(LayoutStyle {
            kind: LayoutKind::Stack,
            width: zui_api::layout::Dimension::Percent(1.0),
            height: zui_api::layout::Dimension::Percent(1.0),
            ..Default::default()
        })
        .clipped(true))
    }
    fn levels(&self) -> Result<Vec<MenuListView>, MenuError> {
        let mut items = &self.items;
        let mut path = Vec::new();
        let mut levels = Vec::new();
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
                    indent: 0.0,
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
                    disabled: i.disabled || i.separator,
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
            list.selected = self.open_path.get(depth).map(|id| ListItemId(id.0));
            list.style.natural_max_height = self.max_height as f32;
            levels.push(MenuListView {
                list,
                items: items.clone(),
                path: path.clone(),
                open_child: self.open_path.get(depth).copied(),
                behavior: self.behavior,
            });
            if let Some(id) = self.open_path.get(depth) {
                let item = items
                    .iter()
                    .find(|i| i.id == *id && !i.disabled && !i.separator && !i.children.is_empty())
                    .ok_or(MenuError::InvalidPath(*id))?;
                items = &item.children;
                path.push(*id);
            }
        }
        Ok(levels)
    }
}

#[derive(Clone, Copy)]
struct PendingMenu {
    id: ListItemId,
    since: Option<Duration>,
    delay: Duration,
}
fn towards_submenu(origin: Point, point: Point, submenu: Rect) -> bool {
    let edge = if origin.x < submenu.origin.x {
        submenu.origin.x
    } else if origin.x > submenu.origin.x + submenu.size.width {
        submenu.origin.x + submenu.size.width
    } else {
        return false;
    };
    let distance = edge - origin.x;
    let progress = (point.x - origin.x) / distance;
    if !(0.0..=1.0).contains(&progress) || progress == 0.0 {
        return false;
    }
    let top = origin.y + (submenu.origin.y - 6.0 - origin.y) * progress;
    let bottom = origin.y + (submenu.origin.y + submenu.size.height + 6.0 - origin.y) * progress;
    point.y >= top && point.y <= bottom
}
#[derive(Clone, PartialEq)]
struct MenuPopupView {
    levels: Vec<MenuListView>,
    placement: PopupPlacement,
    dismiss: ActionId,
    width: f64,
    max_height: f64,
    behavior: MenuBehavior,
}
struct MenuPopup {
    properties: MenuPopupView,
    levels: Vec<MenuList>,
    bounds: Vec<Rect>,
    active: usize,
    hovered: Option<usize>,
    pressed: Option<usize>,
    pointer: Option<Point>,
    keyboard_origin: Option<Point>,
    keyboard_mode: bool,
}
impl ViewSpec for MenuPopupView {
    type Widget = MenuPopup;
    fn build(&self) -> MenuPopup {
        MenuPopup {
            properties: self.clone(),
            levels: self.levels.iter().map(ViewSpec::build).collect(),
            bounds: Vec::new(),
            active: self.levels.len().saturating_sub(1),
            hovered: None,
            pressed: None,
            pointer: None,
            keyboard_origin: None,
            keyboard_mode: false,
        }
    }
    fn update(&self, widget: &mut MenuPopup) -> Invalidation {
        if self == &widget.properties {
            return Invalidation::None;
        }
        let path_changed = self.levels.len() != widget.levels.len()
            || self
                .levels
                .iter()
                .zip(&widget.levels)
                .any(|(next, old)| next.path != old.properties.path);
        let reset = path_changed
            || self.width != widget.properties.width
            || self.max_height != widget.properties.max_height
            || self.placement != widget.properties.placement
            || self.dismiss != widget.properties.dismiss
            || self.behavior != widget.properties.behavior;
        for (index, level) in self.levels.iter().enumerate() {
            if let Some(old) = widget.levels.get_mut(index) {
                level.update(old);
            } else {
                widget.levels.push(level.build());
            }
        }
        widget.levels.truncate(self.levels.len());
        if reset {
            widget.cancel();
            widget.hovered = None;
            widget.pressed = None;
        }
        if path_changed {
            widget.active = self.levels.len().saturating_sub(1);
        }
        widget.properties = self.clone();
        Invalidation::Layout
    }
}
impl MenuPopup {
    fn cancel(&mut self) {
        for level in &mut self.levels {
            level.pending = None;
            level.hovered = None;
            level.aim_origin = None;
        }
    }
    fn dismiss(&self, reason: crate::DismissReason) -> EventResponse {
        EventResponse {
            actions: alloc::vec![WidgetAction::new(
                self.properties.dismiss,
                crate::PopupDismissed { reason }
            )],
            handled: true,
            stop_propagation: true,
            ..Default::default()
        }
    }
}
impl Widget for MenuPopup {
    fn focus_scope(&self) -> bool {
        true
    }
    fn focusable(&self) -> bool {
        true
    }
    fn prepare(&mut self, context: PrepareContext) -> Result<(), LayoutError> {
        let properties = &self.properties;
        if !properties.behavior.pointer_threshold.is_finite()
            || properties.behavior.pointer_threshold < 0.0
        {
            return Err(LayoutError::InvalidStyle(context.id));
        }
        let viewport = context.layout.bounds.size;
        let width = properties.width.min(viewport.width);
        if !properties.width.is_finite()
            || properties.width < 0.0
            || !properties.max_height.is_finite()
            || properties.max_height < 0.0
            || properties.width > f32::MAX as f64
            || properties.max_height > f32::MAX as f64
        {
            return Err(LayoutError::InvalidStyle(context.id));
        }
        let mut bounds = Vec::new();
        let mut anchor: Option<Rect> = None;
        for level in &mut self.levels {
            let measured = level.list.measure(MeasureContext {
                id: context.id,
                input: zui_api::layout::MeasureInput {
                    known_width: Some(width as f32),
                    known_height: None,
                    available_width: zui_api::layout::AvailableSpace::Definite(width as f32),
                    available_height: zui_api::layout::AvailableSpace::MaxContent,
                },
                text: context.text,
            })?;
            let size = Size::new(width, measured.height.max(1.0).min(properties.max_height));
            let placement = if let Some(row) = anchor {
                let right = row.origin.x + row.size.width;
                let left = row.origin.x - width;
                let horizontal = if right + width <= viewport.width - 8.0 || left < 8.0 {
                    right
                } else {
                    left
                };
                PopupPlacement::Point(Point::new(horizontal, row.origin.y))
            } else {
                properties.placement
            };
            let rect = PopupView::new(placement, size, properties.dismiss)
                .bounds(viewport)
                .map_err(|_| LayoutError::InvalidStyle(context.id))?;
            let local = Rect::new(0.0, 0.0, rect.size.width, rect.size.height);
            level.prepare(PrepareContext {
                id: context.id,
                layout: zui_api::layout::LayoutBox::new(local),
                content_size: rect.size,
                text: context.text,
            })?;
            anchor = level
                .properties
                .open_child
                .and_then(|id| level.list.item_rect(ListItemId(id.0), local))
                .map(|row| {
                    Rect::new(
                        rect.origin.x + row.origin.x,
                        rect.origin.y + row.origin.y,
                        row.size.width,
                        row.size.height,
                    )
                });
            bounds.push(rect);
        }
        if bounds != self.bounds {
            self.cancel();
        }
        self.bounds = bounds;
        for (index, level) in self.levels.iter_mut().enumerate() {
            level.submenu = self.bounds.get(index + 1).map(|child| {
                Rect::new(
                    child.origin.x - self.bounds[index].origin.x,
                    child.origin.y - self.bounds[index].origin.y,
                    child.size.width,
                    child.size.height,
                )
            });
        }
        Ok(())
    }
    fn paint(&self, context: PaintContext, painter: &mut Painter<'_>) {
        for (level, bounds) in self.levels.iter().zip(&self.bounds) {
            painter.with_transform(
                Affine::translate(bounds.origin.x, bounds.origin.y),
                |painter| {
                    level.paint(
                        PaintContext {
                            id: context.id,
                            size: bounds.size,
                            content_bounds: Rect::new(
                                0.0,
                                0.0,
                                bounds.size.width,
                                bounds.size.height,
                            ),
                        },
                        painter,
                    );
                },
            );
        }
    }
    fn animate_with_actions(
        &mut self,
        elapsed: Duration,
        actions: &mut Vec<WidgetAction>,
    ) -> AnimationUpdate {
        let mut update = AnimationUpdate::default();
        for level in &mut self.levels {
            let next = level.advance(elapsed, actions);
            update.invalidation = update.invalidation.max(next.invalidation);
            if let Some(deadline) = next.next_frame {
                update.next_frame =
                    Some(update.next_frame.map_or(deadline, |old| old.min(deadline)));
            }
        }
        update
    }
    fn animate(&mut self, elapsed: Duration) -> AnimationUpdate {
        let mut update = AnimationUpdate::default();
        for level in &mut self.levels {
            if let Some(pending) = &mut level.pending {
                let start = *pending.since.get_or_insert(elapsed);
                if let Some(due) = start.checked_add(pending.delay) {
                    let deadline = if due > elapsed {
                        Some(due)
                    } else {
                        elapsed.checked_add(Duration::from_nanos(1))
                    };
                    if let Some(deadline) = deadline {
                        update.next_frame =
                            Some(update.next_frame.map_or(deadline, |old| old.min(deadline)));
                    }
                }
            }
        }
        update
    }
    fn event(&mut self, context: EventContext, event: &WidgetEvent) -> EventResponse {
        if context.phase != EventPhase::Target && context.phase != EventPhase::Direct {
            return EventResponse::default();
        }
        if matches!(
            event,
            WidgetEvent::FocusChanged(false)
                | WidgetEvent::PointerCaptureLost
                | WidgetEvent::Input(InputEvent::PointerLeft)
        ) {
            self.cancel();
            self.hovered = None;
            self.pressed = None;
            self.pointer = None;
            self.keyboard_origin = None;
            self.keyboard_mode = false;
            for level in &mut self.levels {
                level.event(context, event);
            }
            return EventResponse {
                invalidation: Invalidation::Paint,
                ..Default::default()
            };
        }
        if let WidgetEvent::Input(InputEvent::Key(key)) = event
            && key.pressed
        {
            self.cancel();
            self.keyboard_mode = true;
            self.keyboard_origin = self.pointer;
            if key.key == Key::Escape {
                return self.dismiss(crate::DismissReason::Escape);
            }
        }
        let positional = matches!(
            event,
            WidgetEvent::Input(
                InputEvent::PointerMoved(_)
                    | InputEvent::PointerButton { .. }
                    | InputEvent::Scroll(_)
            )
        );
        if matches!(event, WidgetEvent::Input(InputEvent::PointerMoved(_)))
            && let Some(point) = context.local_position
        {
            if self.keyboard_mode {
                let origin = *self.keyboard_origin.get_or_insert(point);
                let distance = (point.x - origin.x) * (point.x - origin.x)
                    + (point.y - origin.y) * (point.y - origin.y);
                if distance
                    <= self.properties.behavior.pointer_threshold
                        * self.properties.behavior.pointer_threshold
                {
                    return EventResponse {
                        handled: true,
                        ..Default::default()
                    };
                }
                self.keyboard_mode = false;
            }
            self.pointer = Some(point);
        }
        if matches!(
            event,
            WidgetEvent::Input(InputEvent::PointerButton { .. } | InputEvent::Scroll(_))
        ) {
            self.keyboard_mode = false;
            self.cancel();
            self.pointer = context.local_position;
        }
        let hit = context
            .local_position
            .and_then(|point| self.bounds.iter().rposition(|rect| rect.contains(point)));
        if positional && self.hovered != hit {
            if let Some(previous) = self.hovered {
                self.levels[previous].event(context, &WidgetEvent::Input(InputEvent::PointerLeft));
            }
            self.hovered = hit;
        }
        if matches!(
            event,
            WidgetEvent::Input(InputEvent::PointerButton { pressed: true, .. })
        ) {
            if hit.is_none() {
                return self.dismiss(crate::DismissReason::OutsidePointer);
            }
            self.pressed = hit;
        }
        let target = if positional {
            self.pressed.or(hit)
        } else {
            Some(self.active)
        };
        let Some(index) = target.filter(|index| *index < self.levels.len()) else {
            return EventResponse::default();
        };
        self.active = index;
        for (other, level) in self.levels.iter_mut().enumerate() {
            if other != index {
                level.list.event(context, &WidgetEvent::FocusChanged(false));
            }
        }
        let bounds = self.bounds.get(index).copied().unwrap_or_default();
        let local = context
            .local_position
            .map(|point| Point::new(point.x - bounds.origin.x, point.y - bounds.origin.y));
        let local_context = EventContext {
            phase: EventPhase::Target,
            local_position: local,
            geometry: Some(EventGeometry {
                size: bounds.size,
                content_bounds: Rect::new(0.0, 0.0, bounds.size.width, bounds.size.height),
            }),
            ..context
        };
        if matches!(event, WidgetEvent::Input(InputEvent::Key(_))) {
            self.levels[index]
                .list
                .event(local_context, &WidgetEvent::FocusChanged(true));
        }
        let offset = self.levels[index].list.scroll_offset();
        let mut response = self.levels[index].event(local_context, event);
        if self.levels[index].list.scroll_offset() != offset {
            response.invalidation = Invalidation::Layout;
        }
        if matches!(
            event,
            WidgetEvent::Input(InputEvent::PointerButton { pressed: false, .. })
        ) {
            self.pressed = None;
        }
        response
    }
}

#[derive(Clone, PartialEq)]
struct MenuListView {
    list: ListBoxView,
    items: Vec<MenuItem>,
    path: Vec<MenuId>,
    open_child: Option<MenuId>,
    behavior: MenuBehavior,
}
struct MenuList {
    list: ListBox,
    properties: MenuListView,
    hovered: Option<ListItemId>,
    pending: Option<PendingMenu>,
    pointer: Option<Point>,
    aim_origin: Option<Point>,
    submenu: Option<Rect>,
}
impl ViewSpec for MenuListView {
    type Widget = MenuList;
    fn build(&self) -> MenuList {
        MenuList {
            list: self.list.build(),
            properties: self.clone(),
            hovered: None,
            pending: None,
            pointer: None,
            aim_origin: None,
            submenu: None,
        }
    }
    fn update(&self, w: &mut MenuList) -> Invalidation {
        if self.items != w.properties.items
            || self.path != w.properties.path
            || self.open_child != w.properties.open_child
            || self.behavior != w.properties.behavior
            || self.list != w.properties.list
        {
            w.pending = None;
            w.hovered = None;
            w.aim_origin = None;
        }
        let restore = (self.open_child.is_none() && w.properties.open_child.is_some())
            .then(|| w.list.active_item())
            .flatten();
        let r = self.list.update(&mut w.list);
        if let Some(id) = restore {
            w.list.restore_active_item(id);
        }
        w.properties = self.clone();
        r
    }
}
impl MenuList {
    fn hover_command(&self, id: ListItemId) -> Option<MenuEvent> {
        match self.chosen(id)? {
            MenuEvent::OpenPath(path)
                if self.properties.open_child.map(|open| open.0) != Some(id.0) =>
            {
                Some(MenuEvent::OpenPath(path))
            }
            MenuEvent::Selected(_) if self.properties.open_child.is_some() => {
                Some(MenuEvent::OpenPath(self.properties.path.clone()))
            }
            _ => None,
        }
    }
    fn advance(&mut self, elapsed: Duration, actions: &mut Vec<WidgetAction>) -> AnimationUpdate {
        let Some(pending) = &mut self.pending else {
            return AnimationUpdate::default();
        };
        let start = *pending.since.get_or_insert(elapsed);
        let Some(due) = start.checked_add(pending.delay) else {
            self.pending = None;
            return AnimationUpdate::default();
        };
        if elapsed < due {
            return AnimationUpdate {
                next_frame: Some(due),
                ..Default::default()
            };
        }
        let id = pending.id;
        self.pending = None;
        if self.hovered == Some(id)
            && let Some(command) = self.hover_command(id)
        {
            actions.push(WidgetAction::new(self.properties.list.action, command));
        }
        AnimationUpdate {
            invalidation: Invalidation::Paint,
            next_frame: None,
        }
    }

    fn chosen(&self, id: ListItemId) -> Option<MenuEvent> {
        let item = self
            .properties
            .items
            .iter()
            .find(|i| i.id.0 == id.0 && !i.disabled && !i.separator)?;
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
        self.list.paint(c, p);
        p.with_clip(zui_paint::Path::rectangle(c.content_bounds), |p| {
            for item in self.properties.items.iter().filter(|i| i.separator) {
                if let Some(rect) = self.list.item_rect(ListItemId(item.id.0), c.content_bounds) {
                    p.fill_rect(
                        zui_types::Rect::new(
                            rect.origin.x + 8.0,
                            rect.origin.y + rect.size.height / 2.0,
                            (rect.size.width - 16.0).max(0.0),
                            1.0,
                        ),
                        self.properties.list.style.normal.text,
                    );
                }
            }
        });
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
        if matches!(
            e,
            WidgetEvent::Input(
                InputEvent::PointerButton { .. } | InputEvent::Scroll(_) | InputEvent::Key(_)
            ) | WidgetEvent::PointerCaptureLost
        ) {
            self.pending = None;
            self.aim_origin = None;
        }
        if c.phase == EventPhase::Target
            && matches!(e, WidgetEvent::Input(InputEvent::PointerMoved(_)))
        {
            let hit = self.list.hit(c);
            let previous = self.pointer;
            self.pointer = c.local_position;
            let towards = self
                .aim_origin
                .or(previous)
                .zip(c.local_position)
                .zip(self.submenu)
                .is_some_and(|((origin, point), bounds)| {
                    let edge = if origin.x < bounds.origin.x {
                        bounds.origin.x
                    } else {
                        bounds.origin.x + bounds.size.width
                    };
                    towards_submenu(origin, point, bounds)
                        && previous.is_none_or(|previous| {
                            (point.x - edge).abs() <= (previous.x - edge).abs()
                        })
                });
            if self
                .properties
                .open_child
                .is_some_and(|id| hit == Some(ListItemId(id.0)))
            {
                self.aim_origin = c.local_position;
                self.pending = None;
            } else if hit != self.hovered {
                let delay = if towards {
                    self.properties.behavior.aim_delay
                } else {
                    self.properties.behavior.hover_delay
                };
                let since = if towards {
                    self.pending.and_then(|pending| pending.since)
                } else {
                    None
                };
                self.pending = hit.and_then(|id| {
                    self.hover_command(id)
                        .map(|_| PendingMenu { id, since, delay })
                });
            } else if !towards && let Some(pending) = &mut self.pending {
                pending.delay = pending.delay.min(self.properties.behavior.hover_delay);
            }
            self.hovered = hit;
            if towards && self.pending.is_some() {
                return EventResponse {
                    handled: true,
                    invalidation: Invalidation::Paint,
                    ..Default::default()
                };
            }
        }
        let mut r = self.list.event(c, e);
        if self.pending.is_some() {
            r.invalidation = r.invalidation.max(Invalidation::Paint);
        }
        if matches!(e, WidgetEvent::Input(InputEvent::PointerLeft)) {
            self.hovered = None;
            self.pending = None;
            self.pointer = None;
            self.aim_origin = None;
        }
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
        row(self
            .items
            .into_iter()
            .filter(|item| !item.separator)
            .map(|item| {
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
