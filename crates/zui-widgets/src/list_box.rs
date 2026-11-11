//! Single-choice list with a bounded viewport and stable option identities.
use crate::{
    ScrollbarStyle, Text, TextView,
    axis::Axis,
    choice::{ChoiceItem, ChoiceState},
    scrollbar::BarGeometry,
};
use alloc::{collections::BTreeSet, string::String, vec::Vec};
use zui_api::{
    action::{ActionId, WidgetAction},
    event::*,
    layout::{AvailableSpace, LayoutBox, LayoutError},
    text::TextStyle,
    view::ViewSpec,
    widget::{Invalidation, MeasureContext, PaintContext, PrepareContext, Widget},
};
use zui_paint::{Painter, Path};
use zui_theme::{ColorRole, Theme, TypeRole};
use zui_types::{Color, NodeId, Point, Rect, Size};
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ListItemId(pub u64);
#[derive(Clone, Debug, PartialEq)]
pub struct ListItem {
    pub id: ListItemId,
    pub label: String,
    /// Additional logical indentation before the leading icon and label.
    pub indent: f32,
    pub leading_icon: Option<crate::Icon>,
    pub trailing_icon: Option<crate::Icon>,
    pub disabled: bool,
}
impl ListItem {
    pub fn new(id: ListItemId, label: impl Into<String>) -> Self {
        Self {
            id,
            label: label.into(),
            indent: 0.0,
            leading_icon: None,
            trailing_icon: None,
            disabled: false,
        }
    }
}
impl ChoiceItem for ListItem {
    type Id = ListItemId;
    fn id(&self) -> ListItemId {
        self.id
    }
    fn disabled(&self) -> bool {
        self.disabled
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ListSelectionChanged {
    pub selected: ListItemId,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListMultiSelectionChanged {
    pub selected: BTreeSet<ListItemId>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ListBoxColors {
    pub background: Color,
    pub text: Color,
    pub selected_background: Color,
    pub selected_text: Color,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ListBoxStyle {
    pub normal: ListBoxColors,
    pub hovered: ListBoxColors,
    pub pressed: ListBoxColors,
    pub disabled: ListBoxColors,
    pub focus_ring: Color,
    pub padding_x: f32,
    pub padding_y: f32,
    pub row_gap: f32,
    pub min_row_height: f32,
    pub natural_max_height: f32,
}
impl ListBoxStyle {
    pub fn from_theme(theme: &Theme) -> Self {
        let normal = ListBoxColors {
            background: theme.color(ColorRole::Surface),
            text: theme.color(ColorRole::Text),
            selected_background: theme.selection_background(),
            selected_text: theme.selection_foreground(),
        };
        Self {
            normal,
            hovered: ListBoxColors {
                background: theme.color(ColorRole::SurfaceRaised),
                ..normal
            },
            pressed: ListBoxColors {
                background: theme.color(ColorRole::Border),
                ..normal
            },
            disabled: ListBoxColors {
                text: theme.color(ColorRole::TextMuted),
                selected_background: theme.color(ColorRole::SurfaceRaised),
                selected_text: theme.color(ColorRole::TextMuted),
                ..normal
            },
            focus_ring: theme.color(ColorRole::Text),
            padding_x: if theme.controls().min_height > 0 {
                f32::from(theme.controls().padding_x)
            } else {
                12.0
            },
            padding_y: if theme.controls().min_height > 0 {
                4.0
            } else {
                8.0
            },
            row_gap: if theme.controls().min_height > 0 {
                0.0
            } else {
                2.0
            },
            min_row_height: if theme.controls().min_height > 0 {
                f32::from(theme.controls().min_height)
            } else {
                32.0
            },
            natural_max_height: 240.0,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct ListBoxView {
    pub items: Vec<ListItem>,
    pub selected: Option<ListItemId>,
    pub disabled: bool,
    pub action: ActionId,
    pub text_style: TextStyle,
    pub style: ListBoxStyle,
    pub scrollbar: Option<ScrollbarStyle>,
    /// Navigation moves the active row; only click/Space/Enter emits selection.
    pub activation_only: bool,
    /// Some enables controlled multi-selection; selected is then only an initial active row.
    pub multi_selection: Option<BTreeSet<ListItemId>>,
}
impl ListBoxView {
    pub fn themed(
        theme: &Theme,
        items: Vec<ListItem>,
        families: Vec<String>,
        selected: Option<ListItemId>,
        action: ActionId,
    ) -> Self {
        let text = TextView::themed(theme, "", families, TypeRole::Body);
        Self {
            items,
            selected,
            disabled: false,
            action,
            text_style: text.style,
            style: ListBoxStyle::from_theme(theme),
            scrollbar: Some(ScrollbarStyle::default()),
            activation_only: false,
            multi_selection: None,
        }
    }
    fn validate(&self, id: NodeId) -> Result<(), LayoutError> {
        let s = self.style;
        let mut ids = BTreeSet::new();
        if self.multi_selection.as_ref().is_some_and(|set| {
            self.activation_only
                || set
                    .iter()
                    .any(|id| !self.items.iter().any(|item| item.id == *id))
        }) || [
            s.padding_x,
            s.padding_y,
            s.row_gap,
            s.min_row_height,
            s.natural_max_height,
        ]
        .iter()
        .any(|v| !v.is_finite() || *v < 0.0)
            || !(s.padding_x * 2.0).is_finite()
            || !(s.padding_y * 2.0).is_finite()
            || self.scrollbar.is_some_and(|bar| !bar.valid())
            || self
                .items
                .iter()
                .any(|item| !ids.insert(item.id) || !item.indent.is_finite() || item.indent < 0.0)
            || self.selected.is_some_and(|id| !ids.contains(&id))
        {
            Err(LayoutError::InvalidStyle(id))
        } else {
            Ok(())
        }
    }
    fn texts(&self) -> Vec<Text> {
        self.items
            .iter()
            .map(|item| {
                TextView::new(
                    item.label.clone(),
                    self.text_style.clone(),
                    self.style.normal.text,
                )
                .build()
            })
            .collect()
    }
}
pub struct ListBox {
    properties: ListBoxView,
    texts: Vec<Text>,
    rows: Vec<Rect>,
    choice: ChoiceState<ListItemId>,
    offset: f64,
    extent: f64,
    viewport: f64,
    width: f64,
    grab: Option<f64>,
    bar_focus: bool,
    reveal: bool,
    range_anchor: Option<ListItemId>,
}
impl ViewSpec for ListBoxView {
    type Widget = ListBox;
    fn build(&self) -> ListBox {
        ListBox {
            properties: self.clone(),
            texts: self.texts(),
            rows: Vec::new(),
            choice: ChoiceState::new(&self.items, self.selected),
            offset: 0.0,
            extent: 0.0,
            viewport: 0.0,
            width: 0.0,
            grab: None,
            bar_focus: false,
            reveal: true,
            range_anchor: self.selected,
        }
    }
    fn update(&self, widget: &mut ListBox) -> Invalidation {
        if self == &widget.properties {
            return Invalidation::None;
        }
        let old = &widget.properties;
        let text = self.items != old.items || self.text_style != old.text_style;
        let geometry = self.scrollbar.map(|bar| (bar.width, bar.min_thumb))
            != old.scrollbar.map(|bar| (bar.width, bar.min_thumb))
            || text
            || self.style.padding_x != old.style.padding_x
            || self.style.padding_y != old.style.padding_y
            || self.style.row_gap != old.style.row_gap
            || self.style.min_row_height != old.style.min_row_height
            || self.style.natural_max_height != old.style.natural_max_height
            || self
                .selected
                .is_some_and(|id| !self.items.iter().any(|item| item.id == id));
        let selected = self.selected != old.selected;
        widget.choice.update(
            &self.items,
            self.selected,
            selected,
            geometry
                || selected
                || self.disabled
                || self.action != old.action
                || self.activation_only != old.activation_only,
        );
        widget.reveal |= geometry || selected;
        if geometry || selected || self.disabled || self.action != old.action {
            widget.grab = None;
        }
        if text {
            widget.texts = self.texts();
        }
        if widget
            .range_anchor
            .is_some_and(|id| !self.items.iter().any(|i| i.id == id && !i.disabled))
        {
            widget.range_anchor = None;
        }
        widget.properties = self.clone();
        if !geometry && widget.reveal {
            widget.reveal_active();
        }
        if geometry {
            Invalidation::Layout
        } else {
            Invalidation::Paint
        }
    }
}
impl ListBox {
    /// Current content offset, in logical pixels.
    pub fn active_item(&self) -> Option<ListItemId> {
        self.choice.active
    }
    pub fn scroll_offset(&self) -> f64 {
        self.offset
    }
    fn reveal_active(&mut self) {
        if self.rows.is_empty() && !self.properties.items.is_empty() {
            self.reveal = true;
            return;
        }
        if let Some(index) = self
            .properties
            .items
            .iter()
            .position(|item| Some(item.id) == self.choice.active)
            && let Some(row) = self.rows.get(index)
        {
            if row.origin.y < self.offset {
                self.offset = row.origin.y;
            } else if row.origin.y + row.size.height > self.offset + self.viewport {
                self.offset = if row.size.height > self.viewport {
                    row.origin.y
                } else {
                    row.origin.y + row.size.height - self.viewport
                };
            }
        }
        self.offset = self
            .offset
            .clamp(0.0, (self.extent - self.viewport).max(0.0));
        self.reveal = false;
    }
    // Search after the active item, wrapping once and considering it last.
    // Lowercase iterators avoid allocating a normalized copy of every label.
    fn prefix_target(&self, prefix: &str) -> Option<ListItemId> {
        let items = &self.properties.items;
        let start = items
            .iter()
            .position(|item| Some(item.id) == self.choice.active)
            .map_or(0, |index| index + 1);
        items[start..]
            .iter()
            .chain(&items[..start])
            .find(|item| {
                if item.disabled {
                    return false;
                }
                let mut label = item.label.chars().flat_map(char::to_lowercase);
                prefix
                    .chars()
                    .flat_map(char::to_lowercase)
                    .all(|character| label.next() == Some(character))
            })
            .map(|item| item.id)
    }
    fn navigate_to(&mut self, target: ListItemId) -> EventResponse {
        self.grab = None;
        let selected = self.choice.navigate_to(
            &self.properties.items,
            target,
            if self.properties.multi_selection.is_some() {
                None
            } else {
                self.properties.selected
            },
        );
        self.reveal_active();
        let mut response = EventResponse {
            handled: true,
            capture: CaptureRequest::Release,
            invalidation: Invalidation::Paint,
            ..Default::default()
        };
        if let Some(selected) = selected.filter(|_| !self.properties.activation_only) {
            response.actions.push(WidgetAction::new(
                self.properties.action,
                ListSelectionChanged { selected },
            ));
        }
        response
    }
    fn bar(&self, bounds: Rect) -> Option<BarGeometry> {
        self.properties.scrollbar?.geometry(
            Axis::Vertical,
            bounds,
            self.offset,
            (self.extent - self.viewport).max(0.0),
            0.0,
        )
    }
    fn bar_event(&mut self, context: EventContext, event: &WidgetEvent) -> Option<EventResponse> {
        if !self.enabled() || context.phase != EventPhase::Target {
            return None;
        }
        let bar = context.geometry.and_then(|g| self.bar(g.content_bounds));
        if let Some(grab) = self.grab
            && matches!(
                event,
                WidgetEvent::Input(
                    InputEvent::PointerMoved(_)
                        | InputEvent::PointerButton {
                            button: PointerButton::Primary,
                            pressed: false,
                            ..
                        }
                )
            )
        {
            if let Some(next) = bar.zip(context.local_position).and_then(|(bar, p)| {
                bar.drag_offset(p, grab, (self.extent - self.viewport).max(0.0))
            }) {
                self.offset = next;
            }
            let release = bar.is_none()
                || matches!(
                    event,
                    WidgetEvent::Input(InputEvent::PointerButton { pressed: false, .. })
                );
            if release {
                self.grab = None;
            }
            return Some(EventResponse {
                handled: true,
                stop_propagation: true,
                invalidation: Invalidation::Paint,
                capture: if release {
                    CaptureRequest::Release
                } else {
                    CaptureRequest::None
                },
                ..Default::default()
            });
        }
        if !matches!(
            event,
            WidgetEvent::Input(InputEvent::PointerButton {
                button: PointerButton::Primary,
                pressed: true,
                ..
            })
        ) {
            return None;
        }
        let (bar, point) = bar.zip(context.local_position)?;
        if !bar.track.contains(point) {
            return None;
        }
        self.choice.update(
            &self.properties.items,
            self.properties.selected,
            false,
            true,
        );
        let mut capture = CaptureRequest::None;
        if bar.thumb.contains(point) {
            if bar.travel > 0.0 {
                self.grab = Some(bar.grab_fraction(point));
                capture = CaptureRequest::Capture;
            }
        } else {
            let direction = if point.y < bar.thumb.origin.y {
                -1.0
            } else {
                1.0
            };
            self.offset = (self.offset + direction * self.viewport)
                .clamp(0.0, (self.extent - self.viewport).max(0.0));
        }
        self.bar_focus = true;
        Some(EventResponse {
            handled: true,
            stop_propagation: true,
            invalidation: Invalidation::Paint,
            capture,
            focus: FocusRequest::Focus,
            ..Default::default()
        })
    }
    fn page_target(&self, forward: bool) -> Option<ListItemId> {
        let index = self
            .properties
            .items
            .iter()
            .position(|item| Some(item.id) == self.choice.active)?;
        let origin = self.rows.get(index)?.origin.y;
        let goal = if forward {
            origin + self.viewport
        } else {
            origin - self.viewport
        };
        let candidates = self
            .rows
            .iter()
            .zip(&self.properties.items)
            .enumerate()
            .filter(|(i, (_, item))| {
                !item.disabled && if forward { *i > index } else { *i < index }
            });
        if forward {
            let mut last = None;
            for (_, (row, item)) in candidates {
                last = Some(item.id);
                if row.origin.y >= goal {
                    return last;
                }
            }
            last.or(self.choice.active)
        } else {
            let mut first = None;
            for (_, (row, item)) in candidates.rev() {
                first = Some(item.id);
                if row.origin.y <= goal {
                    return first;
                }
            }
            first.or(self.choice.active)
        }
    }
    pub(crate) fn item_rect(&self, id: ListItemId, bounds: Rect) -> Option<Rect> {
        let index = self
            .properties
            .items
            .iter()
            .position(|item| item.id == id)?;
        let row = self.rows.get(index)?;
        Some(Rect::new(
            bounds.origin.x,
            bounds.origin.y + row.origin.y - self.offset,
            row.size.width,
            row.size.height,
        ))
    }
    pub(crate) fn hit(&self, context: EventContext) -> Option<ListItemId> {
        let bounds = context.geometry?.content_bounds;
        let p = context.local_position?;
        if self.bar(bounds).is_some_and(|bar| bar.track.contains(p)) {
            return None;
        }
        if !bounds.contains(p) {
            return None;
        }
        let p = Point::new(p.x - bounds.origin.x, p.y - bounds.origin.y + self.offset);
        self.rows
            .iter()
            .zip(&self.properties.items)
            .find(|(row, item)| row.contains(p) && !item.disabled)
            .map(|(_, item)| item.id)
    }
}
impl ListBox {
    fn icon_slots(&self) -> (f64, f64) {
        let slot = self.properties.text_style.font_size as f64 * 1.4;
        (
            if self
                .properties
                .items
                .iter()
                .any(|i| i.leading_icon.is_some())
            {
                slot
            } else {
                0.0
            },
            if self
                .properties
                .items
                .iter()
                .any(|i| i.trailing_icon.is_some())
            {
                slot
            } else {
                0.0
            },
        )
    }
}
impl Widget for ListBox {
    fn enabled(&self) -> bool {
        !self.properties.disabled && self.properties.items.iter().any(|item| !item.disabled)
    }
    fn focusable(&self) -> bool {
        self.enabled()
    }
    fn measure(&mut self, context: MeasureContext) -> Result<Size, LayoutError> {
        self.properties.validate(context.id)?;
        let s = self.properties.style;
        let mut input = context.input;
        let (leading, trailing) = self.icon_slots();
        let inset = s.padding_x as f64 * 2.0 + leading + trailing;
        input.known_width = input
            .known_width
            .map(|v| (v as f64 - inset).max(0.0) as f32);
        if let AvailableSpace::Definite(width) = input.available_width {
            input.available_width =
                AvailableSpace::Definite((width as f64 - inset).max(0.0) as f32);
        }
        input.known_height = None;
        input.available_height = AvailableSpace::MaxContent;
        let mut size = Size::ZERO;
        for (text, item) in self.texts.iter_mut().zip(&self.properties.items) {
            let mut input = input;
            input.known_width = input.known_width.map(|v| (v - item.indent).max(0.0));
            if let AvailableSpace::Definite(w) = input.available_width {
                input.available_width = AvailableSpace::Definite((w - item.indent).max(0.0));
            }
            let label = text.measure(MeasureContext {
                id: context.id,
                input,
                text: context.text,
            })?;
            size.width = size.width.max(label.width + inset + item.indent as f64);
            size.height += (label.height + s.padding_y as f64 * 2.0).max(s.min_row_height as f64);
        }
        size.height = (size.height + self.texts.len().saturating_sub(1) as f64 * s.row_gap as f64)
            .min(s.natural_max_height as f64);
        if let AvailableSpace::Definite(height) = context.input.available_height {
            size.height = size.height.min(height as f64);
        }
        if let Some(width) = context.input.known_width {
            size.width = width as f64;
        }
        if let Some(height) = context.input.known_height {
            size.height = height as f64;
        }
        Ok(size)
    }
    fn prepare(&mut self, context: PrepareContext) -> Result<(), LayoutError> {
        self.properties.validate(context.id)?;
        let s = self.properties.style;
        let (leading, trailing) = self.icon_slots();
        let width =
            (context.content_size.width - s.padding_x as f64 * 2.0 - leading - trailing).max(0.0);
        if !self.rows.is_empty()
            && (self.width != context.content_size.width
                || self.viewport != context.content_size.height)
        {
            self.grab = None;
            self.choice.update(
                &self.properties.items,
                self.properties.selected,
                false,
                true,
            );
            self.reveal |= self.choice.interaction.focused;
        }
        self.width = context.content_size.width;
        self.rows.clear();
        let mut y = 0.0;
        for (text, item) in self.texts.iter_mut().zip(&self.properties.items) {
            let width = (width - item.indent as f64).max(0.0);
            text.prepare(PrepareContext {
                id: context.id,
                layout: LayoutBox::new(Rect::new(0.0, 0.0, width, context.content_size.height)),
                content_size: Size::new(width, context.content_size.height),
                text: context.text,
            })?;
            let height = (text.prepared_size().height + s.padding_y as f64 * 2.0)
                .max(s.min_row_height as f64);
            self.rows
                .push(Rect::new(0.0, y, context.content_size.width, height));
            y += height + s.row_gap as f64;
        }
        self.extent = self
            .rows
            .last()
            .map_or(0.0, |row| row.origin.y + row.size.height);
        self.reveal |=
            self.choice.interaction.focused && self.viewport != context.content_size.height;
        self.viewport = context.content_size.height;
        self.offset = self
            .offset
            .clamp(0.0, (self.extent - self.viewport).max(0.0));
        if self.reveal {
            self.reveal_active();
        }
        Ok(())
    }
    fn paint(&self, context: PaintContext, painter: &mut Painter<'_>) {
        let (leading, trailing) = self.icon_slots();
        let s = self.properties.style;
        // Cover gaps and unused space as well as rows, including empty results.
        // Dropdown lists can be painted over other content.
        painter.fill_rect(
            context.content_bounds,
            if self.properties.disabled {
                s.disabled.background
            } else {
                s.normal.background
            },
        );
        painter.with_clip(Path::rectangle(context.content_bounds), |painter| {
            for ((item, text), row) in self
                .properties
                .items
                .iter()
                .zip(&self.texts)
                .zip(&self.rows)
            {
                let y = row.origin.y - self.offset;
                if y + row.size.height <= 0.0 || y >= context.content_bounds.size.height {
                    continue;
                }
                let disabled = self.properties.disabled || item.disabled;
                let c = if disabled {
                    s.disabled
                } else if (self.choice.interaction.mouse
                    && self.choice.pressed == Some(item.id)
                    && self.choice.hovered == Some(item.id))
                    || (self.choice.interaction.space && self.choice.active == Some(item.id))
                {
                    s.pressed
                } else if self.choice.hovered == Some(item.id) {
                    s.hovered
                } else {
                    s.normal
                };
                let selected = self
                    .properties
                    .multi_selection
                    .as_ref()
                    .map_or(self.properties.selected == Some(item.id), |set| {
                        set.contains(&item.id)
                    });
                let rect = Rect::new(
                    context.content_bounds.origin.x,
                    context.content_bounds.origin.y + y,
                    row.size.width,
                    row.size.height,
                );
                painter.fill_rect(
                    rect,
                    if selected {
                        c.selected_background
                    } else {
                        c.background
                    },
                );
                let color = if selected { c.selected_text } else { c.text };
                let side = (self.properties.text_style.font_size as f64).min(rect.size.height);
                let inner = Rect::new(
                    rect.origin.x + s.padding_x as f64 + item.indent as f64,
                    rect.origin.y,
                    (rect.size.width - s.padding_x as f64 * 2.0 - item.indent as f64).max(0.0),
                    rect.size.height,
                );
                painter.with_clip(Path::rectangle(inner), |p| {
                    if let Some(icon) = &item.leading_icon {
                        icon.paint(
                            Rect::new(
                                inner.origin.x,
                                rect.origin.y + (rect.size.height - side) / 2.0,
                                side,
                                side,
                            ),
                            color,
                            p,
                        );
                    }
                    if let Some(icon) = &item.trailing_icon {
                        icon.paint(
                            Rect::new(
                                inner.origin.x + inner.size.width - side,
                                rect.origin.y + (rect.size.height - side) / 2.0,
                                side,
                                side,
                            ),
                            color,
                            p,
                        );
                    }
                });
                let label = Rect::new(
                    rect.origin.x + s.padding_x as f64 + leading + item.indent as f64,
                    rect.origin.y + (rect.size.height - text.prepared_size().height) / 2.0,
                    (rect.size.width
                        - s.padding_x as f64 * 2.0
                        - leading
                        - trailing
                        - item.indent as f64)
                        .max(0.0),
                    text.prepared_size().height,
                );
                painter.with_clip(Path::rectangle(label), |painter| {
                    text.paint_color(
                        PaintContext {
                            content_bounds: label,
                            ..context
                        },
                        painter,
                        if selected { c.selected_text } else { c.text },
                    )
                });
                if self.choice.interaction.focused
                    && self.choice.interaction.focus_visible
                    && !disabled
                    && self.choice.active == Some(item.id)
                    && rect.size.width > 4.0
                    && rect.size.height > 4.0
                {
                    painter.stroke(
                        Path::rectangle(Rect::new(
                            rect.origin.x + 2.0,
                            rect.origin.y + 2.0,
                            rect.size.width - 4.0,
                            rect.size.height - 4.0,
                        )),
                        s.focus_ring,
                        1.0,
                    );
                }
            }
        });
        if let Some(bar) = self.bar(context.content_bounds)
            && let Some(style) = self.properties.scrollbar
        {
            painter.fill_rect(bar.track, style.track);
            painter.fill_rect(
                bar.thumb,
                if !self.enabled() {
                    style.disabled
                } else if self.grab.is_some() {
                    style.dragging
                } else {
                    style.thumb
                },
            );
        }
    }
    fn event(&mut self, context: EventContext, event: &WidgetEvent) -> EventResponse {
        if self.properties.multi_selection.is_none() {
            return self.handle_event(context, event);
        }
        let mods = context.modifiers;
        if self.enabled()
            && context.phase == EventPhase::Target
            && let WidgetEvent::Input(InputEvent::Key(k)) = event
            && k.pressed
            && !k.repeat
            && (mods.control || mods.super_key)
            && !mods.alt
            && matches!(&k.key,Key::Character(s) if s.eq_ignore_ascii_case("a"))
        {
            return EventResponse {
                actions: alloc::vec![WidgetAction::new(
                    self.properties.action,
                    ListMultiSelectionChanged {
                        selected: self
                            .properties
                            .items
                            .iter()
                            .filter(|i| !i.disabled)
                            .map(|i| i.id)
                            .collect()
                    }
                )],
                handled: true,
                ..Default::default()
            };
        }
        let old_active = self.choice.active;
        let mut forwarded = event.clone();
        if let WidgetEvent::Input(InputEvent::Key(k)) = &mut forwarded
            && matches!(
                k.key,
                Key::ArrowUp
                    | Key::ArrowDown
                    | Key::ArrowLeft
                    | Key::ArrowRight
                    | Key::Home
                    | Key::End
                    | Key::PageUp
                    | Key::PageDown
                    | Key::Space
            )
        {
            k.modifiers.control = false;
            k.modifiers.super_key = false;
        }
        let mut response = self.handle_event(context, &forwarded);
        let navigation = matches!(event,WidgetEvent::Input(InputEvent::Key(k)) if matches!(k.key,Key::ArrowUp|Key::ArrowDown|Key::ArrowLeft|Key::ArrowRight|Key::Home|Key::End|Key::PageUp|Key::PageDown));
        if response.actions.is_empty() {
            return response;
        }
        let current = self
            .properties
            .multi_selection
            .as_ref()
            .expect("multi-selection mode")
            .clone();
        response.actions.retain_mut(|action| {
            let Some(target) = action.payload::<ListSelectionChanged>().map(|e| e.selected) else {
                return true;
            };
            if navigation && (mods.control || mods.super_key) && !mods.shift {
                return false;
            }
            let mut selected = if mods.control || mods.super_key {
                current.clone()
            } else {
                BTreeSet::new()
            };
            if mods.shift {
                let anchor = *self
                    .range_anchor
                    .get_or_insert(old_active.unwrap_or(target));
                let a = self
                    .properties
                    .items
                    .iter()
                    .position(|i| i.id == anchor)
                    .unwrap_or(0);
                let b = self
                    .properties
                    .items
                    .iter()
                    .position(|i| i.id == target)
                    .unwrap_or(a);
                selected.extend(
                    self.properties.items[a.min(b)..=a.max(b)]
                        .iter()
                        .filter(|i| !i.disabled)
                        .map(|i| i.id),
                );
            } else {
                self.range_anchor = Some(target);
                if !selected.remove(&target) {
                    selected.insert(target);
                }
            }
            *action = WidgetAction::new(action.id, ListMultiSelectionChanged { selected });
            true
        });
        response
    }
}
impl ListBox {
    fn handle_event(&mut self, context: EventContext, event: &WidgetEvent) -> EventResponse {
        let grab_canceled = if matches!(
            event,
            WidgetEvent::PointerCaptureLost | WidgetEvent::FocusChanged(false)
        ) {
            self.grab.take().is_some()
        } else {
            false
        };
        let suppress_reveal = self.bar_focus;
        if matches!(event, WidgetEvent::FocusChanged(_)) {
            self.bar_focus = false;
        }
        if let Some(response) = self.bar_event(context, event) {
            return response;
        }

        if self.enabled()
            && self.choice.interaction.focused
            && context.phase == EventPhase::Target
            && let WidgetEvent::Input(InputEvent::Key(key)) = event
            && !key.modifiers.control
            && !key.modifiers.alt
            && !key.modifiers.super_key
        {
            if self.properties.activation_only && key.key == Key::Enter {
                let actions = if key.pressed && !key.repeat {
                    self.choice
                        .active
                        .into_iter()
                        .map(|selected| {
                            WidgetAction::new(
                                self.properties.action,
                                ListSelectionChanged { selected },
                            )
                        })
                        .collect()
                } else {
                    Vec::new()
                };
                return EventResponse {
                    actions,
                    handled: true,
                    ..Default::default()
                };
            }
            if matches!(key.key, Key::PageUp | Key::PageDown) {
                if key.pressed
                    && let Some(target) = self.page_target(key.key == Key::PageDown)
                {
                    return self.navigate_to(target);
                }
                return EventResponse {
                    handled: true,
                    ..Default::default()
                };
            }
            if let Key::Character(character) = &key.key {
                let prefix = key.text.as_deref().unwrap_or(character);
                if !prefix.is_empty()
                    && !prefix.chars().any(char::is_control)
                    && !prefix.chars().all(char::is_whitespace)
                    && let Some(target) = self.prefix_target(prefix)
                {
                    return if key.pressed {
                        self.navigate_to(target)
                    } else {
                        EventResponse {
                            handled: true,
                            ..Default::default()
                        }
                    };
                }
            }
        }
        if self.enabled()
            && context.phase == EventPhase::Target
            && let WidgetEvent::Input(InputEvent::Scroll(delta)) = event
        {
            let amount = match delta {
                ScrollDelta::Lines { y, .. } => {
                    *y * self.properties.style.min_row_height.max(1.0) as f64
                }
                ScrollDelta::Pixels { y, .. } => *y,
            };
            let offset = (self.offset - amount).clamp(0.0, (self.extent - self.viewport).max(0.0));
            if offset != self.offset {
                self.offset = offset;
                self.grab = None;
                self.choice.update(
                    &self.properties.items,
                    self.properties.selected,
                    false,
                    true,
                );
                return EventResponse {
                    handled: true,
                    stop_propagation: true,
                    invalidation: Invalidation::Paint,
                    capture: CaptureRequest::Release,
                    ..Default::default()
                };
            }
        }
        let active = self.choice.active;
        let hit = self.hit(context);
        let (mut response, selected) = self.choice.event(
            context,
            event,
            self.enabled(),
            &self.properties.items,
            if self.properties.multi_selection.is_some()
                || self.properties.activation_only
                    && matches!(
                        event,
                        WidgetEvent::Input(InputEvent::PointerButton { .. })
                            | WidgetEvent::Input(InputEvent::Key(KeyInput {
                                key: Key::Space,
                                ..
                            }))
                    )
            {
                None
            } else {
                self.properties.selected
            },
            hit,
        );
        if self.properties.activation_only
            && matches!(event, WidgetEvent::FocusChanged(true))
            && active.is_some_and(|id| {
                self.properties
                    .items
                    .iter()
                    .any(|item| item.id == id && !item.disabled)
            })
        {
            self.choice.active = active;
        }
        let navigation = response.handled
            && matches!(event, WidgetEvent::Input(InputEvent::Key(key)) if key.pressed && matches!(key.key, Key::ArrowUp | Key::ArrowDown | Key::ArrowLeft | Key::ArrowRight | Key::Home | Key::End));
        if navigation {
            self.grab = None;
        }
        if grab_canceled {
            response.invalidation = response.invalidation.max(Invalidation::Paint);
        }
        if ((active != self.choice.active || matches!(event, WidgetEvent::FocusChanged(true)))
            && !(suppress_reveal && matches!(event, WidgetEvent::FocusChanged(true))))
            || navigation
        {
            self.reveal_active();
            response.invalidation = response.invalidation.max(Invalidation::Paint);
        }
        if let Some(selected) = selected.filter(|_| !navigation || !self.properties.activation_only)
        {
            response.actions.push(WidgetAction::new(
                self.properties.action,
                ListSelectionChanged { selected },
            ));
        }
        response
    }
}
