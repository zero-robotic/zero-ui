//! Multiline adapter over the shared editor, including history, clipboard and IME.
use crate::{TextInput, TextInputView};
use alloc::{string::String, vec::Vec};
use zui_api::{action::ActionId, view::ViewSpec, widget::Invalidation};
use zui_theme::Theme;
#[derive(Clone, Debug, PartialEq)]
pub struct TextAreaView {
    pub editor: TextInputView,
    pub wrap: bool,
    pub visible_lines: u16,
}
impl TextAreaView {
    pub fn themed(
        theme: &Theme,
        text: impl Into<String>,
        families: Vec<String>,
        action: ActionId,
    ) -> Self {
        Self {
            editor: TextInputView::themed(theme, text, families, action),
            wrap: true,
            visible_lines: 4,
        }
    }
    fn input(&self) -> TextInputView {
        let mut editor = self.editor.clone();
        editor.multiline = true;
        editor.password = false;
        editor.wrap = self.wrap;
        editor.visible_lines = self.visible_lines;
        editor
    }
}
impl ViewSpec for TextAreaView {
    type Widget = TextInput;
    fn build(&self) -> TextInput {
        self.input().build()
    }
    fn update(&self, w: &mut TextInput) -> Invalidation {
        self.input().update(w)
    }
}
