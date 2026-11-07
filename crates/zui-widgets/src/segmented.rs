//! Controlled single-choice segments with one focus stop and equal-width cells.
use crate::{
    Text, TextView,
    choice::{ChoiceItem, ChoiceState},
};
use alloc::{collections::BTreeSet, string::String, vec::Vec};
use zui_api::{
    action::{ActionId, WidgetAction},
    event::*,
    layout::{AvailableSpace, LayoutBox, LayoutError},
    text::{TextAlign, TextStyle},
    view::ViewSpec,
    widget::{Invalidation, MeasureContext, PaintContext, PrepareContext, Widget},
};
use zui_paint::{Painter, Path};
use zui_theme::{ColorRole, Theme, TypeRole};
use zui_types::{Color, NodeId, Point, Rect, Size};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SegmentId(pub u64);
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SegmentOption {
    pub id: SegmentId,
    pub label: String,
    pub disabled: bool,
}
impl SegmentOption {
    pub fn new(id: SegmentId, label: impl Into<String>) -> Self {
        Self {
            id,
            label: label.into(),
            disabled: false,
        }
    }
}
impl ChoiceItem for SegmentOption {
    type Id = SegmentId;
    fn id(&self) -> SegmentId {
        self.id
    }
    fn disabled(&self) -> bool {
        self.disabled
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SegmentChanged {
    pub selected: SegmentId,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SegmentColors {
    pub background: Color,
    pub text: Color,
    pub selected_background: Color,
    pub selected_text: Color,
    pub border: Color,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SegmentedStyle {
    pub normal: SegmentColors,
    pub hovered: SegmentColors,
    pub pressed: SegmentColors,
    pub disabled: SegmentColors,
    pub focus_ring: Color,
    pub padding_x: f32,
    pub padding_y: f32,
    pub gap: f32,
    pub min_height: f32,
}
impl SegmentedStyle {
    pub fn from_theme(theme: &Theme) -> Self {
        let normal = SegmentColors {
            background: theme.color(ColorRole::Surface),
            text: theme.color(ColorRole::Text),
            selected_background: theme.selection_background(),
            selected_text: theme.selection_foreground(),
            border: theme.color(ColorRole::Border),
        };
        Self {
            normal,
            hovered: SegmentColors {
                border: theme.color(ColorRole::Accent),
                ..normal
            },
            pressed: SegmentColors {
                background: theme.color(ColorRole::SurfaceRaised),
                border: theme.color(ColorRole::Text),
                ..normal
            },
            disabled: SegmentColors {
                text: theme.color(ColorRole::TextMuted),
                selected_text: theme.color(ColorRole::TextMuted),
                selected_background: theme.color(ColorRole::SurfaceRaised),
                ..normal
            },
            focus_ring: theme.color(ColorRole::Text),
            padding_x: 12.0,
            padding_y: 8.0,
            gap: 2.0,
            min_height: 32.0,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct SegmentedControlView {
    pub options: Vec<SegmentOption>,
    pub selected: Option<SegmentId>,
    pub disabled: bool,
    pub action: ActionId,
    pub text_style: TextStyle,
    pub style: SegmentedStyle,
}
impl SegmentedControlView {
    pub fn themed(
        theme: &Theme,
        options: Vec<SegmentOption>,
        families: Vec<String>,
        selected: Option<SegmentId>,
        action: ActionId,
    ) -> Self {
        let metrics = theme.typography(TypeRole::Body);
        Self {
            options,
            selected,
            disabled: false,
            action,
            style: SegmentedStyle::from_theme(theme),
            text_style: TextStyle {
                families,
                font_size: metrics.font_size(),
                line_height: metrics.line_height(),
                align: TextAlign::Center,
            },
        }
    }
    fn validate(&self, id: NodeId) -> Result<(), LayoutError> {
        let s = self.style;
        let mut ids = BTreeSet::new();
        if [s.padding_x, s.padding_y, s.gap, s.min_height]
            .iter()
            .any(|v| !v.is_finite() || *v < 0.0)
            || !(s.padding_x * 2.0).is_finite()
            || !(s.padding_y * 2.0).is_finite()
            || self.options.iter().any(|option| !ids.insert(option.id))
            || self
                .selected
                .is_some_and(|selected| !ids.contains(&selected))
        {
            return Err(LayoutError::InvalidStyle(id));
        }
        Ok(())
    }
    fn slot_width(&self, width: f64) -> f64 {
        if self.options.is_empty() {
            return 0.0;
        }
        (width - self.options.len().saturating_sub(1) as f64 * f64::from(self.style.gap)).max(0.0)
            / self.options.len() as f64
    }
    fn texts(&self) -> Vec<Text> {
        self.options
            .iter()
            .map(|option| {
                TextView::new(
                    option.label.clone(),
                    self.text_style.clone(),
                    self.style.normal.text,
                )
                .build()
            })
            .collect()
    }
}
pub struct SegmentedControl {
    properties: SegmentedControlView,
    choice: ChoiceState<SegmentId>,
    texts: Vec<Text>,
    slots: Vec<Rect>,
    pub(crate) compact: bool,
    pub(crate) joined: bool,
}
impl ViewSpec for SegmentedControlView {
    type Widget = SegmentedControl;
    fn build(&self) -> SegmentedControl {
        SegmentedControl {
            properties: self.clone(),
            choice: ChoiceState::new(&self.options, self.selected),
            texts: self.texts(),
            slots: Vec::new(),
            compact: false,
            joined: false,
        }
    }
    fn update(&self, widget: &mut SegmentedControl) -> Invalidation {
        if self == &widget.properties {
            return Invalidation::None;
        }
        let old = &widget.properties;
        let text_changed = self.options != old.options || self.text_style != old.text_style;
        let geometry = text_changed
            || self.style.padding_x != old.style.padding_x
            || self.style.padding_y != old.style.padding_y
            || self.style.gap != old.style.gap
            || self.style.min_height != old.style.min_height
            || self
                .selected
                .is_some_and(|id| !self.options.iter().any(|option| option.id == id));
        widget.choice.update(
            &self.options,
            self.selected,
            self.selected != old.selected,
            geometry || self.selected != old.selected || self.disabled || self.action != old.action,
        );
        if text_changed {
            widget.texts = self.texts();
        }
        if geometry {
            widget.slots.clear();
        }
        widget.properties = self.clone();
        if geometry {
            Invalidation::Layout
        } else {
            Invalidation::Paint
        }
    }
}
impl SegmentedControl {
    pub(crate) fn slots(&self) -> &[Rect] {
        &self.slots
    }
    fn compact_widths(
        &mut self,
        id: NodeId,
        service: &mut dyn zui_api::text::TextService,
        available: f64,
    ) -> Result<Vec<f64>, LayoutError> {
        let mut widths = Vec::new();
        for text in &mut self.texts {
            let size = text.measure(MeasureContext {
                id,
                text: service,
                input: zui_api::layout::MeasureInput {
                    known_width: None,
                    known_height: None,
                    available_width: AvailableSpace::MaxContent,
                    available_height: AvailableSpace::MaxContent,
                },
            })?;
            widths.push(size.width + f64::from(self.properties.style.padding_x) * 2.0);
        }
        let total: f64 = widths.iter().sum();
        let gaps = widths.len().saturating_sub(1) as f64 * f64::from(self.properties.style.gap);
        let room = (available - gaps).max(0.0);
        if total > room && total > 0.0 {
            for width in &mut widths {
                *width *= room / total;
            }
        }
        Ok(widths)
    }

    fn option_at(&self, context: EventContext) -> Option<SegmentId> {
        let bounds = context.geometry?.content_bounds;
        let point = context.local_position?;
        if !bounds.contains(point) {
            return None;
        }
        let point = Point::new(point.x - bounds.origin.x, point.y - bounds.origin.y);
        self.slots
            .iter()
            .zip(&self.properties.options)
            .find(|(slot, option)| slot.contains(point) && !option.disabled)
            .map(|(_, option)| option.id)
    }
    fn colors(&self, option: &SegmentOption) -> SegmentColors {
        let style = self.properties.style;
        if self.properties.disabled || option.disabled {
            style.disabled
        } else if (self.choice.interaction.mouse
            && self.choice.pressed == Some(option.id)
            && self.choice.hovered == Some(option.id))
            || (self.choice.interaction.space && self.choice.active == Some(option.id))
        {
            style.pressed
        } else if self.choice.hovered == Some(option.id) {
            style.hovered
        } else {
            style.normal
        }
    }
}
impl Widget for SegmentedControl {
    fn enabled(&self) -> bool {
        !self.properties.disabled
            && self
                .properties
                .options
                .iter()
                .any(|option| !option.disabled)
    }
    fn focusable(&self) -> bool {
        self.enabled()
    }
    fn measure(&mut self, context: MeasureContext) -> Result<Size, LayoutError> {
        self.properties.validate(context.id)?;
        let style = self.properties.style;
        let inset = f64::from(style.padding_x) * 2.0;
        if self.compact {
            let available = context.input.known_width.map(f64::from).unwrap_or(
                match context.input.available_width {
                    AvailableSpace::Definite(w) => f64::from(w),
                    AvailableSpace::MinContent => 0.0,
                    AvailableSpace::MaxContent => f64::MAX,
                },
            );
            let widths = self.compact_widths(context.id, context.text, available)?;
            let mut height = 0.0_f64;
            for (text, width) in self.texts.iter_mut().zip(&widths) {
                let size = text.measure(MeasureContext {
                    id: context.id,
                    text: context.text,
                    input: zui_api::layout::MeasureInput {
                        known_width: Some((width - inset).max(0.0) as f32),
                        known_height: None,
                        available_width: AvailableSpace::Definite((width - inset).max(0.0) as f32),
                        available_height: AvailableSpace::MaxContent,
                    },
                })?;
                height = height.max(
                    (size.height + f64::from(style.padding_y) * 2.0)
                        .max(f64::from(style.min_height)),
                );
            }
            return Ok(Size::new(
                context.input.known_width.map(f64::from).unwrap_or(
                    widths.iter().sum::<f64>()
                        + widths.len().saturating_sub(1) as f64 * f64::from(style.gap),
                ),
                context.input.known_height.map(f64::from).unwrap_or(height),
            ));
        }
        let mut input = context.input;
        input.known_width = input
            .known_width
            .map(|w| (self.properties.slot_width(w as f64) - inset).max(0.0) as f32);
        input.known_height = None;
        input.available_height = AvailableSpace::MaxContent;
        if let AvailableSpace::Definite(width) = input.available_width {
            input.available_width = AvailableSpace::Definite(
                (self.properties.slot_width(width as f64) - inset).max(0.0) as f32,
            );
        }
        let mut size = Size::ZERO;
        for text in &mut self.texts {
            let label = text.measure(MeasureContext {
                id: context.id,
                input,
                text: context.text,
            })?;
            size.width = size.width.max(label.width + inset);
            size.height = size.height.max(
                (label.height + f64::from(style.padding_y) * 2.0).max(style.min_height as f64),
            );
        }
        size.width = size.width * self.texts.len() as f64
            + self.texts.len().saturating_sub(1) as f64 * style.gap as f64;
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
        self.slots.clear();
        let style = self.properties.style;
        let widths = if self.compact {
            self.compact_widths(context.id, context.text, context.content_size.width)?
        } else {
            alloc::vec![self.properties.slot_width(context.content_size.width); self.texts.len()]
        };
        let mut x = 0.0;
        for (text, slot_width) in self.texts.iter_mut().zip(widths) {
            let label_size = Size::new(
                (slot_width - style.padding_x as f64 * 2.0).max(0.0),
                (context.content_size.height - style.padding_y as f64 * 2.0).max(0.0),
            );
            text.prepare(PrepareContext {
                id: context.id,
                layout: LayoutBox::new(Rect::new(0.0, 0.0, label_size.width, label_size.height)),
                content_size: label_size,
                text: context.text,
            })?;
            self.slots
                .push(Rect::new(x, 0.0, slot_width, context.content_size.height));
            x += slot_width + f64::from(style.gap);
        }
        Ok(())
    }
    fn paint(&self, context: PaintContext, painter: &mut Painter<'_>) {
        let style = self.properties.style;
        painter.with_clip(Path::rectangle(context.content_bounds), |painter| {
            for ((option, text), slot) in self
                .properties
                .options
                .iter()
                .zip(&self.texts)
                .zip(&self.slots)
            {
                let rect = Rect::new(
                    context.content_bounds.origin.x + slot.origin.x,
                    context.content_bounds.origin.y,
                    slot.size.width,
                    slot.size.height,
                );
                let colors = self.colors(option);
                let selected = self.properties.selected == Some(option.id);
                painter.fill_rect(
                    rect,
                    if selected {
                        colors.selected_background
                    } else {
                        colors.background
                    },
                );
                if !self.joined && rect.size.width >= 1.0 && rect.size.height >= 1.0 {
                    painter.stroke(
                        Path::rectangle(Rect::new(
                            rect.origin.x + 0.5,
                            rect.origin.y + 0.5,
                            rect.size.width - 1.0,
                            rect.size.height - 1.0,
                        )),
                        colors.border,
                        1.0,
                    );
                }
                let label = Rect::new(
                    rect.origin.x + style.padding_x as f64,
                    rect.origin.y + style.padding_y as f64,
                    (rect.size.width - style.padding_x as f64 * 2.0).max(0.0),
                    (rect.size.height - style.padding_y as f64 * 2.0).max(0.0),
                );
                painter.with_clip(Path::rectangle(label), |painter| {
                    let centered = Rect::new(
                        label.origin.x,
                        label.origin.y
                            + ((label.size.height - text.prepared_size().height) / 2.0).max(0.0),
                        label.size.width,
                        label.size.height,
                    );
                    text.paint_color(
                        PaintContext {
                            content_bounds: centered,
                            ..context
                        },
                        painter,
                        if selected {
                            colors.selected_text
                        } else {
                            colors.text
                        },
                    );
                });
                if self.choice.interaction.focused
                    && !self.properties.disabled
                    && !option.disabled
                    && self.choice.active == Some(option.id)
                    && rect.size.width > 6.0
                    && rect.size.height > 6.0
                {
                    painter.stroke(
                        Path::rectangle(Rect::new(
                            rect.origin.x + 3.0,
                            rect.origin.y + 3.0,
                            rect.size.width - 6.0,
                            rect.size.height - 6.0,
                        )),
                        style.focus_ring,
                        1.0,
                    );
                }
            }
            if self.joined
                && let Some(last) = self.slots.last()
            {
                let width = last.origin.x + last.size.width;
                let height = context.content_bounds.size.height;
                let color = if self.properties.disabled {
                    style.disabled.border
                } else {
                    style.normal.border
                };
                if width >= 1.0 && height >= 1.0 {
                    painter.stroke(
                        Path::rectangle(Rect::new(
                            context.content_bounds.origin.x + 0.5,
                            context.content_bounds.origin.y + 0.5,
                            width - 1.0,
                            height - 1.0,
                        )),
                        color,
                        1.0,
                    );
                    // Each shared boundary is drawn once, after all cell backgrounds.
                    for slot in self.slots.iter().skip(1) {
                        if slot.origin.x >= 1.0 && slot.origin.x < width - 1.0 {
                            painter.fill_rect(
                                Rect::new(
                                    context.content_bounds.origin.x + slot.origin.x,
                                    context.content_bounds.origin.y + 1.0,
                                    1.0,
                                    (height - 2.0).max(0.0),
                                ),
                                color,
                            );
                        }
                    }
                }
            }
        });
    }
    fn event(&mut self, context: EventContext, event: &WidgetEvent) -> EventResponse {
        let hit = self.option_at(context);
        let enabled = self.enabled();
        let (mut response, selected) = self.choice.event(
            context,
            event,
            enabled,
            &self.properties.options,
            self.properties.selected,
            hit,
        );
        if let Some(selected) = selected {
            response.actions.push(WidgetAction::new(
                self.properties.action,
                SegmentChanged { selected },
            ));
        }
        response
    }
}
