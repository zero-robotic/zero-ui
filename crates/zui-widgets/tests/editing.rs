use unicode_segmentation::UnicodeSegmentation;
use zui_widgets::edit::{EditError, EditLimits, Movement, Selection, TextBuffer};
fn buffer(text: &str) -> TextBuffer {
    TextBuffer::new(text, EditLimits::default()).unwrap()
}
fn valid(buffer: &TextBuffer) {
    let mut boundaries: Vec<_> = buffer
        .text()
        .grapheme_indices(true)
        .map(|(i, _)| i)
        .collect();
    boundaries.push(buffer.text().len());
    assert!(boundaries.contains(&buffer.selection().anchor));
    assert!(boundaries.contains(&buffer.selection().focus));
}
#[test]
fn logical_navigation_and_deletion_preserve_user_perceived_characters() {
    let parts = ["中", "e\u{301}", "👩🏽‍💻", "🇨🇳", "\r\n", "क्ष"];
    let text = parts.concat();
    let mut b = buffer(&text);
    let mut offset = text.len();
    for part in parts.iter().rev() {
        assert!(b.move_cursor(Movement::PreviousGrapheme, false));
        offset -= part.len();
        assert_eq!(b.selection(), Selection::caret(offset));
    }
    assert!(!b.move_cursor(Movement::PreviousGrapheme, false));
    for part in parts {
        let old = b.text().to_owned();
        b.delete_forward().unwrap();
        assert_eq!(b.text(), &old[part.len()..]);
        valid(&b);
    }
    assert!(!b.delete_backward().unwrap());
    assert!(!b.delete_forward().unwrap());
    for _ in parts {
        assert!(b.undo());
    }
    assert_eq!(b.text(), text);
}
#[test]
fn reverse_selection_extension_collapse_and_replacement_are_undoable() {
    let mut b = buffer("a中文z");
    b.set_selection(Selection {
        anchor: 7,
        focus: 1,
    })
    .unwrap();
    assert_eq!(b.selected_text(), "中文");
    b.replace_selection("👩‍💻").unwrap();
    assert_eq!(b.text(), "a👩‍💻z");
    assert_eq!(b.selection(), Selection::caret("a👩‍💻".len()));
    assert!(b.undo());
    assert_eq!(
        b.selection(),
        Selection {
            anchor: 7,
            focus: 1
        }
    );
    assert!(b.redo());
    b.move_cursor(Movement::TextEnd, false);
    b.move_cursor(Movement::PreviousGrapheme, true);
    b.move_cursor(Movement::PreviousGrapheme, true);
    assert_eq!(b.selected_text(), "👩‍💻z");
    b.move_cursor(Movement::NextGrapheme, false);
    assert_eq!(b.selection(), Selection::caret(b.text().len()));
    b.select_all();
    b.move_cursor(Movement::PreviousGrapheme, false);
    assert_eq!(b.selection(), Selection::caret(0));
}
#[test]
fn insertion_and_deletion_can_merge_neighbors_without_leaving_an_interior_caret() {
    let mut b = buffer("\u{301}x");
    b.set_selection(Selection::caret(0)).unwrap();
    b.replace_selection("e").unwrap();
    assert_eq!(b.text(), "e\u{301}x");
    assert_eq!(b.selection(), Selection::caret(3));
    b.undo();
    assert_eq!(b.text(), "\u{301}x");
    b.redo();
    assert_eq!(b.selection(), Selection::caret(3));
    let mut b = buffer("👩💻");
    b.set_selection(Selection::caret("👩".len())).unwrap();
    b.replace_selection("\u{200d}").unwrap();
    assert_eq!(b.text(), "👩‍💻");
    assert_eq!(b.selection(), Selection::caret(b.text().len()));
    b.delete_backward().unwrap();
    assert_eq!(b.text(), "");
    b.undo();
    b.undo();
    assert_eq!(b.text(), "👩💻");
    let mut b = buffer("🇨x🇳");
    b.set_selection(Selection {
        anchor: 4,
        focus: 5,
    })
    .unwrap();
    b.replace_selection("").unwrap();
    assert_eq!(b.text(), "🇨🇳");
    assert_eq!(b.selection(), Selection::caret(8));
    b.undo();
    assert_eq!(
        b.selection(),
        Selection {
            anchor: 4,
            focus: 5
        }
    );
}
#[test]
fn invalid_selection_or_text_budget_failure_preserves_text_selection_and_history() {
    let mut b = TextBuffer::new(
        "e\u{301}",
        EditLimits {
            max_text_bytes: 6,
            ..Default::default()
        },
    )
    .unwrap();
    b.replace_selection("中").unwrap();
    let selection = b.selection();
    for offset in [1, 2, 4, 7, usize::MAX] {
        assert_eq!(
            b.set_selection(Selection::caret(offset)),
            Err(EditError::InvalidSelection)
        );
        assert_eq!(b.selection(), selection);
    }
    assert_eq!(b.replace_selection("x"), Err(EditError::BudgetExceeded));
    assert_eq!(b.reset("too long"), Err(EditError::BudgetExceeded));
    assert_eq!(b.text(), "e\u{301}中");
    assert_eq!(b.history_entries(), 1);
    assert!(b.undo());
    assert_eq!(b.text(), "e\u{301}");
    assert_eq!(b.replace_selection("long"), Err(EditError::BudgetExceeded));
    assert!(b.can_redo());
    b.redo();
    assert_eq!(b.text(), "e\u{301}中");
}
#[test]
fn history_is_bounded_across_undo_redo_branches_and_unrecorded_edits() {
    let mut b = TextBuffer::new(
        "",
        EditLimits {
            max_text_bytes: 64,
            max_history_bytes: 3,
            max_history_entries: 2,
        },
    )
    .unwrap();
    for value in ["a", "b", "c"] {
        b.replace_selection(value).unwrap();
    }
    assert_eq!(b.history_entries(), 2);
    assert_eq!(b.history_bytes(), 2);
    b.undo();
    b.undo();
    assert!(!b.undo());
    assert_eq!(b.text(), "a");
    assert_eq!(b.history_bytes(), 2);
    b.redo();
    b.replace_selection("X").unwrap();
    assert!(!b.can_redo());
    assert_eq!(b.text(), "abX");
    b.replace_selection("long").unwrap();
    assert_eq!(b.history_bytes(), 0);
    assert!(!b.can_undo());
    b.replace_selection("z").unwrap();
    assert!(b.undo());
    assert_eq!(b.text(), "abXlong");
    b.reset("新").unwrap();
    assert_eq!(b.selection(), Selection::caret(3));
    assert_eq!(b.history_entries(), 0);
    let mut b = TextBuffer::new(
        "",
        EditLimits {
            max_history_entries: 0,
            ..Default::default()
        },
    )
    .unwrap();
    b.replace_selection("ok").unwrap();
    assert!(!b.can_undo());
}
#[test]
fn replacement_round_trips_across_grapheme_boundaries_and_joining_sequences() {
    for text in [
        "",
        "中文",
        "e\u{301}x",
        "👩💻",
        "🇨🇳🇺🇸",
        "\r\n\n",
        "\u{301}x",
    ] {
        let mut boundaries: Vec<_> = text.grapheme_indices(true).map(|(i, _)| i).collect();
        boundaries.push(text.len());
        for &start in &boundaries {
            for &end in &boundaries {
                for insertion in ["a", "中", "\u{301}", "\u{200d}", "🇨", "\r\n"] {
                    let mut b = buffer(text);
                    let selection = Selection {
                        anchor: start,
                        focus: end,
                    };
                    b.set_selection(selection).unwrap();
                    b.replace_selection(insertion).unwrap();
                    valid(&b);
                    let after = (b.text().to_owned(), b.selection());
                    if b.can_undo() {
                        b.undo();
                        assert_eq!(b.text(), text);
                        assert_eq!(b.selection(), selection);
                        b.redo();
                        assert_eq!((b.text(), b.selection()), (after.0.as_str(), after.1));
                    }
                    valid(&b);
                }
            }
        }
    }
}

