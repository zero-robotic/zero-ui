use crate::{
    event::{is_left_press, ActionKind, EventContext, EventResult, UiEvent},
    layout::Constraints,
    theme::Theme,
    widget::{build_render_node_with_commands, Widget, WidgetId},
};
use zui_core::{Dip, Point, Rect, Size};

pub struct Switch {
    id: WidgetId,
    label: String,
    checked: bool,
    bounds: Rect,
    theme: Theme,
}

impl Switch {
    fn build_render_commands(&self, ctx: &mut crate::PaintContext<'_>) {
        let style = &ctx.theme.switch;
        let text_metrics = zui_render::text_metrics(style.font_size);
        let track = Rect {
            origin: Point {
                x: self.bounds.origin.x,
                y: Dip(self.bounds.origin.y.0 + (self.bounds.size.height.0 - style.height.0) / 2.0),
            },
            size: Size {
                width: style.width,
                height: style.height,
            },
        };
        ctx.fill_rounded_rect(
            track,
            Dip(style.height.0 / 2.0),
            if self.checked {
                style.checked_background
            } else {
                style.background
            },
        );
        let knob_margin = (style.height.0 - style.knob_size.0) / 2.0;
        let knob_x = if self.checked {
            style.width.0 - style.knob_size.0 - knob_margin
        } else {
            knob_margin
        };
        ctx.fill_rounded_rect(
            Rect {
                origin: Point {
                    x: Dip(track.origin.x.0 + knob_x),
                    y: Dip(track.origin.y.0 + (style.height.0 - style.knob_size.0) / 2.0),
                },
                size: Size {
                    width: style.knob_size,
                    height: style.knob_size,
                },
            },
            Dip(style.knob_size.0 / 2.0),
            style.knob,
        );
        ctx.draw_text(
            &self.label,
            Point {
                x: Dip(track.origin.x.0 + style.width.0 + style.gap.0),
                y: Dip(self.bounds.origin.y.0
                    + (self.bounds.size.height.0 - text_metrics.line_height.0) / 2.0),
            },
            ctx.theme.text.color,
            style.font_size,
        );
    }
}

impl Switch {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            id: WidgetId::new(),
            label: label.into(),
            checked: false,
            bounds: Rect::default(),
            theme: Theme::default(),
        }
    }

    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    pub fn is_checked(&self) -> bool {
        self.checked
    }

    pub fn set_checked(&mut self, checked: bool) {
        self.checked = checked;
    }

    pub fn label(&self) -> &str {
        &self.label
    }
}

impl Widget for Switch {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn arrange(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    fn measure(&mut self, constraints: Constraints) -> Size {
        let style = &self.theme.switch;
        let label_width = zui_render::measure_text(&self.label, style.font_size).0;
        let size = constraints.constrain(Size {
            width: Dip(style.width.0 + style.gap.0 + label_width),
            height: Dip(style
                .height
                .0
                .max(zui_render::text_metrics(style.font_size).line_height.0)),
        });
        self.bounds.size = size;
        size
    }

    fn event(&mut self, event: &UiEvent, ctx: &mut EventContext) -> EventResult {
        if is_left_press(event)
            && event
                .position
                .is_some_and(|point| self.bounds.contains(point))
        {
            self.checked = !self.checked;
            ctx.emit(self.id, ActionKind::CheckedChanged);
            ctx.request_full_redraw();
            EventResult::RequestRedraw
        } else {
            EventResult::Ignored
        }
    }

    fn set_theme(&mut self, theme: &Theme) {
        self.theme = theme.clone();
    }

    fn build_render_node(&self, theme: &Theme) -> zui_render::RenderNode {
        build_render_node_with_commands(self.id, self.bounds, theme, |ctx| {
            self.build_render_commands(ctx)
        })
    }
}
