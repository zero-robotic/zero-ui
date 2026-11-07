use std::sync::{Arc, OnceLock};
use unicode_segmentation::UnicodeSegmentation;
use zui_api::text::*;
use zui_text::ParleyText;
use zui_types::Point;
fn setup() -> (ParleyText, TextStyle) {
    static FONT: OnceLock<Arc<[u8]>> = OnceLock::new();
    let mut service = ParleyText::default();
    let families = service
        .register_font(
            FONT.get_or_init(|| {
                Arc::from(include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice())
            })
            .clone(),
        )
        .unwrap();
    (
        service,
        TextStyle {
            families,
            font_size: 24.0,
            line_height: 1.3,
            align: TextAlign::Start,
        },
    )
}
fn position(byte_index: usize) -> TextPosition {
    TextPosition {
        byte_index,
        affinity: TextAffinity::Downstream,
    }
}
#[test]
fn geometry_uses_the_same_shaping_alignment_and_survives_service_drop() {
    let (mut service, mut style) = setup();
    style.align = TextAlign::Center;
    let text = "ZUI 中文 e\u{301}";
    let interactive = service
        .layout_interactive(TextRequest {
            text,
            style: &style,
            width: TextWidth::AtMost(300.0),
        })
        .unwrap();
    let plain = service
        .layout(TextRequest {
            text,
            style: &style,
            width: TextWidth::AtMost(300.0),
        })
        .unwrap();
    assert_eq!(interactive.layout.size, plain.size);
    let a: Vec<_> = interactive
        .layout
        .runs
        .iter()
        .flat_map(|r| r.glyphs.iter().map(|g| (g.id, g.x, g.y)))
        .collect();
    let b: Vec<_> = plain
        .runs
        .iter()
        .flat_map(|r| r.glyphs.iter().map(|g| (g.id, g.x, g.y)))
        .collect();
    assert_eq!(a, b);
    let saved = interactive.clone();
    drop(interactive);
    drop(service);
    let start = saved.geometry.caret(position(0), 1.0).unwrap();
    assert!(start.origin.x > 20.0);
    assert!(start.size.height > 20.0);
    assert_eq!(
        saved
            .geometry
            .cursor_at(Point::new(
                start.origin.x,
                start.origin.y + start.size.height / 2.0
            ))
            .unwrap()
            .byte_index,
        0
    );
    for (offset, _) in text.grapheme_indices(true) {
        let rect = saved.geometry.caret(position(offset), 1.0).unwrap();
        assert!(rect.is_valid());
    }
    let end = saved.geometry.caret(position(text.len()), 1.0).unwrap();
    assert!(end.origin.x > start.origin.x);
}
#[test]
fn soft_wrap_affinity_and_multiline_selection_are_preserved() {
    let (mut service, style) = setup();
    let text = "中文布局与输入控件";
    let layout = service
        .layout_interactive(TextRequest {
            text,
            style: &style,
            width: TextWidth::AtMost(80.0),
        })
        .unwrap();
    assert!(layout.layout.lines.len() >= 3);
    let wrap = layout.layout.lines[1].text_range.start;
    let upstream = TextPosition {
        byte_index: wrap,
        affinity: TextAffinity::Upstream,
    };
    let downstream = position(wrap);
    let before = layout.geometry.caret(upstream, 1.0).unwrap();
    let after = layout.geometry.caret(downstream, 1.0).unwrap();
    assert!(after.origin.y > before.origin.y);
    let rects = layout
        .geometry
        .selection_rects(position(0), position(text.len()))
        .unwrap();
    assert!(rects.len() >= 3);
    assert!(rects.iter().all(|r| r.is_valid() && r.size.height > 0.0));
    assert_eq!(
        rects,
        layout
            .geometry
            .selection_rects(position(text.len()), position(0))
            .unwrap()
    );
    assert!(
        layout
            .geometry
            .selection_rects(upstream, downstream)
            .unwrap()
            .is_empty()
    );
}
#[test]
fn empty_text_trailing_newline_and_invalid_queries_have_defined_results() {
    let (mut service, style) = setup();
    for text in ["", "中文\n", "\n"] {
        let layout = service
            .layout_interactive(TextRequest {
                text,
                style: &style,
                width: TextWidth::Unbounded,
            })
            .unwrap();
        let end = layout.geometry.caret(position(text.len()), 1.0).unwrap();
        assert!(end.is_valid() && end.size.height > 0.0);
        if !text.is_empty() {
            let first = layout.geometry.caret(position(0), 1.0).unwrap();
            assert!(end.origin.y > first.origin.y);
        }
        assert_eq!(
            layout.geometry.caret(position(text.len() + 1), 1.0),
            Err(TextGeometryError::InvalidPosition)
        );
        assert_eq!(
            layout.geometry.caret(position(0), f32::NAN),
            Err(TextGeometryError::InvalidCaretWidth)
        );
        assert_eq!(
            layout.geometry.cursor_at(Point::new(f64::INFINITY, 0.0)),
            Err(TextGeometryError::InvalidPoint)
        );
    }
    let layout = service
        .layout_interactive(TextRequest {
            text: "e\u{301}中",
            style: &style,
            width: TextWidth::Unbounded,
        })
        .unwrap();
    for offset in [1, 2, 4, 5] {
        assert_eq!(
            layout.geometry.caret(position(offset), 1.0),
            Err(TextGeometryError::InvalidPosition)
        );
    }
}
#[test]
fn hit_testing_and_visual_navigation_return_full_graphemes() {
    let (mut service, style) = setup();
    let text = "中e\u{301}文 ffi";
    let layout = service
        .layout_interactive(TextRequest {
            text,
            style: &style,
            width: TextWidth::Unbounded,
        })
        .unwrap();
    let mut boundaries: Vec<_> = text.grapheme_indices(true).map(|(i, _)| i).collect();
    boundaries.push(text.len());
    for x in -20..300 {
        let hit = layout
            .geometry
            .cursor_at(Point::new(x as f64, 12.0))
            .unwrap();
        assert!(boundaries.contains(&hit.byte_index));
    }
    let mut current = position(0);
    let mut indices = vec![0];
    for _ in 0..boundaries.len() * 2 + 2 {
        let next = layout
            .geometry
            .move_visual(current, VisualDirection::Right)
            .unwrap();
        assert!(boundaries.contains(&next.byte_index));
        if next == current {
            break;
        }
        indices.push(next.byte_index);
        current = next;
    }
    assert_eq!(current.byte_index, text.len());
    assert!(boundaries.iter().all(|i| indices.contains(i)));
    for _ in 0..boundaries.len() * 2 + 2 {
        current = layout
            .geometry
            .move_visual(current, VisualDirection::Left)
            .unwrap();
    }
    assert_eq!(current.byte_index, 0);
}
#[test]
fn bidi_visual_order_is_not_logical_byte_order() {
    let (mut service, style) = setup();
    // Explicit RTL override exercises bidi geometry using the registered Latin font.
    let text = "a \u{202e}XYZ\u{202c} b";
    let layout = service
        .layout_interactive(TextRequest {
            text,
            style: &style,
            width: TextWidth::Unbounded,
        })
        .unwrap();
    let mut current = position(0);
    let mut indices = vec![0];
    for _ in 0..50 {
        let next = layout
            .geometry
            .move_visual(current, VisualDirection::Right)
            .unwrap();
        if next == current {
            break;
        }
        indices.push(next.byte_index);
        current = next;
    }
    assert!(
        indices.windows(2).any(|pair| pair[1] < pair[0]),
        "{indices:?}"
    );
    for (i, _) in text.grapheme_indices(true) {
        assert!(layout.geometry.caret(position(i), 1.0).unwrap().is_valid());
    }
}

