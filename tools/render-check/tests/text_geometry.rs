use std::sync::Arc;
use zui_api::text::*;
use zui_paint::{Frame, OffscreenRenderer, Scene};
use zui_text::ParleyText;
use zui_types::{Affine, Color, Extent, Point};
fn verify(renderer: &mut impl OffscreenRenderer) {
    let mut service = ParleyText::default();
    let families = service
        .register_font(Arc::from(
            include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
        ))
        .unwrap();
    let text = "中文选择\n光标对齐";
    let result = service
        .layout_interactive(TextRequest {
            text,
            style: &TextStyle {
                families,
                font_size: 24.0,
                line_height: 1.5,
                align: TextAlign::Start,
            },
            width: TextWidth::AtMost(240.0),
        })
        .unwrap();
    let position = |byte_index| TextPosition {
        byte_index,
        affinity: TextAffinity::Downstream,
    };
    let rects = result
        .geometry
        .selection_rects(position(3), position(9))
        .unwrap();
    let caret = result.geometry.caret(position(3), 3.0).unwrap();
    let hit = result
        .geometry
        .cursor_at(Point::new(
            caret.origin.x,
            caret.origin.y + caret.size.height / 2.0,
        ))
        .unwrap();
    assert_eq!(hit.byte_index, 3);
    let selection_color = Color::rgb(20, 80, 130);
    let caret_color = Color::rgb(0, 220, 240);
    let mut scene = Scene::new();
    scene
        .painter()
        .with_transform(Affine::translate(20.0, 16.0), |painter| {
            for rect in &rects {
                painter.fill_rect(*rect, selection_color);
            }
            result.layout.paint(painter, Color::WHITE);
            painter.fill_rect(caret, caret_color);
        });
    drop(service);
    for scale in [1.0, 2.0] {
        let pixels = renderer
            .render(
                &scene,
                Frame {
                    extent: Extent::new((300.0 * scale) as u32, (140.0 * scale) as u32),
                    scale,
                    background: Color::BLACK,
                },
            )
            .unwrap();
        let x = ((20.0 + caret.origin.x + 1.0) * scale).ceil() as usize;
        let y = ((16.0 + caret.origin.y + caret.size.height / 2.0) * scale) as usize;
        let offset = y * pixels.stride() + x * 4;
        assert_eq!(
            &pixels.pixels()[offset..offset + 4],
            &[caret_color.r, caret_color.g, caret_color.b, 255]
        );
        for color in [selection_color, Color::WHITE] {
            assert!(
                pixels
                    .pixels()
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|pixel| pixel[..3] == [color.r, color.g, color.b])
            );
        }
    }
}
#[cfg(feature = "cpu")]
#[test]
fn selection_caret_and_glyphs_render_cpu() {
    verify(&mut zui_render_cpu::CpuRenderer::default());
}
#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires a real GPU adapter; explicitly run --ignored"]
fn selection_caret_and_glyphs_render_gpu() {
    verify(&mut pollster::block_on(zui_render_gpu::GpuRenderer::new(Default::default())).unwrap());
}
