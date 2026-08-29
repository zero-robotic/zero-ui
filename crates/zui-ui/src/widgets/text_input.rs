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
    cursor: usize,
    selection_anchor: Option<usize>,
    bounds: Rect,
    focused: bool,
    pointer_selecting: bool,
    focus_started: Instant,
    theme: Theme,
}

impl TextInput {
    pub fn new() -> Self {
        Self {
            id: WidgetId::new(),
            text: String::new(),
            cursor: 0,
            selection_anchor: None,
            bounds: Rect::default(),
            focused: false,
            pointer_selecting: false,
            focus_started: Instant::now(),
            theme: Theme::default(),
        }
    }

    pub fn with_text(text: impl Into<String>) -> Self {
        let mut input = Self::new();
        input.text = text.into();
        input.cursor = input.text.len();
        input
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
        self.focus_started = Instant::now();
    }

    fn reset_blink(&mut self) {
        self.focus_started = Instant::now();
    }

    fn selection(&self) -> Option<(usize, usize)> {
        self.selection_anchor
            .map(|anchor| {
                if anchor <= self.cursor {
                    (anchor, self.cursor)
                } else {
                    (self.cursor, anchor)
                }
            })
            .filter(|(start, end)| start != end)
    }

    fn clear_selection(&mut self) {
        self.selection_anchor = None;
    }

    fn previous_boundary(&self, index: usize) -> usize {
        self.text[..index]
            .char_indices()
            .last()
            .map_or(0, |(offset, _)| offset)
    }

    fn next_boundary(&self, index: usize) -> usize {
        self.text[index..]
            .chars()
            .next()
            .map_or(self.text.len(), |character| index + character.len_utf8())
    }

