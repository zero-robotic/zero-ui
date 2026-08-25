use crate::{
    event::{EventContext, EventResult, UiEvent},
    layout::Constraints,
    theme::Theme,
    widget::{Widget, WidgetId},
};
use zui_core::{Dip, Point, Rect, Size};
use zui_render::RenderNode;

pub struct Padding {
    id: WidgetId,
    child: Box<dyn Widget>,
    amount: Dip,
    bounds: Rect,
}
impl Padding {
    pub fn new(child: impl Widget + 'static, amount: Dip) -> Self {
        Self {
            id: WidgetId::new(),
            child: Box::new(child),
            amount,
            bounds: Rect::default(),
        }
    }
    pub fn child(&self) -> &dyn Widget {
        &*self.child
    }
}
impl Widget for Padding {
    fn id(&self) -> WidgetId {
        self.id
    }
    fn bounds(&self) -> Rect {
        self.bounds
    }
    fn arrange(&mut self, bounds: Rect) {
        self.bounds = bounds;
        let child_size = self.child.bounds().size;
        self.child.arrange(Rect {
            origin: Point {
                x: Dip(bounds.origin.x.0 + self.amount.0),
                y: Dip(bounds.origin.y.0 + self.amount.0),
            },
            size: child_size,
        });
    }
    fn next_redraw(&self) -> Option<std::time::Instant> {
        self.child.next_redraw()
    }
    fn measure(&mut self, constraints: Constraints) -> Size {
        let inset = self.amount.0 * 2.0;
        let child_size = self.child.measure(Constraints::loose(Size {
            width: Dip((constraints.max.width.0 - inset).max(0.0)),
            height: Dip((constraints.max.height.0 - inset).max(0.0)),
        }));
        let size = constraints.constrain(Size {
            width: Dip(child_size.width.0 + inset),
            height: Dip(child_size.height.0 + inset),
        });
        self.bounds.size = size;
        size
    }
    fn event(&mut self, event: &UiEvent, ctx: &mut EventContext) -> EventResult {
        self.child.event(event, ctx)
    }
    fn build_render_node(&self, theme: &Theme) -> RenderNode {
        let mut node = RenderNode::for_widget(self.bounds);
        node.set_source_id(self.id.0);
        node.add_child(self.child.build_render_node(theme));
        node
    }
    fn set_theme(&mut self, theme: &Theme) {
        self.child.set_theme(theme);
    }
}
