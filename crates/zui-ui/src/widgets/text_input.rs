use crate::{
    event::{ActionKind, EventContext, EventResult, UiEvent},
    layout::Constraints,
    theme::Theme,
    widget::{Widget, WidgetId},
};
use std::time::{Duration, Instant};
use zui_core::{Dip, Point, Rect, Size};
use zui_platform::{InputEvent, KeyCode, KeyState};

pub struct TextInput {
    id: WidgetId,
    text: String,
    bounds: Rect,
    focused: bool,
    focus_started: Instant,
    theme: Theme,
}

impl TextInput {
    pub fn new() -> Self {
        Self {
            id: WidgetId::new(),
            text: String::new(),
            bounds: Rect::default(),
            focused: false,
            focus_started: Instant::now(),
            theme: Theme::default(),
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
    fn arrange(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }
    fn measure(&mut self, constraints: Constraints) -> Size {
        let size = constraints.constrain(Size {
            width: self.theme.text_input.width,
            height: self.theme.text_input.height,
        });
        self.bounds.size = size;
        size
    }
    fn next_redraw(&self) -> Option<Instant> {
        if !self.focused {
            return None;
        }
        let elapsed = self.focus_started.elapsed();
        let interval = Duration::from_millis(500);
        Some(self.focus_started + interval * (elapsed.as_millis() as u32 / 500 + 1))
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
                let focus_changed = !self.focused;
                self.focused = true;
                self.focus_started = Instant::now();
                if focus_changed {
                    ctx.emit(self.id, ActionKind::FocusRequested);
                }
                ctx.invalidate(self.bounds);
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
                ctx.invalidate(self.bounds);
                EventResult::RequestRedraw
            }
            InputEvent::Keyboard {
                key: KeyCode::Character(character),
                state: KeyState::Pressed,
                ..
            } => {
                self.text.push(*character);
                ctx.emit(self.id, ActionKind::TextChanged);
                ctx.invalidate(self.bounds);
                EventResult::RequestRedraw
            }
            InputEvent::Keyboard {
                key: KeyCode::Backspace,
                state: KeyState::Pressed,
                ..
            } => {
                self.text.pop();
                ctx.emit(self.id, ActionKind::TextChanged);
                ctx.invalidate(self.bounds);
                EventResult::RequestRedraw
            }
            _ => EventResult::Ignored,
        }
    }
    fn set_theme(&mut self, theme: &Theme) {
        self.theme = theme.clone();
    }
    fn paint(&self, ctx: &mut crate::PaintContext<'_>) {
        let style = &ctx.theme.text_input;
        ctx.fill_rect(
            self.bounds,
            if self.focused {
                style.focused_background
            } else {
                style.background
            },
        );
        ctx.draw_text(
            &self.text,
            Point {
                x: Dip(self.bounds.origin.x.0 + style.padding_x.0),
                y: Dip(self.bounds.origin.y.0
                    + (self.bounds.size.height.0 - style.font_size as f32 * 7.0) / 2.0),
            },
            style.foreground,
            style.font_size,
        );
        if self.focused && ctx.now.duration_since(self.focus_started).as_millis() / 500 % 2 == 0 {
            let caret_x = self.bounds.origin.x.0
                + style.padding_x.0
                + zui_render::measure_text(&self.text, style.font_size).0;
            ctx.fill_rect(
                Rect {
                    origin: Point {
                        x: Dip(caret_x),
                        y: Dip(self.bounds.origin.y.0 + (self.bounds.size.height.0 - 26.0) / 2.0),
                    },
                    size: Size {
                        width: Dip(2.0),
                        height: Dip(26.0),
                    },
                },
                style.caret_color,
            );
        }
    }
}
