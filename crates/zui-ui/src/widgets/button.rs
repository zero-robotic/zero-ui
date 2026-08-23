use crate::{
    event::{is_left_press, ActionKind, EventContext, EventResult, UiEvent},
    layout::Constraints,
    widget::{Widget, WidgetId},
};
use zui_core::{Color, Dip, Point, Rect, Size};

pub struct Button {
    id: WidgetId,
    label: String,
    bounds: Rect,
}

impl Button {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            id: WidgetId::new(),
            label: label.into(),
            bounds: Rect::default(),
        }
    }
    pub fn label(&self) -> &str {
        &self.label
    }
}

impl Widget for Button {
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
            width: Dip(self.label.chars().count() as f32 * 8.0 + 24.0),
            height: Dip(32.0),
        });
        self.bounds.size = size;
        size
    }
    fn event(&mut self, event: &UiEvent, ctx: &mut EventContext) -> EventResult {
        let clicked = is_left_press(event)
            && event
                .position
                .is_some_and(|point| self.bounds.contains(point));
        if clicked {
            ctx.emit(self.id, ActionKind::Clicked);
            EventResult::RequestRedraw
        } else {
            EventResult::Ignored
        }
    }
    fn paint(&self, ctx: &mut crate::PaintContext<'_>) {
        ctx.fill_rect(
            self.bounds,
            Color {
                r: 0.18,
                g: 0.42,
                b: 0.86,
                a: 1.0,
            },
        );
        ctx.draw_text(
            &self.label,
            Point {
                x: Dip(self.bounds.origin.x.0 + 12.0),
                y: Dip(self.bounds.origin.y.0 + 9.0),
            },
            Color::WHITE,
            2,
        );
    }
}
