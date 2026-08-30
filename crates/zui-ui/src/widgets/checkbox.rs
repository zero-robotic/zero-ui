use crate::{
    event::{is_left_press, ActionKind, EventContext, EventResult, UiEvent},
    layout::Constraints,
    theme::Theme,
    widget::{build_render_node_with_commands, RenderBuildContext, Widget, WidgetId},
    Text,
};
use zui_core::{Dip, Point, Rect, Size};
use zui_render::{RenderNode, Transform};

pub struct Checkbox {
    id: WidgetId,
    label: Text,
    checked: bool,
    bounds: Rect,
    theme: Theme,
}

impl Checkbox {
    pub fn new(label: impl Into<String>) -> Self {
        let theme = Theme::default();
        let mut label = Text::new(label);
        label.set_font_size(theme.checkbox.font_size);
        label.set_color(theme.text.color);
        Self {
            id: WidgetId::new(),
            label,
            checked: false,
            bounds: Rect::default(),
            theme,
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
        self.label.text()
    }

    fn build_render_commands(&self, ctx: &mut crate::PaintContext<'_>) {
        let style = &ctx.theme.checkbox;
        let box_rect = Rect {
            origin: self.bounds.origin,
            size: Size {
                width: style.size,
                height: style.size,
            },
        };
        ctx.fill_rounded_rect(
            box_rect,
            style.radius,
            if self.checked {
                style.checked_background
            } else {
                style.background
            },
        );
        if self.checked {
            let x = box_rect.origin.x.0;
            let y = box_rect.origin.y.0;
            let s = style.size.0;
            let left = Point {
                x: Dip(x + s * 0.23),
                y: Dip(y + s * 0.52),
            };
            let middle = Point {
                x: Dip(x + s * 0.43),
                y: Dip(y + s * 0.72),
            };
            let right = Point {
                x: Dip(x + s * 0.80),
                y: Dip(y + s * 0.28),
            };
            let stroke = Dip((s * 0.10).max(1.5));
            let cap_radius = Dip(stroke.0 / 2.0);
            for point in [left, middle, right] {
                ctx.fill_rounded_rect(
                    Rect {
                        origin: Point {
                            x: Dip(point.x.0 - cap_radius.0),
                            y: Dip(point.y.0 - cap_radius.0),
                        },
                        size: Size {
                            width: stroke,
                            height: stroke,
                        },
                    },
                    cap_radius,
                    style.foreground,
                );
            }
            ctx.draw_line(left, middle, stroke, style.foreground);
            ctx.draw_line(middle, right, stroke, style.foreground);
        }
    }
}

impl Widget for Checkbox {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn arrange(&mut self, bounds: Rect) {
        self.bounds = bounds;
        let size = self.label.bounds().size;
        let style = &self.theme.checkbox;
        self.label.arrange(Rect {
            origin: Point {
                x: Dip(bounds.origin.x.0 + style.size.0 + style.gap.0),
                y: Dip(bounds.origin.y.0 + (bounds.size.height.0 - size.height.0) / 2.0),
            },
            size,
        });
    }

    fn measure(&mut self, constraints: Constraints) -> Size {
        let style = &self.theme.checkbox;
        let label = self.label.measure(Constraints::loose(constraints.max));
        let size = constraints.constrain(Size {
            width: Dip(style.size.0 + style.gap.0 + label.width.0),
            height: Dip(style
                .size
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
            self.invalidate(ctx, self.bounds);
            EventResult::RequestRedraw
        } else {
            EventResult::Ignored
        }
    }

    fn set_theme(&mut self, theme: &Theme) {
        self.theme = theme.clone();
        self.label.set_font_size(theme.checkbox.font_size);
        self.label.set_color(theme.text.color);
        self.label.set_theme(theme);
    }

    fn build_render_node(&self, theme: &Theme) -> RenderNode {
        let mut node = build_render_node_with_commands(self.id, self.bounds, theme, |ctx| {
            self.build_render_commands(ctx)
        });
        node.add_child(self.label.build_render_node(theme));
        node
    }
    fn build_render_node_incremental(&self, context: &mut RenderBuildContext<'_>) -> RenderNode {
        if !context.subtree_is_dirty(self.id) {
            if let Some(previous) = context.previous() {
                return previous.clone();
            }
        }
        let mut node = context.localize(build_render_node_with_commands(
            self.id,
            self.bounds,
            context.theme(),
            |ctx| self.build_render_commands(ctx),
        ));
        let mut label_context = context.child(
            0,
            Transform::translate(self.bounds.origin.x, self.bounds.origin.y),
        );
        node.add_child(self.label.build_render_node_incremental(&mut label_context));
        node
    }
}
