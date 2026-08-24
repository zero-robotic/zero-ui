use crate::{
    event::{is_left_press, ActionKind, EventContext, EventResult, UiEvent},
    layout::Constraints,
    theme::Theme,
    widget::{Widget, WidgetId},
};
use zui_core::{Dip, Point, Rect, Size};
use zui_platform::InputEvent;

pub struct Button {
    id: WidgetId,
    label: String,
    bounds: Rect,
    hovered: bool,
    theme: Theme,
}

impl Button {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            id: WidgetId::new(),
            label: label.into(),
            bounds: Rect::default(),
            hovered: false,
            theme: Theme::default(),
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
    fn arrange(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }
    fn measure(&mut self, constraints: Constraints) -> Size {
        let size = constraints.constrain(Size {
            width: Dip(
                zui_render::measure_text(&self.label, self.theme.button.font_size).0
                    + self.theme.button.padding_x.0 * 2.0,
            ),
            height: self.theme.button.height,
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
    fn set_theme(&mut self, theme: &Theme) {
        self.theme = theme.clone();
    }
    fn paint(&self, ctx: &mut crate::PaintContext<'_>) {
        let style = &ctx.theme.button;
        ctx.fill_rounded_rect(
            self.bounds,
            style.radius,
            if self.hovered {
                style.hover_background
            } else {
                style.background
            },
        );
        ctx.draw_text(
            &self.label,
            Point {
                x: Dip(self.bounds.origin.x.0 + style.padding_x.0),
                y: Dip(self.bounds.origin.y.0
                    + (self.bounds.size.height.0 - style.font_size as f32 * 7.0) / 2.0),
            },
            style.foreground,
            style.font_size,
        );
    }
}
