use std::sync::Arc;
use zui_paint::{
    FontResource, Frame, Glyph, GlyphRun, OffscreenRenderer, PixelBuffer, RenderError,
    RenderLimits, Scene,
};
use zui_types::{Color, Extent, Rect};

fn frame(scale: f64) -> Frame {
    Frame {
        extent: Extent::new((640.0 * scale) as u32, (360.0 * scale) as u32),
        scale,
        background: Color::rgb(5, 14, 25),
    }
}
fn assert_ink(pixels: &PixelBuffer, scale: usize) {
    let mut changed = 0;
    // This box contains only the title glyphs, no background decoration.
    for y in 20 * scale..80 * scale {
        for x in 20 * scale..400 * scale {
            let offset = y * pixels.stride() + x * 4;
            changed += usize::from(pixels.pixels()[offset..offset + 4] != [5, 14, 25, 255]);
        }
    }
    assert!(changed > 1000 * scale, "title glyphs were not rasterized");
    // Outside the paragraph clip and below all other elements remains untouched.
    for y in 340 * scale..360 * scale {
        for x in 0..640 * scale {
            let offset = y * pixels.stride() + x * 4;
            assert_eq!(&pixels.pixels()[offset..offset + 4], &[5, 14, 25, 255]);
        }
    }
}
fn verify(renderer: &mut impl OffscreenRenderer) {
    let scene = zui_render_check::text_preview().unwrap();
    let run = scene
        .commands()
        .iter()
        .find_map(|command| match command {
            zui_paint::Command::Glyphs { run, .. } => Some(run.clone()),
            _ => None,
        })
        .unwrap();
    let mut clipped = Scene::new();
    clipped.painter().with_clip(
        zui_paint::Path::rectangle(Rect::new(0.0, 0.0, 60.0, 20.0)),
        |painter| painter.glyphs(run, Color::WHITE),
    );
    for scale in [1.0, 2.0] {
        assert_ink(
            &renderer.render(&scene, frame(scale)).unwrap(),
            scale as usize,
        );
        let pixels = renderer.render(&clipped, frame(scale)).unwrap();
        let mut ink = 0;
        for y in 0..pixels.extent().height as usize {
            for x in 0..pixels.extent().width as usize {
                let offset = y * pixels.stride() + x * 4;
                let changed = pixels.pixels()[offset..offset + 4] != [5, 14, 25, 255];
                if x < 60 * scale as usize && y < 20 * scale as usize {
                    ink += usize::from(changed);
                } else {
                    assert!(!changed, "glyph escaped clip at {x},{y}");
                }
            }
        }
        assert!(ink > 20, "clipped glyphs were not drawn");
    }
    let empty = renderer.render(&Scene::new(), frame(1.0)).unwrap();
    assert!(
        empty
            .pixels()
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| *p == [5, 14, 25, 255])
    );
}
#[cfg(feature = "cpu")]
#[test]
fn cpu_renders_shaped_text_after_service_drop() {
    verify(&mut zui_render_cpu::CpuRenderer::default());
}
#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires a real GPU adapter; explicitly run --ignored"]
fn gpu_renders_shaped_text_after_service_drop() {
    verify(&mut pollster::block_on(zui_render_gpu::GpuRenderer::new(Default::default())).unwrap());
}
#[cfg(all(feature = "cpu", feature = "gpu"))]
#[test]
#[ignore = "requires a real GPU adapter; explicitly run --ignored"]
fn text_matches_across_backends() {
    let scene = zui_render_check::text_preview().unwrap();
    let cpu = zui_render_cpu::CpuRenderer::default()
        .render(&scene, frame(1.0))
        .unwrap();
    let gpu = pollster::block_on(zui_render_gpu::GpuRenderer::new(Default::default()))
        .unwrap()
        .render(&scene, frame(1.0))
        .unwrap();
    let total: u64 = cpu
        .pixels()
        .iter()
        .zip(gpu.pixels())
        .map(|(a, b)| u64::from(a.abs_diff(*b)))
        .sum();
    assert!(total as f64 / (cpu.pixels().len() as f64) < 1.0);
    assert_ink(&gpu, 1);
}
#[test]
fn font_and_glyph_budgets_and_invalid_positions_are_checked() {
    let scene = zui_render_check::text_preview().unwrap();
    assert!(matches!(
        scene.validate(RenderLimits {
            max_glyphs: 1,
            ..Default::default()
        }),
        Err(RenderError::SceneTooComplex)
    ));
    assert!(matches!(
        scene.validate(RenderLimits {
            max_font_bytes: 1,
            ..Default::default()
        }),
        Err(RenderError::SceneTooComplex)
    ));
    let font = FontResource::new(Arc::from(&b"invalid font"[..]), 0).unwrap();
    let mut scene = Scene::new();
    scene.painter().glyphs(
        GlyphRun {
            font,
            font_size: 20.0,
            glyphs: vec![Glyph {
                id: 1,
                x: f32::NAN,
                y: 0.0,
            }],
            normalized_coords: vec![],
        },
        Color::WHITE,
    );
    assert!(matches!(
        scene.validate(RenderLimits::default()),
        Err(RenderError::InvalidGeometry)
    ));
}
#[cfg(feature = "cpu")]
#[test]
fn invalid_font_does_not_modify_caller_pixels() {
    let font = FontResource::new(Arc::from(&b"invalid font"[..]), 0).unwrap();
    let mut scene = Scene::new();
    scene.painter().glyphs(
        GlyphRun {
            font,
            font_size: 20.0,
            glyphs: vec![Glyph {
                id: 1,
                x: 10.0,
                y: 20.0,
            }],
            normalized_coords: vec![],
        },
        Color::WHITE,
    );
    let mut target = PixelBuffer::new(frame(1.0).extent, RenderLimits::default()).unwrap();
    target.pixels_mut().fill(42);
    let result =
        zui_render_cpu::CpuRenderer::default().render_into(&scene, frame(1.0), &mut target);
    assert!(matches!(result, Err(RenderError::InvalidFont)));
    assert!(target.pixels().iter().all(|&b| b == 42));
}