    fn build_render_commands(&self, ctx: &mut crate::PaintContext<'_>, focused: bool) {
        let style = &ctx.theme.text_input;
        let text_origin = Point {
            x: Dip(self.bounds.origin.x.0 + style.padding_x.0),
            y: Dip(self.bounds.origin.y.0
                + (self.bounds.size.height.0 - style.font_size as f32 * 7.0) / 2.0),
        };
        ctx.fill_rect(
            self.bounds,
            if focused {
                style.focused_background
            } else {
                style.background
            },
        );
        if let Some((start, end)) = self.selection() {
            let start_x =
                text_origin.x.0 + zui_render::measure_text(&self.text[..start], style.font_size).0;
            let end_x =
                text_origin.x.0 + zui_render::measure_text(&self.text[..end], style.font_size).0;
            ctx.fill_rect(
                Rect {
                    origin: Point {
                        x: Dip(start_x),
                        y: text_origin.y,
                    },
                    size: Size {
                        width: Dip(end_x - start_x),
                        height: Dip(style.font_size as f32 * 7.0),
                    },
                },
                style.selection_background,
            );
        }
        ctx.draw_text(&self.text, text_origin, style.foreground, style.font_size);
        if focused && ctx.now.duration_since(self.focus_started).as_millis() / 500 % 2 == 0 {
            let caret_x = text_origin.x.0
                + zui_render::measure_text(&self.text[..self.cursor], style.font_size).0;
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

    fn delete_selection(&mut self) -> bool {
        let Some((start, end)) = self.selection() else {
            return false;
        };
        self.text.replace_range(start..end, "");
        self.cursor = start;
        self.clear_selection();
        true
    }

    fn insert_text(&mut self, value: &str) {
        self.delete_selection();
        // A pointer press establishes a zero-width anchor for potential drag
        // selection. Once text is committed, that anchor must collapse or it
        // would turn the newly typed prefix into a selection as the cursor
        // advances.
        self.clear_selection();
        self.text.insert_str(self.cursor, value);
        self.cursor += value.len();
        self.reset_blink();
    }

    fn move_cursor(&mut self, cursor: usize, extending: bool) {
        if extending {
            if self.selection_anchor.is_none() {
                self.selection_anchor = Some(self.cursor);
            }
        } else {
            self.clear_selection();
        }
        self.cursor = cursor;
        self.reset_blink();
    }

    fn cursor_at_x(&self, x: Dip, style: &crate::theme::TextInputStyle) -> usize {
        let target = (x.0 - self.bounds.origin.x.0 - style.padding_x.0).max(0.0);
        let mut previous = 0.0;
        for (index, character) in self.text.char_indices() {
            let end = index + character.len_utf8();
            let width = zui_render::measure_text(&self.text[..end], style.font_size).0;
            if target < (previous + width) / 2.0 {
                return index;
            }
            previous = width;
        }
        self.text.len()
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
        let interval = Duration::from_millis(500);
        Some(
            self.focus_started
                + interval * (self.focus_started.elapsed().as_millis() as u32 / 500 + 1),
        )
    }

    fn event(&mut self, event: &UiEvent, ctx: &mut EventContext) -> EventResult {
        if let (Some(point), InputEvent::CursorMoved { .. }) = (event.position, &event.input) {
            if self.pointer_selecting {
                self.cursor = self.cursor_at_x(point.x, &self.theme.text_input);
                self.reset_blink();
                self.invalidate(ctx, self.bounds);
                return EventResult::RequestRedraw;
            }
        }
        if matches!(
            event.input,
            InputEvent::MouseInput {
                state: KeyState::Released,
                ..
            }
        ) {
            self.pointer_selecting = false;
        }
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
                let focus_changed = ctx.request_focus(self.id);
                self.focused = true;
                self.cursor = self.cursor_at_x(point.x, &self.theme.text_input);
                self.selection_anchor = Some(self.cursor);
                self.pointer_selecting = true;
                self.reset_blink();
                if focus_changed {
                    ctx.emit(self.id, ActionKind::FocusRequested);
                }
                self.invalidate(ctx, self.bounds);
                return EventResult::RequestRedraw;
            }
        }
        let focused = if ctx.has_tree_runtime() {
            ctx.is_focused(self.id)
        } else {
            self.focused
        };
        if !focused {
            return EventResult::Ignored;
        }

        let mut changed = false;
        match &event.input {
            InputEvent::Text(text) if !text.is_empty() => {
                self.insert_text(text);
                changed = true;
            }
            InputEvent::Keyboard {
                key: KeyCode::Character('a' | 'A'),
                state: KeyState::Pressed,
                modifiers,
            } if modifiers.control || modifiers.logo => {
                self.selection_anchor = Some(0);
                self.cursor = self.text.len();
                self.reset_blink();
            }
            InputEvent::Keyboard {
                key: KeyCode::Character(character),
                state: KeyState::Pressed,
                ..
            } => {
                self.insert_text(&character.to_string());
                changed = true;
            }
            InputEvent::Keyboard {
                key: KeyCode::Backspace,
                state: KeyState::Pressed,
                ..
            } => {
                changed = if self.delete_selection() {
                    true
                } else if self.cursor > 0 {
                    let previous = self.previous_boundary(self.cursor);
                    self.text.replace_range(previous..self.cursor, "");
                    self.cursor = previous;
                    self.reset_blink();
                    true
                } else {
                    false
                };
            }
            InputEvent::Keyboard {
                key: KeyCode::ArrowLeft,
                state: KeyState::Pressed,
                modifiers,
            } => {
                if !modifiers.shift && self.selection().is_some() {
                    self.cursor = self.selection().unwrap().0;
                    self.clear_selection();
                    self.reset_blink();
                } else {
                    self.move_cursor(self.previous_boundary(self.cursor), modifiers.shift);
                }
            }
            InputEvent::Keyboard {
                key: KeyCode::ArrowRight,
                state: KeyState::Pressed,
                modifiers,
            } => {
                if !modifiers.shift && self.selection().is_some() {
                    self.cursor = self.selection().unwrap().1;
                    self.clear_selection();
                    self.reset_blink();
                } else {
                    self.move_cursor(self.next_boundary(self.cursor), modifiers.shift);
                }
            }
            InputEvent::Keyboard {
                key: KeyCode::Home,
                state: KeyState::Pressed,
                modifiers,
            } => self.move_cursor(0, modifiers.shift),
            InputEvent::Keyboard {
                key: KeyCode::End,
                state: KeyState::Pressed,
                modifiers,
            } => self.move_cursor(self.text.len(), modifiers.shift),
            _ => return EventResult::Ignored,
        }
        if changed {
            ctx.emit(self.id, ActionKind::TextChanged);
        }
        self.invalidate(ctx, self.bounds);
        EventResult::RequestRedraw
    }

    fn set_theme(&mut self, theme: &Theme) {
        self.theme = theme.clone();
    }

    fn build_render_node(&self, theme: &Theme) -> zui_render::RenderNode {
        crate::widget::build_render_node_with_commands(self.id, self.bounds, theme, |ctx| {
            self.build_render_commands(ctx, self.focused)
        })
    }

    fn build_render_node_incremental(
        &self,
        context: &mut crate::RenderBuildContext<'_>,
    ) -> zui_render::RenderNode {
        if !context.subtree_is_dirty(self.id) {
            if let Some(previous) = context.previous() {
                return previous.clone();
            }
        }
        let focused = context.runtime().is_focused(self.id);
        context.localize(crate::widget::build_render_node_with_commands(
            self.id,
            self.bounds,
            context.theme(),
            |ctx| self.build_render_commands(ctx, focused),
        ))
    }
}
