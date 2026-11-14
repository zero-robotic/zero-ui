#![cfg(feature = "system-fonts")]

use zui_api::text::*;
use zui_text::{MissingGlyphPolicy, ParleyText, TextLimits};

fn style(families: Vec<String>) -> TextStyle {
    TextStyle {
        families,
        font_size: 24.0,
        line_height: 1.3,
        align: TextAlign::Start,
    }
}

#[test]
fn enabling_feature_does_not_change_default_font_source() {
    let mut service = ParleyText::default();
    assert!(service.font_family_names().is_empty());
    assert_eq!(service.resolve_families(&[]), Err(TextError::NoFonts));
    assert!(matches!(
        service.layout(TextRequest {
            text: "ZUI",
            style: &style(vec!["sans-serif".into()]),
            width: TextWidth::Unbounded,
        }),
        Err(TextError::NoFonts)
    ));
}

// Explicit environment checks: require installed Latin and Chinese fonts.
#[test]
#[ignore = "requires installed system Latin and Chinese fonts"]
fn system_discovery_named_selection_fallback_geometry_and_resource_identity() {
    let mut service =
        ParleyText::with_system_fonts_and_policy(TextLimits::default(), MissingGlyphPolicy::Reject);
    let names = service.font_family_names();
    assert!(!names.is_empty());
    let families = service
        .resolve_families(&["Arial", "Helvetica", "Liberation Sans", "DejaVu Sans"])
        .unwrap();
    assert!(families.iter().all(|name| names.contains(name)));
    let generic = service.resolve_families(&[]).unwrap();
    assert_eq!(
        service
            .resolve_families(&["ZUI nonexistent family 987654"])
            .unwrap(),
        generic
    );
    let style = style(families);
    let epoch = service.font_epoch();
    let request = || TextRequest {
        text: "Hello 中文界面",
        style: &style,
        width: TextWidth::AtMost(400.0),
    };
    let layout = service.layout(request()).unwrap();
    assert_eq!(layout.missing_glyph_count(), 0);
    let ids: std::collections::BTreeSet<_> = layout.runs.iter().map(|run| run.font.id()).collect();
    assert!(
        ids.len() >= 2,
        "expected a Chinese fallback face after the Latin face"
    );
    let interactive = service.layout_interactive(request()).unwrap();
    assert_eq!(
        service.font_epoch(),
        epoch,
        "lazy loading must not invalidate measurement caches"
    );
    assert_eq!(layout.runs.len(), interactive.layout.runs.len());
    for (a, b) in layout.runs.iter().zip(&interactive.layout.runs) {
        assert_eq!(a.font.id(), b.font.id());
        assert_eq!(a.glyphs, b.glyphs);
    }
    let unknown = unknown_style();
    assert!(matches!(
        service.layout(TextRequest {
            text: "A",
            style: &unknown,
            width: TextWidth::Unbounded
        }),
        Err(TextError::UnknownFamily(_))
    ));
    drop(service);
    assert!(layout.runs.iter().all(|run| !run.font.bytes().is_empty()));
    interactive
        .geometry
        .caret(
            TextPosition {
                byte_index: 0,
                affinity: TextAffinity::Downstream,
            },
            1.0,
        )
        .unwrap();
}

fn unknown_style() -> TextStyle {
    style(vec!["ZUI nonexistent family 987654".into()])
}

#[test]
#[ignore = "requires installed system fonts"]
fn system_fonts_obey_retained_byte_and_face_budgets() {
    for limits in [
        TextLimits {
            max_font_bytes: 0,
            ..Default::default()
        },
        TextLimits {
            max_font_faces: 0,
            ..Default::default()
        },
    ] {
        let mut service = ParleyText::with_system_fonts(limits);
        let style = style(service.resolve_families(&[]).unwrap());
        let epoch = service.font_epoch();
        for _ in 0..2 {
            assert!(matches!(
                service.layout(TextRequest {
                    text: "Hello",
                    style: &style,
                    width: TextWidth::Unbounded,
                }),
                Err(TextError::BudgetExceeded)
            ));
            assert_eq!(service.font_epoch(), epoch);
        }
    }
}
