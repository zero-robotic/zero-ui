//! A controlled selection group with one runtime focus owner.
use crate::appearance::{Surface, VisualState};
use crate::{
    Text, TextView,
    choice::{ChoiceItem, ChoiceState},
};
use alloc::{collections::BTreeSet, string::String, vec::Vec};
use zui_api::{
    action::{ActionId, WidgetAction},
    event::*,
    layout::{AvailableSpace, LayoutError},
    text::TextStyle,
    view::ViewSpec,
    widget::{Invalidation, MeasureContext, PaintContext, PrepareContext, Widget},
};
use zui_paint::{Painter, Path};
use zui_theme::{ColorRole, Space, Theme, TypeRole};
use zui_types::{Color, NodeId, Point, Rect, Size};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RadioOptionId(pub u64);
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RadioOption {
    pub id: RadioOptionId,
    pub label: String,
    pub disabled: bool,
}
impl RadioOption {
    pub fn new(id: RadioOptionId, label: impl Into<String>) -> Self {
        Self {
            id,
            label: label.into(),
            disabled: false,
        }
    }
}
impl ChoiceItem for RadioOption {
    type Id = RadioOptionId;
    fn id(&self) -> RadioOptionId {
        self.id
    }
    fn disabled(&self) -> bool {
        self.disabled
    }
}
/// A request, not an optimistic mutation of the application's selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RadioChanged {
    pub selected: RadioOptionId,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RadioColors {
    pub background: Color,
    pub border: Color,
    pub dot: Color,
    pub text: Color,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RadioGroupStyle {
    pub normal: RadioColors,
    pub hovered: RadioColors,
    pub pressed: RadioColors,
    pub disabled: RadioColors,
    pub focus_ring: Color,
    pub indicator_size: f32,
    pub label_gap: f32,
    pub row_gap: f32,
}
impl RadioGroupStyle {
    pub fn from_theme(theme: &Theme) -> Self {
        let normal = RadioColors {
            background: theme.color(ColorRole::Surface),
            border: theme.color(ColorRole::Border),
            dot: theme.color(ColorRole::Accent),
            text: theme.color(ColorRole::Text),
        };
        Self {
            normal,
            hovered: RadioColors {
                border: theme.color(ColorRole::Accent),
                ..normal
            },
            pressed: RadioColors {
                background: theme.color(ColorRole::SurfaceRaised),
                ..normal
            },
            disabled: RadioColors {
                dot: theme.color(ColorRole::TextMuted),
                text: theme.color(ColorRole::TextMuted),
                ..normal
            },
            focus_ring: theme.color(ColorRole::Accent),
            indicator_size: if theme.controls().min_height > 0 {
                16.0
            } else {
                20.0
            },
            label_gap: theme.space(Space::Sm),
            row_gap: theme.space(Space::Sm),
        }
    }
    fn advance(self) -> f64 {
        self.indicator_size as f64 + self.label_gap as f64
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RadioOrientation {
    #[default]
    Vertical,
    /// Equal-width slots; labels wrap inside each slot. No automatic row wrapping.
    Horizontal,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RadioGroupView {
    pub orientation: RadioOrientation,
    pub options: Vec<RadioOption>,
    pub selected: Option<RadioOptionId>,
    pub text_style: TextStyle,
    pub style: RadioGroupStyle,
    pub disabled: bool,
    pub action: ActionId,
}
impl RadioGroupView {
    pub fn themed(
        theme: &Theme,
        options: Vec<RadioOption>,
        families: Vec<String>,
        selected: Option<RadioOptionId>,
        action: ActionId,
    ) -> Self {
        let metrics = theme.typography(TypeRole::Body);
        Self {
            orientation: RadioOrientation::Vertical,
            options,
            selected,
            action,
            disabled: false,
            text_style: TextStyle {
                families,
                font_size: metrics.font_size(),
                line_height: metrics.line_height(),
                align: Default::default(),
            },
            style: RadioGroupStyle::from_theme(theme),
        }
    }
    fn option_width(&self, width: f64) -> f64 {
        if self.orientation == RadioOrientation::Vertical {
            return width;
        }
        let gaps = self.options.len().saturating_sub(1) as f64 * self.style.row_gap as f64;
        if self.options.is_empty() {
            0.0
        } else {
            (width - gaps).max(0.0) / self.options.len() as f64
        }
    }
    fn texts(&self) -> Vec<Text> {
        self.options
            .iter()
            .map(|o| {
                TextView::new(
                    o.label.clone(),
                    self.text_style.clone(),
                    self.style.normal.text,
                )
                .build()
            })
            .collect()
    }
    fn validate(&self, id: NodeId) -> Result<(), LayoutError> {
        let s = self.style;
        let mut ids = BTreeSet::new();
        if !s.indicator_size.is_finite()
            || s.indicator_size < 4.0
            || !s.label_gap.is_finite()
            || s.label_gap < 0.0
            || !s.row_gap.is_finite()
            || s.row_gap < 0.0
            || !(s.indicator_size + s.label_gap).is_finite()
            || self.options.iter().any(|o| !ids.insert(o.id))
            || self
                .selected
                .is_some_and(|selected| !ids.contains(&selected))
        {
            return Err(LayoutError::InvalidStyle(id));
        }
        Ok(())
    }
}

pub struct RadioGroup {
    properties: RadioGroupView,
    texts: Vec<Text>,
    rows: Vec<Rect>,
    choice: ChoiceState<RadioOptionId>,
}
impl ViewSpec for RadioGroupView {
    type Widget = RadioGroup;
    fn build(&self) -> RadioGroup {
        RadioGroup {
            properties: self.clone(),
            texts: self.texts(),
            rows: Vec::new(),
            choice: ChoiceState::new(&self.options, self.selected),
        }
    }
    fn update(&self, widget: &mut RadioGroup) -> Invalidation {
        if self == &widget.properties {
            return Invalidation::None;
        }
        let old = &widget.properties;
        let text_changed = self.options != old.options || self.text_style != old.text_style;
        let geometry_changed = self.orientation != old.orientation
            || text_changed
            || self
                .selected
                .is_some_and(|id| !self.options.iter().any(|o| o.id == id))
            || self.style.indicator_size != old.style.indicator_size
            || self.style.label_gap != old.style.label_gap
            || self.style.row_gap != old.style.row_gap;
        widget.choice.update(
            &self.options,
            self.selected,
            self.selected != old.selected,
            text_changed
                || self.selected != old.selected
                || self.disabled
                || self.action != old.action
                || geometry_changed,
        );
        if text_changed {
            widget.texts = self.texts();
        }
        if geometry_changed {
            widget.rows.clear();
        }
        widget.properties = self.clone();
        if geometry_changed {
            Invalidation::Layout
        } else {
            Invalidation::Paint
        }
    }
}
impl RadioGroup {
    fn option_at(&self, context: EventContext) -> Option<RadioOptionId> {
        let bounds = context.geometry?.content_bounds;
        let p = context.local_position?;
        if !bounds.contains(p) {
            return None;
        }
        let local = Point::new(p.x - bounds.origin.x, p.y - bounds.origin.y);
        self.rows
            .iter()
            .zip(&self.properties.options)
            .find(|(r, o)| r.contains(local) && !o.disabled)
            .map(|(_, o)| o.id)
    }
}

impl Widget for RadioGroup {
    fn enabled(&self) -> bool {
        !self.properties.disabled && self.properties.options.iter().any(|o| !o.disabled)
    }
    fn focusable(&self) -> bool {
        self.enabled()
    }
    fn measure(&mut self, context: MeasureContext) -> Result<Size, LayoutError> {
        self.properties.validate(context.id)?;
        let mut input = context.input;
        let advance = self.properties.style.advance();
        input.known_width = input
            .known_width
            .map(|w| (self.properties.option_width(w as f64) - advance).max(0.0) as f32);
        input.known_height = None;
        input.available_height = AvailableSpace::MaxContent;
        if let AvailableSpace::Definite(w) = input.available_width {
            input.available_width = AvailableSpace::Definite(
                (self.properties.option_width(w as f64) - advance).max(0.0) as f32,
            );
        }
        let mut size = Size::ZERO;
        for text in &mut self.texts {
            let label = text.measure(MeasureContext {
                id: context.id,
                input,
                text: context.text,
            })?;
            size.width = size.width.max(label.width + advance);
            let height = label
                .height
                .max(self.properties.style.indicator_size as f64);
            if self.properties.orientation == RadioOrientation::Horizontal {
                size.height = size.height.max(height);
            } else {
                size.height += height;
            }
        }
        let gaps = self.texts.len().saturating_sub(1) as f64 * self.properties.style.row_gap as f64;
        if self.properties.orientation == RadioOrientation::Horizontal {
            size.width = size.width * self.texts.len() as f64 + gaps;
        } else {
            size.height += gaps;
        }
        if let Some(w) = context.input.known_width {
            size.width = w as f64;
        }
        if let Some(h) = context.input.known_height {
            size.height = h as f64;
        }
        Ok(size)
    }
    fn prepare(&mut self, context: PrepareContext) -> Result<(), LayoutError> {
        self.properties.validate(context.id)?;
        self.rows.clear();
        let slot_width = self.properties.option_width(context.content_size.width);
        let width = (slot_width - self.properties.style.advance()).max(0.0);
        let mut y = 0.0;
        for (index, text) in self.texts.iter_mut().enumerate() {
            text.prepare(PrepareContext {
                id: context.id,
                layout: zui_api::layout::LayoutBox::new(Rect::new(
                    0.0,
                    0.0,
                    width,
                    context.content_size.height,
                )),
                content_size: Size::new(width, context.content_size.height),
                text: context.text,
            })?;
            let height = text
                .prepared_size()
                .height
                .max(self.properties.style.indicator_size as f64);
            self.rows.push(
                if self.properties.orientation == RadioOrientation::Horizontal {
                    Rect::new(
                        index as f64 * (slot_width + self.properties.style.row_gap as f64),
                        0.0,
                        slot_width,
                        height,
                    )
                } else {
                    Rect::new(0.0, y, slot_width, height)
                },
            );
            y += height + self.properties.style.row_gap as f64;
        }
        if self.properties.orientation == RadioOrientation::Horizontal {
            let height = self
                .rows
                .iter()
                .map(|row| row.size.height)
                .fold(0.0, f64::max);
            for row in &mut self.rows {
                row.size.height = height;
            }
        }
        Ok(())
    }
    fn paint(&self, context: PaintContext, painter: &mut Painter<'_>) {
        let style = self.properties.style;
        painter.with_clip(Path::rectangle(context.content_bounds), |painter| {
            for ((option, text), row) in self
                .properties
                .options
                .iter()
                .zip(&self.texts)
                .zip(&self.rows)
            {
                let disabled = self.properties.disabled || option.disabled;
                let state = VisualState::from_choice(&self.choice, option.id, disabled);
                let colors =
                    state.resolve(style.normal, style.hovered, style.pressed, style.disabled);
                let row = Rect::new(
                    context.content_bounds.origin.x + row.origin.x,
                    context.content_bounds.origin.y + row.origin.y,
                    row.size.width,
                    row.size.height,
                );
                let diameter = style.indicator_size as f64;
                let center = Point::new(
                    row.origin.x + diameter / 2.0,
                    row.origin.y + row.size.height / 2.0,
                );
                Surface {
                    bounds: Rect::new(
                        center.x - diameter / 2.0,
                        center.y - diameter / 2.0,
                        diameter,
                        diameter,
                    ),
                    radius: diameter / 2.0,
                }
                .paint(painter, colors.background, colors.border);
                if self.properties.selected == Some(option.id) {
                    painter.fill(
                        Path::circle(center, diameter * 0.23).expect("validated radio geometry"),
                        colors.dot,
                    );
                }
                let label = Rect::new(
                    row.origin.x + style.advance(),
                    row.origin.y + (row.size.height - text.prepared_size().height) / 2.0,
                    (row.size.width - style.advance()).max(0.0),
                    row.size.height,
                );
                painter.with_clip(Path::rectangle(label), |painter| {
                    text.paint_color(
                        PaintContext {
                            content_bounds: label,
                            ..context
                        },
                        painter,
                        colors.text,
                    )
                });
                Surface {
                    bounds: row,
                    radius: 0.0,
                }
                .focus(painter, state, style.focus_ring);
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
                RadioChanged { selected },
            ));
        }
        response
    }
}
