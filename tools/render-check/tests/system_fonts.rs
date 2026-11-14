#![cfg(all(feature = "system-fonts", feature = "cpu"))]

use zui_api::text::{TextAlign, TextRequest, TextService, TextStyle, TextWidth};
use zui_paint::{Frame, OffscreenRenderer, Scene};
use zui_text::{MissingGlyphPolicy, ParleyText};
use zui_types::{Color, Extent};

#[test]
#[ignore = "requires installed system Latin and Chinese fonts"]
fn system_fallback_fonts_render_after_text_service_is_dropped() {
    let mut text =
        ParleyText::with_system_fonts_and_policy(Default::default(), MissingGlyphPolicy::Reject);
    let style = TextStyle {
        families: text.resolve_families(&[]).unwrap(),
        font_size: 24.0,
        line_height: 1.3,
        align: TextAlign::Start,
    };
    let layout = text
        .layout(TextRequest {
            text: "System fonts 系统中文字体 预设 50%",
            style: &style,
            width: TextWidth::Unbounded,
        })
        .unwrap();
    assert_eq!(layout.missing_glyph_count(), 0);
    drop(text);
    let mut scene = Scene::new();
    layout.paint(&mut scene.painter(), Color::WHITE);
    let frame = Frame {
        extent: Extent::new(640, 80),
        scale: 1.0,
        background: Color::BLACK,
    };
    let pixels = zui_render_cpu::CpuRenderer::default()
        .render(&scene, frame)
        .unwrap();
    assert!(
        pixels
            .pixels()
            .as_chunks::<4>()
            .0
            .iter()
            .any(|pixel| pixel[0] > 0)
    );
}