#[test]
fn missing_glyph_keeps_original_byte_geometry_in_default_mode() {
    let (mut service, style) = setup();
    let text = "A\u{10ffff}中";
    let layout = service
        .layout_interactive(TextRequest {
            text,
            style: &style,
            width: TextWidth::Unbounded,
        })
        .unwrap();
    assert_eq!(layout.layout.missing_glyph_count(), 1);
    assert_eq!(layout.layout.lines[0].text_range, 0..text.len());
    let before = layout.geometry.caret(position(1), 1.0).unwrap();
    let after = layout.geometry.caret(position(5), 1.0).unwrap();
    assert!(after.origin.x > before.origin.x);
    assert_eq!(
        layout
            .geometry
            .move_visual(position(1), VisualDirection::Right)
            .unwrap()
            .byte_index,
        5
    );
    assert!(
        !layout
            .geometry
            .selection_rects(position(1), position(5))
            .unwrap()
            .is_empty()
    );
    assert!(layout.geometry.caret(position(2), 1.0).is_err());
    let plain = service
        .layout(TextRequest {
            text,
            style: &style,
            width: TextWidth::Unbounded,
        })
        .unwrap();
    assert_eq!(plain.size, layout.layout.size);
    assert_eq!(plain.missing_glyph_count(), 1);
}
