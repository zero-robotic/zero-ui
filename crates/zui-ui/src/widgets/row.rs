use crate::{
    event::{EventContext, EventResult, UiEvent},
    layout::Constraints,
    widget::{PaintContext, Widget, WidgetId},
};
use zui_core::{Dip, Point, Rect, Size};

pub struct Row {
    id: WidgetId,
    children: Vec<Box<dyn Widget>>,
    bounds: Rect,
    spacing: Dip,
}

impl Row {
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
    pub fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }
}

impl Widget for Row {
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
        let mut width = 0.0;
        let mut height: f32 = 0.0;
        for (index, child) in self.children.iter_mut().enumerate() {
            let size = child.layout(Constraints::loose(constraints.max));
            let gap = if index == 0 { 0.0 } else { self.spacing.0 };
            child.set_bounds(Rect {
                origin: Point {
                    x: Dip(self.bounds.origin.x.0 + width + gap),
                    y: self.bounds.origin.y,
                },
                size,
            });
            width += size.width.0 + gap;
            height = height.max(size.height.0);
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
