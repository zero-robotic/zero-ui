use crate::{
    event::{EventContext, EventResult, UiEvent},
    layout::Constraints,
    widget::{PaintContext, Widget, WidgetId},
};
use zui_core::{Dip, Point, Rect, Size};

pub struct Column {
    id: WidgetId,
    children: Vec<Box<dyn Widget>>,
    bounds: Rect,
    spacing: Dip,
}

impl Column {
    pub fn new(children: Vec<Box<dyn Widget>>) -> Self {
        Self {
            id: WidgetId::new(),
            children,
            bounds: Rect::default(),
            spacing: Dip(0.0),
        }
    }
    pub fn with_spacing(mut self, spacing: Dip) -> Self {
        self.spacing = spacing;
        self
    }
}

impl Widget for Column {
    fn id(&self) -> WidgetId {
        self.id
    }
    fn bounds(&self) -> Rect {
        self.bounds
    }
    fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }
    fn layout(&mut self, constraints: Constraints) -> Size {
        let mut width: f32 = 0.0;
        let mut height = 0.0;
        for (index, child) in self.children.iter_mut().enumerate() {
            let size = child.layout(Constraints::loose(constraints.max));
            let gap = if index == 0 { 0.0 } else { self.spacing.0 };
            child.set_bounds(Rect {
                origin: Point {
                    x: self.bounds.origin.x,
                    y: Dip(self.bounds.origin.y.0 + height + gap),
                },
                size,
            });
            width = width.max(size.width.0);
            height += size.height.0 + gap;
        }
        let size = constraints.constrain(Size {
            width: Dip(width),
            height: Dip(height),
        });
        self.bounds.size = size;
        size
    }
    fn event(&mut self, event: &UiEvent, ctx: &mut EventContext) -> EventResult {
        for child in self.children.iter_mut().rev() {
            if child.event(event, ctx) != EventResult::Ignored {
                return EventResult::Handled;
            }
        }
        EventResult::Ignored
    }
    fn paint(&self, ctx: &mut PaintContext<'_>) {
        for child in &self.children {
            child.paint(ctx);
        }
    }
}
