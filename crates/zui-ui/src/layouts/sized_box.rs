use std::collections::HashSet;

use crate::{
    event::{EventContext, EventResult, UiEvent},
    layout::Constraints,
    theme::Theme,
    widget::{PaintContext, Widget, WidgetId},
};
use zui_core::{Dip, Rect, Size};
use zui_render::RenderNode;

pub struct SizedBox {
    id: WidgetId,
    child: Box<dyn Widget>,
    width: Option<Dip>,
    height: Option<Dip>,
    bounds: Rect,
}
impl SizedBox {
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            id: WidgetId::new(),
            child: Box::new(child),
            width: None,
            height: None,
            bounds: Rect::default(),
        }
    }
    pub fn width(mut self, value: f32) -> Self {
        self.width = Some(Dip(value));
        self
    }
    pub fn height(mut self, value: f32) -> Self {
        self.height = Some(Dip(value));
        self
    }
}
impl Widget for SizedBox {
    fn id(&self) -> WidgetId {
        self.id
    }
    fn bounds(&self) -> Rect {
        self.bounds
    }
    fn arrange(&mut self, bounds: Rect) {
        self.bounds = bounds;
        self.child.arrange(Rect {
            origin: bounds.origin,
            size: bounds.size,
        });
    }
    fn next_redraw(&self) -> Option<std::time::Instant> {
        self.child.next_redraw()
    }
    fn measure(&mut self, constraints: Constraints) -> Size {
        let fixed_width = self.width.map(|width| {
            Dip(width
                .0
                .clamp(constraints.min.width.0, constraints.max.width.0))
        });
        let fixed_height = self.height.map(|height| {
            Dip(height
                .0
                .clamp(constraints.min.height.0, constraints.max.height.0))
        });
        let child_constraints = Constraints {
            min: Size {
                width: fixed_width.unwrap_or(constraints.min.width),
                height: fixed_height.unwrap_or(constraints.min.height),
            },
            max: Size {
                width: fixed_width.unwrap_or(constraints.max.width),
                height: fixed_height.unwrap_or(constraints.max.height),
            },
        };
        let child_size = self.child.measure(child_constraints);
        let size = constraints.constrain(Size {
            width: fixed_width.unwrap_or(child_size.width),
            height: fixed_height.unwrap_or(child_size.height),
        });
        self.bounds.size = size;
        size
    }
    fn event(&mut self, event: &UiEvent, ctx: &mut EventContext) -> EventResult {
        self.child.event(event, ctx)
    }
    fn paint(&self, ctx: &mut PaintContext<'_>) {
        self.child.paint(ctx);
    }
    fn build_render_node_with_cache(
        &self,
        previous: Option<&RenderNode>,
        dirty_region: Option<Rect>,
        theme: &Theme,
    ) -> RenderNode {
        let mut node = RenderNode::for_widget(self.bounds);
        node.set_source_id(self.id.0);
        let cached_child = previous.and_then(|node| node.children.first());
        node.add_child(
            self.child
                .build_render_node_with_cache(cached_child, dirty_region, theme),
        );
        node
    }
    fn build_render_node_with_dirty_widgets(
        &self,
        previous: Option<&RenderNode>,
        dirty_region: Option<Rect>,
        dirty_widgets: &HashSet<WidgetId>,
        theme: &Theme,
    ) -> RenderNode {
        let mut node = RenderNode::for_widget(self.bounds);
        node.set_source_id(self.id.0);
        let cached_child = previous.and_then(|node| node.children.first());
        node.add_child(self.child.build_render_node_with_dirty_widgets(
            cached_child,
            dirty_region,
            dirty_widgets,
            theme,
        ));
        node
    }
    fn set_theme(&mut self, theme: &Theme) {
        self.child.set_theme(theme);
    }
}
