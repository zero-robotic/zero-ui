use crate::{
    event::{ActionKind, EventContext, EventResult, UiEvent},
    layout::Constraints,
    widget::{Widget, WidgetId},
};
use std::time::Instant;
use zui_core::{Color, Dip, Point, Rect, Size};
use zui_platform::{InputEvent, KeyCode, KeyState};

pub struct TextInput {
    id: WidgetId,
    text: String,
    bounds: Rect,
    focused: bool,
    focus_started: Instant,
}

impl TextInput {
    pub fn new() -> Self {
        Self {
            id: WidgetId::new(),
            text: String::new(),
            bounds: Rect::default(),
            focused: false,
            focus_started: Instant::now(),
        }
    }
    pub fn with_text(text: impl Into<String>) -> Self {
        let mut input = Self::new();
        input.text = text.into();
        input
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }
}

impl Default for TextInput {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for TextInput {
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
            width: Dip(240.0),
            height: Dip(32.0),
        });
        self.bounds.size = size;
        size
    }
    fn event(&mut self, event: &UiEvent, ctx: &mut EventContext) -> EventResult {
        if let Some(point) = event.position {
            if self.bounds.contains(point)
                && matches!(
                    event.input,
                    InputEvent::MouseInput {
                        state: KeyState::Pressed,
                        ..
                    }
                )
            {
                self.focused = true;
                self.focus_started = Instant::now();
                ctx.emit(self.id, ActionKind::FocusRequested);
                return EventResult::RequestRedraw;
            }
        }
        if !self.focused {
            return EventResult::Ignored;
        }
        match &event.input {
            InputEvent::Text(text) => {
                self.text.push_str(text);
                ctx.emit(self.id, ActionKind::TextChanged);
                EventResult::RequestRedraw
            }
            InputEvent::Keyboard {
                key: KeyCode::Character(character),
                state: KeyState::Pressed,
                ..
            } => {
                self.text.push(*character);
                ctx.emit(self.id, ActionKind::TextChanged);
                EventResult::RequestRedraw
            }
            InputEvent::Keyboard {
                key: KeyCode::Backspace,
                state: KeyState::Pressed,
                ..
            } => {
                self.text.pop();
                ctx.emit(self.id, ActionKind::TextChanged);
                EventResult::RequestRedraw
            }
            _ => EventResult::Ignored,
        }
    }
    fn paint(&self, ctx: &mut crate::PaintContext<'_>) {
        let color = if self.focused {
            Color {
                r: 0.92,
                g: 0.95,
                b: 1.0,
                a: 1.0,
            }
        } else {
            Color {
                r: 0.82,
                g: 0.85,
                b: 0.9,
                a: 1.0,
            }
        };
        ctx.fill_rect(self.bounds, color);
        ctx.draw_text(
            &self.text,
            Point {
                x: Dip(self.bounds.origin.x.0 + 8.0),
                y: Dip(self.bounds.origin.y.0 + 9.0),
            },
            Color::BLACK,
            2,
        );
        if self.focused && ctx.now.duration_since(self.focus_started).as_millis() / 500 % 2 == 0 {
            let caret_x = self.bounds.origin.x.0 + 8.0 + self.text.chars().count() as f32 * 12.0;
            ctx.fill_rect(
                Rect {
                    origin: Point {
                        x: Dip(caret_x),
                        y: Dip(self.bounds.origin.y.0 + 6.0),
                    },
                    size: Size {
                        width: Dip(2.0),
                        height: Dip(20.0),
                    },
                },
                Color::BLACK,
            );
        }
    }
}
