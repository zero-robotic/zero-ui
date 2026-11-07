//! Temporary display snapshot; never mutates committed text or edit history.
use super::{TextInputError, single_line};
use alloc::string::String;
use core::ops::Range;
use unicode_segmentation::UnicodeSegmentation;
use zui_edit::{EditError, Selection, TextBuffer};

pub(super) struct Composition {
    pub text: String,
    pub selection: Selection,
    pub range: Range<usize>,
    pub show_caret: bool,
}
impl Composition {
    pub fn new(
        buffer: &TextBuffer,
        preedit: &str,
        cursor: Option<(usize, usize)>,
        limit: usize,
    ) -> Result<Self, TextInputError> {
        single_line(preedit)?;
        if cursor.is_some_and(|(a, b)| !preedit.is_char_boundary(a) || !preedit.is_char_boundary(b))
        {
            return Err(TextInputError::InvalidPreeditCursor);
        }
        let replaced = buffer.selection().range();
        let size = buffer.text().len() - replaced.len();
        let size = size
            .checked_add(preedit.len())
            .filter(|size| *size <= limit)
            .ok_or(TextInputError::Edit(EditError::BudgetExceeded))?;
        let mut text = String::with_capacity(size);
        text.push_str(&buffer.text()[..replaced.start]);
        text.push_str(preedit);
        text.push_str(&buffer.text()[replaced.end..]);
        // Composition may join surrounding graphemes. Geometry only accepts complete
        // grapheme boundaries; preserve the original committed selection separately.
        let next = |index| {
            text.grapheme_indices(true)
                .map(|(i, _)| i)
                .find(|i| *i >= index)
                .unwrap_or(text.len())
        };
        let previous = |index| {
            text.grapheme_indices(true)
                .map(|(i, _)| i)
                .take_while(|i| *i <= index)
                .last()
                .unwrap_or(0)
        };
        let (anchor, focus) = cursor.unwrap_or((preedit.len(), preedit.len()));
        let selection = Selection {
            anchor: next(replaced.start + anchor),
            focus: next(replaced.start + focus),
        };
        let range = previous(replaced.start)..next(replaced.start + preedit.len());
        Ok(Self {
            text,
            selection,
            range,
            show_caret: cursor.is_some(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn combining_preedit_snaps_geometry_without_changing_committed_selection() {
        let buffer = TextBuffer::new("e", Default::default()).unwrap();
        let composition = Composition::new(&buffer, "\u{301}", Some((0, 2)), 16).unwrap();
        assert_eq!(composition.text, "e\u{301}");
        assert_eq!(composition.range, 0..3);
        assert_eq!(composition.selection, Selection::caret(3));
        assert_eq!(buffer.selection(), Selection::caret(1));
        assert_eq!(buffer.text(), "e");
    }
}
