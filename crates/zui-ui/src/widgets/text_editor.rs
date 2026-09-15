use crate::{
    event::{EventContext, EventResult, UiEvent},
    layout::Constraints,
    theme::Theme,
    Text, TextWrap, Widget, WidgetId,
};
use zui_core::{Rect, Size};
use zui_platform::{ImeEvent, InputEvent, KeyCode, KeyState};
/// Multi-line document editor foundation; selection/IME/undo extend this retained document model.
pub struct TextEditor {
    id: WidgetId,
    document: String,
    view: Text,
}
impl TextEditor {
    pub fn new() -> Self {
        Self::with_text("")
    }
    pub fn with_text(text: impl Into<String>) -> Self {
        let document = text.into();
        Self {
            id: WidgetId::new(),
            view: Text::new(document.clone()).wrap(TextWrap::Word),
            document,
        }
    }
    pub fn text(&self) -> &str {
        &self.document
    }
    fn sync(&mut self) {
        self.view.set_text(self.document.clone())
    }
}
impl Default for TextEditor {
    fn default() -> Self {
        Self::new()
    }
}
impl Widget for TextEditor {
    fn id(&self) -> WidgetId {
        self.id
    }
    fn bounds(&self) -> Rect {
        self.view.bounds()
    }
    fn measure(&mut self, c: Constraints) -> Size {
        self.view.measure(c)
    }
    fn arrange(&mut self, b: Rect) {
        self.view.arrange(b)
    }
    fn event(&mut self, e: &UiEvent, c: &mut EventContext) -> EventResult {
        match &e.input {
            InputEvent::Text(value) => self.document.push_str(value),
            InputEvent::Ime(ImeEvent::Commit(value)) => self.document.push_str(value),
            InputEvent::Keyboard {
                key: KeyCode::Enter,
                state: KeyState::Pressed,
                ..
            } => self.document.push('\n'),
            InputEvent::Keyboard {
                key: KeyCode::Backspace,
                state: KeyState::Pressed,
                ..
            } => {
                self.document.pop();
            }
            _ => return EventResult::Ignored,
        };
        self.sync();
        self.invalidate(c, self.bounds());
        EventResult::RequestRedraw
    }
    fn set_theme(&mut self, t: &Theme) {
        self.view.set_theme(t)
    }
    fn build_render_node(&self, t: &Theme) -> zui_render::RenderNode {
        self.view.build_render_node(t)
    }
}
