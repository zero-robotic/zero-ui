use crate::{Text, TextView, interaction::Interaction};
use alloc::{string::String, vec::Vec};
use zui_api::{
    action::{ActionId, WidgetAction},
    event::{EventContext, EventResponse, WidgetEvent},
    layout::{AvailableSpace, LayoutError},
    text::TextStyle,
    view::ViewSpec,
    widget::{Invalidation, MeasureContext, PaintContext, PrepareContext, Widget},
};
use zui_paint::{Painter, Path};
use zui_theme::{ColorRole, Space, Theme, TypeRole};
use zui_types::{Color, NodeId, Point, Rect, Size};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CheckState {
    #[default]
    Unchecked,
    Checked,
    Mixed,
}
/// Requested next state. The application decides whether to apply it to the View.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CheckChanged {
    pub state: CheckState,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CheckBoxColors {
    pub background: Color,
    pub selected_background: Color,
    pub border: Color,
    pub mark: Color,
    pub text: Color,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CheckBoxStyle {
    pub normal: CheckBoxColors,
    pub hovered: CheckBoxColors,
    pub pressed: CheckBoxColors,
    pub disabled: CheckBoxColors,
    pub focus_ring: Color,
    pub indicator_size: f32,
    pub gap: f32,
}
impl CheckBoxStyle {
    pub fn from_theme(theme: &Theme) -> Self {
        let normal = CheckBoxColors {
            background: theme.color(ColorRole::Surface),
            selected_background: theme.color(ColorRole::Accent),
            border: theme.color(ColorRole::Border),
            mark: theme.color(ColorRole::OnAccent),
            text: theme.color(ColorRole::Text),
        };
        Self {
            normal,
            hovered: CheckBoxColors {
                border: theme.color(ColorRole::Accent),
                ..normal
            },
            pressed: CheckBoxColors {
                background: theme.color(ColorRole::SurfaceRaised),
                border: theme.color(ColorRole::Text),
                ..normal
            },
            disabled: CheckBoxColors {
                background: theme.color(ColorRole::SurfaceRaised),
                selected_background: theme.color(ColorRole::SurfaceRaised),
                border: theme.color(ColorRole::Border),
                mark: theme.color(ColorRole::TextMuted),
                text: theme.color(ColorRole::TextMuted),
            },
            focus_ring: theme.color(ColorRole::Text),
            indicator_size: if theme.controls().min_height > 0 {
                16.0
            } else {
                20.0
            },
            gap: theme.space(Space::Sm),
        }
    }
    fn validate(&self, id: NodeId) -> Result<(), LayoutError> {
        if !self.indicator_size.is_finite()
            || self.indicator_size < 4.0
            || !self.gap.is_finite()
            || self.gap < 0.0
            || !(self.indicator_size + self.gap).is_finite()
        {
            return Err(LayoutError::InvalidStyle(id));
        }
        Ok(())
    }
    fn advance(self) -> f32 {
        self.indicator_size + self.gap
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct CheckBoxView {
    pub label: String,
    pub text_style: TextStyle,
    pub style: CheckBoxStyle,
    pub state: CheckState,
    pub disabled: bool,
    pub action: ActionId,
}
impl CheckBoxView {
    pub fn themed(
        theme: &Theme,
        label: impl Into<String>,
        families: Vec<String>,
        state: CheckState,
        action: ActionId,
    ) -> Self {
        let metrics = theme.typography(TypeRole::Body);
        Self {
            label: label.into(),
            text_style: TextStyle {
                families,
                font_size: metrics.font_size(),
                line_height: metrics.line_height(),
                align: Default::default(),
            },
            style: CheckBoxStyle::from_theme(theme),
            state,
            disabled: false,
            action,
        }
    }
    fn text(&self) -> TextView {
        TextView::new(
            self.label.clone(),
            self.text_style.clone(),
            self.style.normal.text,
        )
    }
}
pub struct CheckBox {
    properties: CheckBoxView,
    text: Text,
    interaction: Interaction,
}
impl ViewSpec for CheckBoxView {
    type Widget = CheckBox;
    fn build(&self) -> CheckBox {
        CheckBox {
            properties: self.clone(),
            text: self.text().build(),
            interaction: Default::default(),
        }
    }
    fn update(&self, widget: &mut CheckBox) -> Invalidation {
        let mut invalidation = self.text().update(&mut widget.text);
        if self.style.indicator_size != widget.properties.style.indicator_size
            || self.style.gap != widget.properties.style.gap
        {
            invalidation = Invalidation::Layout;
        } else if self != &widget.properties {
            invalidation = invalidation.max(Invalidation::Paint);
        }
        if self.disabled
            || self.action != widget.properties.action
            || self.state != widget.properties.state
        {
            widget.interaction.cancel();
        }
        widget.properties = self.clone();
        invalidation
    }
}
impl CheckBox {
    fn colors(&self) -> CheckBoxColors {
        let style = self.properties.style;
        if !self.enabled() {
            style.disabled
        } else if self.interaction.space || (self.interaction.mouse && self.interaction.hovered) {
            style.pressed
        } else if self.interaction.hovered {
            style.hovered
        } else {
            style.normal
        }
    }
}
impl Widget for CheckBox {
    fn enabled(&self) -> bool {
        !self.properties.disabled
    }
    fn focusable(&self) -> bool {
        self.enabled()
    }
    fn measure(&mut self, mut context: MeasureContext) -> Result<Size, LayoutError> {
        self.properties.style.validate(context.id)?;
        let input = context.input;
        let advance = self.properties.style.advance();
        context.input.known_width = input.known_width.map(|width| (width - advance).max(0.0));
        context.input.known_height = None;
        if let AvailableSpace::Definite(width) = input.available_width {
            context.input.available_width = AvailableSpace::Definite((width - advance).max(0.0));
        }
        let label = self.text.measure(context)?;
        Ok(Size::new(
            input
                .known_width
                .map(f64::from)
                .unwrap_or(label.width + advance as f64),
            input.known_height.map(f64::from).unwrap_or(
                label
                    .height
                    .max(self.properties.style.indicator_size as f64),
            ),
        ))
    }
    fn prepare(&mut self, mut context: PrepareContext) -> Result<(), LayoutError> {
        self.properties.style.validate(context.id)?;
        context.content_size.width =
            (context.content_size.width - self.properties.style.advance() as f64).max(0.0);
        self.text.prepare(context)
    }
    fn paint(&self, mut context: PaintContext, painter: &mut Painter<'_>) {
        let colors = self.colors();
        let bounds = context.content_bounds;
        let size = self.properties.style.indicator_size as f64;
        let x = bounds.origin.x;
        let y = bounds.origin.y + ((bounds.size.height - size) / 2.0).max(0.0);
        painter.with_clip(Path::rectangle(bounds), |painter| {
            let selected = self.properties.state != CheckState::Unchecked;
            painter.fill(
                Path::rounded_rectangle(Rect::new(x, y, size, size), size * 0.15)
                    .expect("valid checkbox bounds"),
                if selected {
                    colors.selected_background
                } else {
                    colors.background
                },
            );
            painter.stroke(
                Path::rounded_rectangle(
                    Rect::new(x + 0.5, y + 0.5, size - 1.0, size - 1.0),
                    size * 0.15,
                )
                .expect("valid checkbox border"),
                colors.border,
                1.0,
            );
            match self.properties.state {
                CheckState::Checked => {
                    let mut mark = Path::new();
                    mark.move_to(Point::new(x + size * 0.22, y + size * 0.5));
                    mark.line_to(Point::new(x + size * 0.43, y + size * 0.7));
                    mark.line_to(Point::new(x + size * 0.80, y + size * 0.28));
                    painter.stroke(mark, colors.mark, size * 0.12);
                }
                CheckState::Mixed => painter.fill_rect(
                    Rect::new(x + size * 0.22, y + size * 0.44, size * 0.56, size * 0.12),
                    colors.mark,
                ),
                CheckState::Unchecked => {}
            }
            let advance = self.properties.style.advance() as f64;
            context.content_bounds.origin.x += advance;
            context.content_bounds.size.width = (bounds.size.width - advance).max(0.0);
            context.content_bounds.origin.y +=
                ((bounds.size.height - self.text.prepared_size().height) / 2.0).max(0.0);
            painter.with_clip(
                Path::rectangle(Rect::new(
                    bounds.origin.x + advance,
                    bounds.origin.y,
                    context.content_bounds.size.width,
                    bounds.size.height,
                )),
                |painter| self.text.paint_color(context, painter, colors.text),
            );
        });
        if self.interaction.focused
            && self.enabled()
            && context.size.width > 4.0
            && context.size.height > 4.0
        {
            painter.stroke(
                Path::rectangle(Rect::new(
                    1.0,
                    1.0,
                    context.size.width - 2.0,
                    context.size.height - 2.0,
                )),
                self.properties.style.focus_ring,
                2.0,
            );
        }
    }
    fn event(&mut self, context: EventContext, event: &WidgetEvent) -> EventResponse {
        let enabled = self.enabled();
        let (mut response, activated) = self.interaction.event(context, event, enabled, false);
        if activated {
            let state = match self.properties.state {
                CheckState::Checked => CheckState::Unchecked,
                _ => CheckState::Checked,
            };
            response.actions.push(WidgetAction::new(
                self.properties.action,
                CheckChanged { state },
            ));
        }
        response
    }
}