#[test]
fn word_navigation_skips_whitespace_and_keeps_punctuation_and_emoji_segments() {
    let text = "one,  e\u{301} 👩🏽‍💻 中文";
    let segments = ["one", ",", "e\u{301}", "👩🏽‍💻", "中", "文"];
    let mut b = buffer(text);
    b.move_cursor(Movement::TextStart, false);
    for segment in segments {
        b.move_cursor(Movement::NextWord, false);
        assert_eq!(
            b.selection().focus,
            text.find(segment).unwrap() + segment.len()
        );
        valid(&b);
    }
    assert!(!b.move_cursor(Movement::NextWord, false));
    for segment in segments.into_iter().rev() {
        b.move_cursor(Movement::PreviousWord, false);
        assert_eq!(b.selection().focus, text.find(segment).unwrap());
        valid(&b);
    }
    assert!(!b.move_cursor(Movement::PreviousWord, false));
    let mut b = buffer("   \t\r\n");
    b.move_cursor(Movement::PreviousWord, false);
    assert_eq!(b.selection().focus, 0);
    b.move_cursor(Movement::NextWord, true);
    assert_eq!(b.selected_text(), "   \t\r\n");
}

#[test]
fn word_deletions_restore_exact_text_and_selection_at_every_grapheme_boundary() {
    for text in [
        "one  two!",
        "don't stop",
        "中文 e\u{301}",
        "👩🏽‍💻 🇨🇳",
        "क्ष abc",
        "\t\r\n",
    ] {
        for offset in text
            .grapheme_indices(true)
            .map(|(i, _)| i)
            .chain([text.len()])
        {
            for forward in [true, false] {
                let mut b = buffer(text);
                b.set_selection(Selection::caret(offset)).unwrap();
                let changed = if forward {
                    b.delete_word_forward()
                } else {
                    b.delete_word_backward()
                }
                .unwrap();
                valid(&b);
                if changed {
                    let deleted = b.text().to_owned();
                    assert!(b.undo());
                    assert_eq!(b.text(), text);
                    assert_eq!(b.selection(), Selection::caret(offset));
                    assert!(b.redo());
                    assert_eq!(b.text(), deleted);
                }
            }
        }
    }
    let mut b = buffer("one two three");
    let selection = Selection {
        anchor: 7,
        focus: 4,
    };
    b.set_selection(selection).unwrap();
    b.delete_word_backward().unwrap();
    assert_eq!(b.text(), "one  three");
    b.undo();
    assert_eq!(b.selection(), selection);
    b.move_cursor(Movement::PreviousWord, false);
    assert_eq!(b.selection(), Selection::caret(4));
    b.set_selection(selection).unwrap();
    b.move_cursor(Movement::NextWord, false);
    assert_eq!(b.selection(), Selection::caret(7));
}

