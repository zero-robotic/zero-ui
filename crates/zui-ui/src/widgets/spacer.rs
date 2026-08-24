use crate::{
    event::{EventContext, EventResult, UiEvent},
    layout::Constraints,
    widget::{PaintContext, Widget, WidgetId},
};
use zui_core::{Dip, Rect, Size};

pub struct Spacer {
    id: WidgetId,
    extent: Dip,
    bounds: Rect,
}
impl Spacer {
    pub fn new(extent: f32) -> Self {
        Self {
            id: WidgetId::new(),
            extent: Dip(extent),
            bounds: Rect::default(),
        }
    }
}
impl Widget for Spacer {
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
            width: self.extent,
            height: self.extent,
        });
        self.bounds.size = size;
        size
    }
    fn event(&mut self, _event: &UiEvent, _ctx: &mut EventContext) -> EventResult {
        EventResult::Ignored
    }
    fn paint(&self, _ctx: &mut PaintContext<'_>) {}
}
