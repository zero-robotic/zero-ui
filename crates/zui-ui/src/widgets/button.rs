use crate::{
    event::{is_left_press, ActionKind, EventContext, EventResult, UiEvent},
    layout::Constraints,
    widget::{Widget, WidgetId},
};
use zui_core::{Color, Dip, Point, Rect, Size};
use zui_platform::InputEvent;

pub struct Button {
    id: WidgetId,
    label: String,
    bounds: Rect,
    hovered: bool,
}

impl Button {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            id: WidgetId::new(),
            label: label.into(),
            bounds: Rect::default(),
            hovered: false,
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
            width: Dip(zui_render::measure_text(&self.label, 3).0 + 32.0),
            height: Dip(40.0),
        });
        self.bounds.size = size;
        size
    }
    fn event(&mut self, event: &UiEvent, ctx: &mut EventContext) -> EventResult {
        if let (InputEvent::CursorMoved { .. }, Some(point)) = (&event.input, event.position) {
            let hovered = self.bounds.contains(point);
            if hovered != self.hovered {
                self.hovered = hovered;
                return EventResult::RequestRedraw;
            }
        }
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
        ctx.fill_rounded_rect(
            self.bounds,
            Dip(8.0),
            Color {
                r: if self.hovered { 0.14 } else { 0.18 },
                g: if self.hovered { 0.34 } else { 0.42 },
                b: if self.hovered { 0.72 } else { 0.86 },
                a: 1.0,
            },
        );
        ctx.draw_text(
            &self.label,
            Point {
                x: Dip(self.bounds.origin.x.0 + 16.0),
                y: Dip(self.bounds.origin.y.0 + 9.0),
            },
            Color::WHITE,
            3,
        );
    }
}
