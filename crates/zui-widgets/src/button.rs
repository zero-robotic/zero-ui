use crate::interaction::Interaction;
use crate::{Text, TextView};
use alloc::{string::String, vec::Vec};
use zui_api::{
    action::{ActionId, WidgetAction},
    event::*,
    layout::LayoutError,
    text::{TextAlign, TextStyle},
    view::ViewSpec,
    widget::{Invalidation, MeasureContext, PaintContext, PrepareContext, Widget},
};
use zui_paint::{Painter, Path};
use zui_theme::{ColorRole, Theme, TypeRole};
use zui_types::{Color, Rect, Size};

/// Public action payload for a successful mouse or keyboard activation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ButtonActivated;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ButtonColors {
    pub background: Color,
    pub foreground: Color,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ButtonStyle {
    pub normal: ButtonColors,
    pub hovered: ButtonColors,
    pub pressed: ButtonColors,
    pub disabled: ButtonColors,
    pub focus_ring: Color,
    pub border: Color,
    pub metrics: zui_theme::ControlMetrics,
}
impl ButtonStyle {
    pub fn from_theme(theme: &Theme) -> Self {
        Self::primary(theme)
    }
    pub fn primary(theme: &Theme) -> Self {
        let accent = theme.color(ColorRole::Accent);
        let foreground = theme.color(ColorRole::OnAccent);
        Self {
            normal: ButtonColors {
                background: accent,
                foreground,
            },
            hovered: ButtonColors {
                background: mix(accent, Color::WHITE, 32),
                foreground,
            },
            pressed: ButtonColors {
                background: mix(accent, Color::BLACK, 48),
                foreground,
            },
            disabled: ButtonColors {
                background: theme.color(ColorRole::SurfaceRaised),
                foreground: theme.color(ColorRole::TextMuted),
            },
            focus_ring: theme.color(if theme.controls().min_height > 0 {
                ColorRole::Accent
            } else {
                ColorRole::Text
            }),
            border: Color::TRANSPARENT,
            metrics: theme.controls(),
        }
    }
    pub fn secondary(theme: &Theme) -> Self {
        let mut style = Self::primary(theme);
        style.normal = ButtonColors {
            background: theme.color(ColorRole::SurfaceRaised),
            foreground: theme.color(ColorRole::Text),
        };
        style.hovered = ButtonColors {
            background: mix(style.normal.background, theme.color(ColorRole::Text), 16),
            ..style.normal
        };
        style.pressed = ButtonColors {
            background: theme.selection_background(),
            foreground: theme.selection_foreground(),
        };
        style.border = theme.color(ColorRole::Border);
        style
    }
    pub fn text(theme: &Theme) -> Self {
        let mut style = Self::secondary(theme);
        style.normal.background = Color::TRANSPARENT;
        style.border = Color::TRANSPARENT;
        style
    }
}
fn mix(a: Color, b: Color, amount: u16) -> Color {
    let channel =
        |a: u8, b: u8| ((u16::from(a) * (255 - amount) + u16::from(b) * amount) / 255) as u8;
    Color::rgba(channel(a.r, b.r), channel(a.g, b.g), channel(a.b, b.b), a.a)
}
#[derive(Clone, Debug, PartialEq)]
pub struct ButtonView {
    pub label: String,
    pub trailing_icon: Option<crate::Icon>,
    pub reserve_trailing_icon: bool,
    pub text_style: TextStyle,
    pub style: ButtonStyle,
    pub action: ActionId,
    pub disabled: bool,
}
impl ButtonView {
    pub fn themed(
        theme: &Theme,
        label: impl Into<String>,
        families: Vec<String>,
        action: ActionId,
    ) -> Self {
        let metrics = theme.typography(TypeRole::Body);
        Self {
            label: label.into(),
            trailing_icon: None,
            reserve_trailing_icon: false,
            text_style: TextStyle {
                families,
                font_size: metrics.font_size(),
                line_height: metrics.line_height(),
                align: TextAlign::Center,
            },
            style: if theme.controls().min_height > 0 {
                ButtonStyle::secondary(theme)
            } else {
                ButtonStyle::from_theme(theme)
            },
            action,
            disabled: false,
        }
    }
    fn text(&self) -> TextView {
        TextView::new(
            self.label.clone(),
            self.text_style.clone(),
            self.style.normal.foreground,
        )
    }
}
pub struct Button {
    properties: ButtonView,
    text: Text,
    state: Interaction,
}
impl ViewSpec for ButtonView {
    type Widget = Button;
    fn build(&self) -> Button {
        Button {
            properties: self.clone(),
            text: self.text().build(),
            state: Default::default(),
        }
    }
    fn update(&self, button: &mut Button) -> Invalidation {
        let mut invalidation = self.text().update(&mut button.text);
        if (self.trailing_icon.is_some() || self.reserve_trailing_icon)
            != (button.properties.trailing_icon.is_some()
                || button.properties.reserve_trailing_icon)
            || self.style.metrics != button.properties.style.metrics
        {
            invalidation = Invalidation::Layout;
        }
        if self != &button.properties {
            invalidation = invalidation.max(Invalidation::Paint);
        }
        // Changing action identity or disabling during a press must never
        // activate a new binding using an old gesture.
        if self.disabled || self.action != button.properties.action {
            button.cancel_press();
        }
        button.properties = self.clone();
        invalidation
    }
}
impl Button {
    pub(crate) fn cancel_press(&mut self) {
        self.state.cancel();
    }
    fn icon_space(&self) -> f64 {
        if self.properties.trailing_icon.is_some() || self.properties.reserve_trailing_icon {
            self.properties.text_style.font_size as f64 * 1.4
        } else {
            0.0
        }
    }
    fn colors(&self) -> ButtonColors {
        let style = self.properties.style;
        if self.properties.disabled {
            style.disabled
        } else if self.state.space || (self.state.mouse && self.state.hovered) {
            style.pressed
        } else if self.state.hovered {
            style.hovered
        } else {
            style.normal
        }
    }
    fn activate(&self, response: &mut EventResponse) {
        response
            .actions
            .push(WidgetAction::new(self.properties.action, ButtonActivated));
    }
}
impl Widget for Button {
    fn enabled(&self) -> bool {
        !self.properties.disabled
    }
    fn focusable(&self) -> bool {
        self.enabled()
    }
    fn measure(&mut self, context: MeasureContext) -> Result<Size, LayoutError> {
        let known_width = context.input.known_width;
        let known_height = context.input.known_height;
        let extra = self.icon_space() + f64::from(self.properties.style.metrics.padding_x) * 2.0;
        let mut context = context;
        context.input.known_width = context
            .input
            .known_width
            .map(|v| (v - extra as f32).max(0.0));
        if let zui_api::layout::AvailableSpace::Definite(v) = context.input.available_width {
            context.input.available_width =
                zui_api::layout::AvailableSpace::Definite((v - extra as f32).max(0.0));
        }
        let mut size = self.text.measure(context)?;
        size.width += extra;
        if self.properties.trailing_icon.is_some() || self.properties.reserve_trailing_icon {
            size.height = size.height.max(self.properties.text_style.font_size as f64);
        }
        size.height = size
            .height
            .max(f64::from(self.properties.style.metrics.min_height));
        if let Some(width) = known_width {
            size.width = f64::from(width);
        }
        if let Some(height) = known_height {
            size.height = f64::from(height);
        }
        Ok(size)
    }
    fn prepare(&mut self, context: PrepareContext) -> Result<(), LayoutError> {
        let mut context = context;
        context.content_size.width = (context.content_size.width
            - self.icon_space()
            - f64::from(self.properties.style.metrics.padding_x) * 2.0)
            .max(0.0);
        self.text.prepare(context)
    }
    fn paint(&self, mut context: PaintContext, painter: &mut Painter<'_>) {
        let colors = self.colors();
        let metrics = self.properties.style.metrics;
        painter.fill(
            Path::rounded_rectangle(
                Rect::new(0.0, 0.0, context.size.width, context.size.height),
                f64::from(metrics.radius),
            )
            .expect("valid button bounds"),
            colors.background,
        );
        if self.properties.style.border.a > 0
            && context.size.width > 1.0
            && context.size.height > 1.0
        {
            painter.stroke(
                Path::rounded_rectangle(
                    Rect::new(
                        0.5,
                        0.5,
                        context.size.width - 1.0,
                        context.size.height - 1.0,
                    ),
                    f64::from(metrics.radius),
                )
                .expect("valid border"),
                self.properties.style.border,
                1.0,
            );
        }
        let padding = f64::from(metrics.padding_x).min(context.content_bounds.size.width / 2.0);
        context.content_bounds.origin.x += padding;
        context.content_bounds.size.width -= 2.0 * padding;
        if self.state.focused
            && self.state.focus_visible
            && self.enabled()
            && context.size.width > 4.0
            && context.size.height > 4.0
        {
            painter.stroke(
                Path::rounded_rectangle(
                    Rect::new(
                        1.0,
                        1.0,
                        context.size.width - 2.0,
                        context.size.height - 2.0,
                    ),
                    f64::from(metrics.radius.saturating_sub(1)),
                )
                .expect("valid focus ring"),
                self.properties.style.focus_ring,
                2.0,
            );
        }
        if let Some(icon) = &self.properties.trailing_icon {
            let bounds = context.content_bounds;
            let side = (self.properties.text_style.font_size as f64)
                .min(bounds.size.width)
                .min(bounds.size.height);
            icon.paint(
                Rect::new(
                    bounds.origin.x + bounds.size.width - side,
                    bounds.origin.y + (bounds.size.height - side) / 2.0,
                    side,
                    side,
                ),
                colors.foreground,
                painter,
            );
        }
        context.content_bounds.size.width =
            (context.content_bounds.size.width - self.icon_space()).max(0.0);
        let clip = context.content_bounds;
        context.content_bounds.origin.y +=
            ((context.content_bounds.size.height - self.text.prepared_size().height) / 2.0)
                .max(0.0);
        painter.with_clip(Path::rectangle(clip), |painter| {
            self.text.paint_color(context, painter, colors.foreground)
        });
    }
    fn event(&mut self, context: EventContext, event: &WidgetEvent) -> EventResponse {
        let enabled = self.enabled();
        let (mut response, activated) = self.state.event(context, event, enabled, true);
        if activated {
            self.activate(&mut response);
        }
        response
    }
}