fn verify_missing_glyph(renderer: &mut impl OffscreenRenderer) {
    use zui_api::text::{TextAlign, TextRequest, TextService, TextStyle, TextWidth};
    let mut text = zui_text::ParleyText::default();
    let families = text
        .register_font(Arc::from(
            include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
        ))
        .unwrap();
    let style = TextStyle {
        families,
        font_size: 24.0,
        line_height: 1.3,
        align: TextAlign::Start,
    };
    let layout = text
        .layout(TextRequest {
            text: "\u{10ffff}",
            style: &style,
            width: TextWidth::Unbounded,
        })
        .unwrap();
    assert_eq!(layout.missing_glyph_count(), 1);
    let mut scene = Scene::new();
    scene
        .painter()
        .with_transform(zui_types::Affine::translate(8.0, 8.0), |painter| {
            layout.paint(painter, Color::WHITE)
        });
    for scale in [1.0, 2.0] {
        let pixels = renderer
            .render(
                &scene,
                Frame {
                    extent: Extent::new((64.0 * scale) as u32, (64.0 * scale) as u32),
                    scale,
                    background: Color::BLACK,
                },
            )
            .unwrap();
        let ink = pixels
            .pixels()
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| pixel[..3] != [0, 0, 0])
            .count();
        assert!(
            ink > (10.0 * scale * scale) as usize,
            "font placeholder must produce visible ink"
        );
    }
}
#[cfg(feature = "cpu")]
#[test]
fn missing_glyph_placeholder_renders_cpu() {
    verify_missing_glyph(&mut zui_render_cpu::CpuRenderer::default());
}
#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires a real GPU adapter; explicitly run --ignored"]
fn missing_glyph_placeholder_renders_gpu() {
    verify_missing_glyph(
        &mut pollster::block_on(zui_render_gpu::GpuRenderer::new(Default::default())).unwrap(),
    );
}
