use crate::{
    event::{EventContext, EventResult, UiEvent},
    layout::Constraints,
    theme::Theme,
    widget::{PaintContext, Widget, WidgetId},
};
use zui_core::{Dip, Point, Rect, Size};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alignment {
    TopLeft,
    TopCenter,
    Center,
    BottomCenter,
    TopRight,
    BottomRight,
}

pub struct Align {
    id: WidgetId,
    child: Box<dyn Widget>,
    alignment: Alignment,
    bounds: Rect,
}
impl Align {
    pub fn new(child: impl Widget + 'static, alignment: Alignment) -> Self {
        Self {
            id: WidgetId::new(),
            child: Box::new(child),
            alignment,
            bounds: Rect::default(),
        }
    }
}
impl Widget for Align {
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
        let child_size = self.child.measure(Constraints::loose(constraints.max));
        let size = constraints.constrain(child_size);
        let dx = match self.alignment {
            Alignment::TopRight | Alignment::BottomRight => size.width.0 - child_size.width.0,
            Alignment::TopCenter | Alignment::Center | Alignment::BottomCenter => {
                (size.width.0 - child_size.width.0) / 2.0
            }
            _ => 0.0,
        };
        let dy = match self.alignment {
            Alignment::BottomCenter | Alignment::BottomRight => size.height.0 - child_size.height.0,
            Alignment::Center => (size.height.0 - child_size.height.0) / 2.0,
            _ => 0.0,
        };
        self.child.arrange(Rect {
            origin: Point {
                x: Dip(self.bounds.origin.x.0 + dx),
                y: Dip(self.bounds.origin.y.0 + dy),
            },
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
