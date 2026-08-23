use crate::{
    event::{EventContext, EventResult, UiEvent},
    layout::Constraints,
    widget::{Widget, WidgetId},
};
use zui_core::{Color, Dip, Point, Rect, Size};

pub struct Text {
    id: WidgetId,
    text: String,
    bounds: Rect,
}

impl Text {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            id: WidgetId::new(),
            text: text.into(),
            bounds: Rect::default(),
        }
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
    }
}

impl Widget for Text {
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
    fn paint(&self, ctx: &mut crate::PaintContext<'_>) {
        ctx.draw_text(
            &self.text,
            Point {
                x: Dip(self.bounds.origin.x.0),
                y: Dip(self.bounds.origin.y.0 + 4.0),
            },
            Color::WHITE,
            2,
        );
    }
}
