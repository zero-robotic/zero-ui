use std::sync::{Arc, OnceLock};
use zui_api::text::*;
use zui_text::{MissingGlyphPolicy, ParleyText, TextLimits};

fn font() -> Arc<[u8]> {
    static BYTES: OnceLock<Arc<[u8]>> = OnceLock::new();
    BYTES
        .get_or_init(|| {
            Arc::from(include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice())
        })
        .clone()
}
fn setup(limits: TextLimits) -> (ParleyText, TextStyle) {
    let mut service = ParleyText::new(limits);
    let families = service.register_font(font()).unwrap();
    let style = TextStyle {
        families,
        font_size: 24.0,
        line_height: 1.3,
        align: TextAlign::Start,
    };
    (service, style)
}
#[test]
fn chinese_latin_combining_marks_and_line_ranges_are_shaped() {
    let (mut service, style) = setup(TextLimits::default());
    let text = "ZUI 中文界面 e\u{301}\n字体排版 123";
    let layout = service
        .layout(TextRequest {
            text,
            style: &style,
            width: TextWidth::Unbounded,
        })
        .unwrap();
    assert_eq!(layout.lines.len(), 2);
    assert!(layout.size.width > 100.0 && layout.size.height > 40.0);
    assert!(layout.first_baseline.unwrap() > 0.0);
    assert!(
        layout
            .runs
            .iter()
            .flat_map(|run| &run.glyphs)
            .all(|glyph| glyph.id != 0)
    );
    for line in &layout.lines {
        assert!(text.is_char_boundary(line.text_range.start));
        assert!(text.is_char_boundary(line.text_range.end));
    }
    assert_eq!(layout.font_epoch, service.font_epoch());
}
#[test]
fn wrapping_reflows_the_same_content_and_preserves_resource_identity() {
    let (mut service, style) = setup(TextLimits::default());
    let text = "布局与文字使用一致的测量结果，窗口变窄后自动换行。";
    let wide = service
        .layout(TextRequest {
            text,
            style: &style,
            width: TextWidth::AtMost(400.0),
        })
        .unwrap();
    let narrow = service
        .layout(TextRequest {
            text,
            style: &style,
            width: TextWidth::AtMost(120.0),
        })
        .unwrap();
    assert!(narrow.lines.len() > wide.lines.len());
    assert!(narrow.size.height > wide.size.height);
    assert!(narrow.size.width <= 120.01);
    assert_eq!(wide.runs[0].font.id(), narrow.runs[0].font.id());
    let min = service
        .layout(TextRequest {
            text,
            style: &style,
            width: TextWidth::MinContent,
        })
        .unwrap();
    assert!(min.lines.len() >= narrow.lines.len());
}
#[test]
fn alignment_changes_positions_without_changing_glyph_selection() {
    let (mut service, mut style) = setup(TextLimits::default());
    let start = service
        .layout(TextRequest {
            text: "ZUI",
            style: &style,
            width: TextWidth::AtMost(200.0),
        })
        .unwrap();
    style.align = TextAlign::End;
    let end = service
        .layout(TextRequest {
            text: "ZUI",
            style: &style,
            width: TextWidth::AtMost(200.0),
        })
        .unwrap();
    assert!(end.runs[0].glyphs[0].x > start.runs[0].glyphs[0].x + 100.0);
    assert_eq!(end.runs[0].glyphs[0].id, start.runs[0].glyphs[0].id);
}
#[test]
fn failed_font_registration_does_not_change_epoch_or_existing_fonts() {
    let (mut service, style) = setup(TextLimits::default());
    let epoch = service.font_epoch();
    assert_eq!(
        service.register_font(Arc::from(&b"not a font"[..])),
        Err(TextError::InvalidFont)
    );
    assert_eq!(service.font_epoch(), epoch);
    assert!(
        service
            .layout(TextRequest {
                text: "仍然可用",
                style: &style,
                width: TextWidth::Unbounded
            })
            .is_ok()
    );
    let mut limited = ParleyText::new(TextLimits {
        max_font_faces: 0,
        ..Default::default()
    });
    assert_eq!(
        limited.register_font(font()),
        Err(TextError::BudgetExceeded)
    );
    assert_eq!(limited.font_epoch(), 0);
    assert_eq!(
        limited
            .layout(TextRequest {
                text: "a",
                style: &style,
                width: TextWidth::Unbounded
            })
            .unwrap_err(),
        TextError::NoFonts
    );
}
#[test]
fn invalid_requests_and_missing_glyphs_are_explicit_errors() {
    let (_, mut style) = setup(TextLimits::default());
    let mut service =
        ParleyText::new_with_policy(TextLimits::default(), MissingGlyphPolicy::Reject);
    service.register_font(font()).unwrap();
    assert_eq!(
        service
            .layout(TextRequest {
                text: "x",
                style: &style,
                width: TextWidth::AtMost(f32::NAN)
            })
            .unwrap_err(),
        TextError::InvalidWidth
    );
    assert_eq!(
        service
            .layout(TextRequest {
                text: "\u{10ffff}",
                style: &style,
                width: TextWidth::Unbounded
            })
            .unwrap_err(),
        TextError::MissingGlyph
    );
    assert_eq!(
        service
            .layout_interactive(TextRequest {
                text: "\u{10ffff}",
                style: &style,
                width: TextWidth::Unbounded,
            })
            .unwrap_err(),
        TextError::MissingGlyph
    );
    style.families = vec!["unregistered font".into()];
    assert!(matches!(
        service.layout(TextRequest {
            text: "x",
            style: &style,
            width: TextWidth::Unbounded
        }),
        Err(TextError::UnknownFamily(_))
    ));
    style.font_size = f32::NAN;
    assert_eq!(
        service
            .layout(TextRequest {
                text: "x",
                style: &style,
                width: TextWidth::Unbounded
            })
            .unwrap_err(),
        TextError::InvalidStyle
    );
}
#[test]
fn text_and_glyph_budgets_bound_requests() {
    let (mut service, style) = setup(TextLimits {
        max_text_bytes: 4,
        max_glyphs: 1,
        ..Default::default()
    });
    assert_eq!(
        service
            .layout(TextRequest {
                text: "abcde",
                style: &style,
                width: TextWidth::Unbounded
            })
            .unwrap_err(),
        TextError::BudgetExceeded
    );
    assert_eq!(
        service
            .layout(TextRequest {
                text: "abc",
                style: &style,
                width: TextWidth::Unbounded
            })
            .unwrap_err(),
        TextError::BudgetExceeded
    );
}
