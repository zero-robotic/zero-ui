use crate::{
    event::{EventContext, EventResult, UiEvent},
    layout::Constraints,
    theme::Theme,
    widget::{PaintContext, Widget, WidgetId},
};
use zui_core::{Dip, Point, Rect, Size};

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
    }
    fn measure(&mut self, constraints: Constraints) -> Size {
        let inset = self.amount.0 * 2.0;
        let child_origin = Point {
            x: Dip(self.bounds.origin.x.0 + self.amount.0),
            y: Dip(self.bounds.origin.y.0 + self.amount.0),
        };
        self.child.arrange(Rect {
            origin: child_origin,
            size: Size::ZERO,
        });
        let child_size = self.child.measure(Constraints::loose(Size {
            width: Dip((constraints.max.width.0 - inset).max(0.0)),
            height: Dip((constraints.max.height.0 - inset).max(0.0)),
        }));
        let size = constraints.constrain(Size {
            width: Dip(child_size.width.0 + inset),
            height: Dip(child_size.height.0 + inset),
        });
        self.child.arrange(Rect {
            origin: child_origin,
            size: child_size,
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
    fn set_theme(&mut self, theme: &Theme) {
        self.child.set_theme(theme);
    }
}
