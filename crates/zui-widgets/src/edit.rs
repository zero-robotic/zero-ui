//! Logical Unicode editing, independent of widgets, input devices and shaping.
use alloc::{
    collections::VecDeque,
    string::{String, ToString},
    vec::Vec,
};
use core::{fmt, ops::Range};
use unicode_segmentation::UnicodeSegmentation;

/// UTF-8 byte offsets. The buffer validates both endpoints as grapheme boundaries.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    pub anchor: usize,
    pub focus: usize,
}
impl Selection {
    pub const fn caret(offset: usize) -> Self {
        Self {
            anchor: offset,
            focus: offset,
        }
    }
    pub fn range(self) -> Range<usize> {
        self.anchor.min(self.focus)..self.anchor.max(self.focus)
    }
    pub fn is_caret(self) -> bool {
        self.anchor == self.focus
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Movement {
    PreviousGrapheme,
    NextGrapheme,
    /// Previous non-whitespace Unicode word-bound segment start, in logical order.
    PreviousWord,
    /// Next non-whitespace Unicode word-bound segment end, in logical order.
    NextWord,
    TextStart,
    TextEnd,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditLimits {
    pub max_text_bytes: usize,
    /// Combined undo/redo payload bytes (inserted plus removed UTF-8).
    pub max_history_bytes: usize,
    pub max_history_entries: usize,
}
impl Default for EditLimits {
    fn default() -> Self {
        Self {
            max_text_bytes: 1024 * 1024,
            max_history_bytes: 1024 * 1024,
            max_history_entries: 256,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditError {
    InvalidSelection,
    BudgetExceeded,
}
impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "text edit error: {self:?}")
    }
}
impl core::error::Error for EditError {}
struct Edit {
    start: usize,
    removed: String,
    inserted: String,
    before: Selection,
    after: Selection,
}
impl Edit {
    fn bytes(&self) -> usize {
        self.removed.len() + self.inserted.len()
    }
}
/// A committed text buffer. IME preedit and visual caret geometry are separate concerns.
pub struct TextBuffer {
    text: String,
    selection: Selection,
    limits: EditLimits,
    undo: VecDeque<Edit>,
    redo: Vec<Edit>,
    history_bytes: usize,
}

/// Owned buffer capacities, excluding this value, allocator metadata and other UI
/// resources. History capacity includes record arrays and retained string buffers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EditMemoryUsage {
    pub text_bytes: usize,
    pub text_capacity_bytes: usize,
    pub history_payload_bytes: usize,
    pub history_capacity_bytes: usize,
}
impl EditMemoryUsage {
    pub fn allocated_bytes(self) -> usize {
        self.text_capacity_bytes
            .saturating_add(self.history_capacity_bytes)
    }
}
impl TextBuffer {
    /// Initial caret is at the end; the buffer does not normalize Unicode or newlines.
    pub fn new(text: impl Into<String>, limits: EditLimits) -> Result<Self, EditError> {
        let text = text.into();
        if text.len() > limits.max_text_bytes {
            return Err(EditError::BudgetExceeded);
        }
        Ok(Self {
            selection: Selection::caret(text.len()),
            text,
            limits,
            undo: VecDeque::new(),
            redo: Vec::new(),
            history_bytes: 0,
        })
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn selection(&self) -> Selection {
        self.selection
    }
    pub fn selected_text(&self) -> &str {
        &self.text[self.selection.range()]
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn history_bytes(&self) -> usize {
        self.history_bytes
    }
    pub fn history_entries(&self) -> usize {
        self.undo.len() + self.redo.len()
    }
    /// O(history entries); not intended as a per-frame query.
    pub fn memory_usage(&self) -> EditMemoryUsage {
        let records = self
            .undo
            .capacity()
            .saturating_add(self.redo.capacity())
            .saturating_mul(core::mem::size_of::<Edit>());
        let history_capacity_bytes =
            self.undo
                .iter()
                .chain(&self.redo)
                .fold(records, |bytes, edit| {
                    bytes
                        .saturating_add(edit.removed.capacity())
                        .saturating_add(edit.inserted.capacity())
                });
        EditMemoryUsage {
            text_bytes: self.text.len(),
            text_capacity_bytes: self.text.capacity(),
            history_payload_bytes: self.history_bytes,
            history_capacity_bytes,
        }
    }
    /// Discard undo/redo and their backing arrays, preserving text and selection.
    /// Returns the owned history-buffer capacity released, not allocator RSS.
    pub fn release_history(&mut self) -> usize {
        let released = self.memory_usage().history_capacity_bytes;
        self.undo = VecDeque::new();
        self.redo = Vec::new();
        self.history_bytes = 0;
        released
    }
    fn boundary(&self, offset: usize) -> bool {
        offset == self.text.len() || self.text.grapheme_indices(true).any(|(i, _)| i == offset)
    }
    fn previous(&self, offset: usize) -> usize {
        self.text
            .grapheme_indices(true)
            .map(|(i, _)| i)
            .take_while(|i| *i < offset)
            .last()
            .unwrap_or(0)
    }
    fn next(&self, offset: usize) -> usize {
        self.text
            .grapheme_indices(true)
            .map(|(i, _)| i)
            .find(|i| *i > offset)
            .unwrap_or(self.text.len())
    }
    fn word_boundary(&self, offset: usize, forward: bool) -> usize {
        let segments = self
            .text
            .split_word_bound_indices()
            .filter(|(_, segment)| !segment.chars().all(char::is_whitespace));
        let boundary = if forward {
            segments
                .map(|(start, segment)| start + segment.len())
                .find(|end| *end > offset)
                .unwrap_or(self.text.len())
        } else {
            segments
                .map(|(start, _)| start)
                .take_while(|start| *start < offset)
                .last()
                .unwrap_or(0)
        };
        // Word segmentation and grapheme segmentation are independent contracts.
        // Never expose an interior grapheme position, including across scripts.
        if self.boundary(boundary) {
            boundary
        } else if forward {
            self.next(boundary)
        } else {
            self.previous(boundary)
        }
    }
    fn destination(&self, movement: Movement) -> usize {
        match movement {
            Movement::PreviousGrapheme => self.previous(self.selection.focus),
            Movement::NextGrapheme => self.next(self.selection.focus),
            Movement::PreviousWord => self.word_boundary(self.selection.focus, false),
            Movement::NextWord => self.word_boundary(self.selection.focus, true),
            Movement::TextStart => 0,
            Movement::TextEnd => self.text.len(),
        }
    }
    pub fn set_selection(&mut self, selection: Selection) -> Result<bool, EditError> {
        if !self.boundary(selection.anchor) || !self.boundary(selection.focus) {
            return Err(EditError::InvalidSelection);
        }
        let changed = self.selection != selection;
        self.selection = selection;
        Ok(changed)
    }
    pub fn select_all(&mut self) -> bool {
        let selected = Selection {
            anchor: 0,
            focus: self.text.len(),
        };
        let changed = self.selection != selected;
        self.selection = selected;
        changed
    }
    /// Logical order only. Visual left/right in bidi text belongs to the layout service.
    pub fn move_cursor(&mut self, movement: Movement, extend: bool) -> bool {
        let selected = self.selection.range();
        let focus = match movement {
            Movement::PreviousGrapheme | Movement::PreviousWord
                if !extend && !self.selection.is_caret() =>
            {
                selected.start
            }
            Movement::NextGrapheme | Movement::NextWord
                if !extend && !self.selection.is_caret() =>
            {
                selected.end
            }
            movement => self.destination(movement),
        };
        let next = Selection {
            anchor: if extend { self.selection.anchor } else { focus },
            focus,
        };
        let changed = next != self.selection;
        self.selection = next;
        changed
    }
    /// Returns whether text or selection changed. One replacement is one undo unit.
    pub fn replace_selection(&mut self, inserted: &str) -> Result<bool, EditError> {
        self.replace(self.selection.range(), inserted)
    }
    pub fn delete_backward(&mut self) -> Result<bool, EditError> {
        self.delete_toward(Movement::PreviousGrapheme)
    }
    pub fn delete_forward(&mut self) -> Result<bool, EditError> {
        self.delete_toward(Movement::NextGrapheme)
    }
    /// Deletes the selection, otherwise through the preceding non-whitespace segment start.
    pub fn delete_word_backward(&mut self) -> Result<bool, EditError> {
        self.delete_toward(Movement::PreviousWord)
    }
    /// Deletes the selection, otherwise through the following non-whitespace segment end.
    pub fn delete_word_forward(&mut self) -> Result<bool, EditError> {
        self.delete_toward(Movement::NextWord)
    }
    fn delete_toward(&mut self, movement: Movement) -> Result<bool, EditError> {
        let range = if self.selection.is_caret() {
            let destination = self.destination(movement);
            destination.min(self.selection.focus)..destination.max(self.selection.focus)
        } else {
            self.selection.range()
        };
        self.replace(range, "")
    }
    fn replace(&mut self, range: Range<usize>, inserted: &str) -> Result<bool, EditError> {
        let remaining = self.text.len() - range.len();
        let Some(new_len) = remaining.checked_add(inserted.len()) else {
            return Err(EditError::BudgetExceeded);
        };
        if new_len > self.limits.max_text_bytes {
            return Err(EditError::BudgetExceeded);
        }
        let before = self.selection;
        let removed = self.text[range.clone()].to_string();
        let changed = removed != inserted;
        self.text.replace_range(range.clone(), inserted);
        let insertion_end = range.start + inserted.len();
        // Insertions/deletions may join neighboring graphemes (combining marks,
        // ZWJ emoji, regional indicators). Never publish an interior caret.
        let focus = if self.boundary(insertion_end) {
            insertion_end
        } else {
            self.next(insertion_end)
        };
        self.selection = Selection::caret(focus);
        if changed {
            self.record(Edit {
                start: range.start,
                removed,
                inserted: inserted.to_string(),
                before,
                after: self.selection,
            });
        }
        Ok(changed || before != self.selection)
    }
    fn record(&mut self, edit: Edit) {
        for future in self.redo.drain(..) {
            self.history_bytes -= future.bytes();
        }
        let bytes = edit.bytes();
        if bytes > self.limits.max_history_bytes || self.limits.max_history_entries == 0 {
            // An unrecorded edit breaks the chain; old deltas cannot safely apply.
            self.clear_history();
            return;
        }
        while self.undo.len() >= self.limits.max_history_entries
            || self.history_bytes > self.limits.max_history_bytes - bytes
        {
            let oldest = self
                .undo
                .pop_front()
                .expect("nonempty history exceeds budget");
            self.history_bytes -= oldest.bytes();
        }
        self.history_bytes += bytes;
        self.undo.push_back(edit);
    }
    pub fn undo(&mut self) -> bool {
        let Some(edit) = self.undo.pop_back() else {
            return false;
        };
        self.text
            .replace_range(edit.start..edit.start + edit.inserted.len(), &edit.removed);
        self.selection = edit.before;
        self.redo.push(edit);
        true
    }
    pub fn redo(&mut self) -> bool {
        let Some(edit) = self.redo.pop() else {
            return false;
        };
        self.text
            .replace_range(edit.start..edit.start + edit.removed.len(), &edit.inserted);
        self.selection = edit.after;
        self.undo.push_back(edit);
        true
    }
    pub fn clear_history(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.history_bytes = 0;
    }
    /// External replacement starts a new editing session. Failure leaves everything intact.
    pub fn reset(&mut self, text: impl Into<String>) -> Result<(), EditError> {
        let text = text.into();
        if text.len() > self.limits.max_text_bytes {
            return Err(EditError::BudgetExceeded);
        }
        self.selection = Selection::caret(text.len());
        self.text = text;
        self.clear_history();
        Ok(())
    }
}

/// UTF-8 offsets at user-perceived character boundaries, including the final
/// byte length. Useful for display transformations without exposing Unicode crates.
pub fn grapheme_boundaries(text: &str) -> impl Iterator<Item = usize> + '_ {
    text.grapheme_indices(true)
        .map(|(index, _)| index)
        .chain(core::iter::once(text.len()))
}
