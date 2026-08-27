use crate::{
    event::{EventContext, EventResult, UiEvent},
    layout::Constraints,
    theme::Theme,
    widget::{build_render_node_with_commands, PaintContext, Widget, WidgetId},
};
use zui_core::{Color, Dip, Rect, Size};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DividerAxis {
    Horizontal,
    Vertical,
}
pub struct Divider {
    id: WidgetId,
    axis: DividerAxis,
    thickness: Dip,
    color: Color,
    bounds: Rect,
}

impl Divider {
    fn build_render_commands(&self, ctx: &mut PaintContext<'_>) {
        ctx.fill_rect(self.bounds, self.color);
    }
}
impl Divider {
    pub fn horizontal() -> Self {
        Self::new(DividerAxis::Horizontal)
    }
    pub fn vertical() -> Self {
        Self::new(DividerAxis::Vertical)
    }
    fn new(axis: DividerAxis) -> Self {
        Self {
            id: WidgetId::new(),
            axis,
            thickness: Dip(1.0),
            color: Color {
                r: 0.45,
                g: 0.48,
                b: 0.55,
                a: 1.0,
            },
            bounds: Rect::default(),
        }
    }
    pub fn thickness(mut self, value: f32) -> Self {
        self.thickness = Dip(value);
        self
    }
    pub fn color(mut self, value: Color) -> Self {
        self.color = value;
        self
    }
}
impl Widget for Divider {
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
        let desired = match self.axis {
            DividerAxis::Horizontal => Size {
                width: constraints.max.width,
                height: self.thickness,
            },
            DividerAxis::Vertical => Size {
                width: self.thickness,
                height: constraints.max.height,
            },
        };
        let size = constraints.constrain(desired);
        self.bounds.size = size;
        size
    }
    fn event(&mut self, _event: &UiEvent, _ctx: &mut EventContext) -> EventResult {
        EventResult::Ignored
    }
    fn build_render_node(&self, theme: &Theme) -> zui_render::RenderNode {
        build_render_node_with_commands(self.id, self.bounds, theme, |ctx| {
            self.build_render_commands(ctx)
        })
    }
    fn build_render_node_incremental(&self, context: &mut crate::RenderBuildContext<'_>) -> zui_render::RenderNode {
        crate::widget::build_leaf_render_node_incremental(self.id, context, |theme| self.build_render_node(theme))
    }
    fn set_theme(&mut self, _theme: &Theme) {}
}
