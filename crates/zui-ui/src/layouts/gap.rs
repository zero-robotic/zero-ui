use crate::{
    event::{EventContext, EventResult, UiEvent},
    layout::Constraints,
    theme::Theme,
    widget::{build_render_node_with_commands, Widget, WidgetId},
};
use zui_core::{Dip, Rect, Size};

pub struct Gap {
    id: WidgetId,
    size: Size,
    bounds: Rect,
}

impl Gap {
    pub fn horizontal(value: f32) -> Self {
        Self::new(Size {
            width: Dip(value),
            height: Dip::ZERO,
        })
    }

    pub fn vertical(value: f32) -> Self {
        Self::new(Size {
            width: Dip::ZERO,
            height: Dip(value),
        })
    }

    fn new(size: Size) -> Self {
        Self {
            id: WidgetId::new(),
            size,
            bounds: Rect::default(),
        }
    }
}

impl Widget for Gap {
    fn id(&self) -> WidgetId {
        self.id
    }
    fn bounds(&self) -> Rect {
        self.bounds
    }
    fn measure(&mut self, constraints: Constraints) -> Size {
        let size = constraints.constrain(self.size);
        self.bounds.size = size;
        size
    }
    fn arrange(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }
    fn event(&mut self, _event: &UiEvent, _ctx: &mut EventContext) -> EventResult {
        EventResult::Ignored
    }
    fn build_render_node(&self, theme: &Theme) -> zui_render::RenderNode {
        build_render_node_with_commands(self.id, self.bounds, theme, |_ctx| {})
    }
    fn build_render_node_incremental(
        &self,
        context: &mut crate::RenderBuildContext<'_>,
    ) -> zui_render::RenderNode {
        crate::widget::build_leaf_render_node_incremental(self.id, context, |theme| {
            self.build_render_node(theme)
        })
    }
}
