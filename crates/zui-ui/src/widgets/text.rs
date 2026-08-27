use crate::{
    event::{EventContext, EventResult, UiEvent},
    layout::Constraints,
    theme::Theme,
    widget::{build_render_node_with_commands, Widget, WidgetId},
};
use zui_core::{Dip, Point, Rect, Size};

pub struct Text {
    id: WidgetId,
    text: String,
    bounds: Rect,
    theme: Theme,
}

impl Text {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            id: WidgetId::new(),
            text: text.into(),
            bounds: Rect::default(),
            theme: Theme::default(),
        }
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
    }

    fn build_render_commands(&self, ctx: &mut crate::PaintContext<'_>) {
        ctx.draw_text(
            &self.text,
            Point {
                x: Dip(self.bounds.origin.x.0),
                y: Dip(self.bounds.origin.y.0 + 4.0),
            },
            ctx.theme.text.color,
            ctx.theme.text.font_size,
        );
    }
}

impl Widget for Text {
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
        let size = constraints.constrain(Size {
            width: Dip(self.text.chars().count() as f32 * 8.0),
            height: Dip(20.0),
        });
        self.bounds.size = size;
        size
    }
    fn event(&mut self, _event: &UiEvent, _ctx: &mut EventContext) -> EventResult {
        EventResult::Ignored
    }
    fn set_theme(&mut self, theme: &Theme) {
        self.theme = theme.clone();
    }
    fn build_render_node(&self, theme: &Theme) -> zui_render::RenderNode {
        build_render_node_with_commands(self.id, self.bounds, theme, |ctx| {
            self.build_render_commands(ctx)
        })
    }
    fn build_render_node_incremental(&self, context: &mut crate::RenderBuildContext<'_>) -> zui_render::RenderNode {
        crate::widget::build_leaf_render_node_incremental(self.id, context, |theme| self.build_render_node(theme))
    }
}
