use crate::{
    event::{EventContext, EventResult, UiEvent},
    layout::Constraints,
    theme::Theme,
    widget::{PaintContext, Widget, WidgetId},
};
use zui_core::{Dip, Point, Rect, Size};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Alignment {
    pub x: f32,
    pub y: f32,
}
impl Alignment {
    pub const TOP_LEFT: Self = Self { x: 0.0, y: 0.0 };
    pub const TOP_CENTER: Self = Self { x: 0.5, y: 0.0 };
    pub const TOP_RIGHT: Self = Self { x: 1.0, y: 0.0 };
    pub const CENTER_LEFT: Self = Self { x: 0.0, y: 0.5 };
    pub const CENTER: Self = Self { x: 0.5, y: 0.5 };
    pub const CENTER_RIGHT: Self = Self { x: 1.0, y: 0.5 };
    pub const BOTTOM_LEFT: Self = Self { x: 0.0, y: 1.0 };
    pub const BOTTOM_CENTER: Self = Self { x: 0.5, y: 1.0 };
    pub const BOTTOM_RIGHT: Self = Self { x: 1.0, y: 1.0 };

    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
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
    pub fn child(&self) -> &dyn Widget {
        &*self.child
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
        let child_size = self.child.bounds().size;
        let dx = (bounds.size.width.0 - child_size.width.0) * self.alignment.x;
        let dy = (bounds.size.height.0 - child_size.height.0) * self.alignment.y;
        self.child.arrange(Rect {
            origin: Point {
                x: Dip(bounds.origin.x.0 + dx),
                y: Dip(bounds.origin.y.0 + dy),
            },
            size: child_size,
        });
    }
    fn next_redraw(&self) -> Option<std::time::Instant> {
        self.child.next_redraw()
    }
    fn measure(&mut self, constraints: Constraints) -> Size {
        let child_size = self.child.measure(Constraints::loose(constraints.max));
        let size = constraints.constrain(child_size);
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