#[test]
fn memory_usage_distinguishes_payload_from_capacity_and_releases_both_history_stacks() {
    let mut text = String::with_capacity(512);
    text.push('中');
    let capacity = text.capacity();
    let mut b = TextBuffer::new(text, Default::default()).unwrap();
    assert_eq!(b.memory_usage().text_bytes, 3);
    assert_eq!(b.memory_usage().text_capacity_bytes, capacity);
    for part in ["文", "a", "b"] {
        b.replace_selection(part).unwrap();
    }
    assert!(b.undo());
    assert!(b.can_undo() && b.can_redo());
    b.select_all();
    let before = b.memory_usage();
    assert_eq!(before.history_payload_bytes, b.history_bytes());
    assert!(before.history_capacity_bytes > before.history_payload_bytes);
    let text = b.text().to_owned();
    let selection = b.selection();
    assert_eq!(b.release_history(), before.history_capacity_bytes);
    let after = b.memory_usage();
    assert_eq!(after.history_capacity_bytes, 0);
    assert_eq!(after.history_payload_bytes, 0);
    assert_eq!(after.text_capacity_bytes, before.text_capacity_bytes);
    assert_eq!(
        before.allocated_bytes() - after.allocated_bytes(),
        before.history_capacity_bytes
    );
    assert_eq!(b.text(), text);
    assert_eq!(b.selection(), selection);
    assert!(!b.undo() && !b.redo());
    assert_eq!(b.release_history(), 0);
    b.replace_selection("新").unwrap();
    assert!(b.undo());
    assert_eq!(b.text(), text);
    b.clear_history();
    assert_eq!(b.history_bytes(), 0);
    assert!(b.memory_usage().history_capacity_bytes > 0);
    assert!(b.release_history() > 0);
}
