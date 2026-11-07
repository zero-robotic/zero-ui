use zui_paint::{Frame, OffscreenRenderer, Path, Scene};
use zui_types::{Color, Extent, Point, Rect};

fn verify(renderer: &mut impl OffscreenRenderer) {
    let mut scene = Scene::new();
    let mut p = scene.painter();
    p.fill(
        Path::circle(Point::new(12.0, 12.0), 10.0).unwrap(),
        Color::WHITE,
    );
    p.fill(
        Path::ellipse(Rect::new(26.0, 2.0, 28.0, 16.0)).unwrap(),
        Color::rgb(255, 0, 0),
    );
    p.fill(
        Path::rounded_rectangle(Rect::new(2.0, 28.0, 28.0, 20.0), 8.0).unwrap(),
        Color::rgb(0, 255, 0),
    );
    p.with_clip(Path::circle(Point::new(44.0, 38.0), 10.0).unwrap(), |p| {
        p.fill_rect(Rect::new(32.0, 26.0, 24.0, 24.0), Color::rgb(0, 0, 255));
    });
    p.stroke(
        Path::rounded_rectangle(Rect::new(60.0, 4.0, 24.0, 20.0), 5.0).unwrap(),
        Color::WHITE,
        2.0,
    );
    for scale in [1, 2] {
        let pixels = renderer
            .render(
                &scene,
                Frame {
                    extent: Extent::new(90 * scale, 52 * scale),
                    scale: f64::from(scale),
                    background: Color::BLACK,
                },
            )
            .unwrap();
        // Interior/exterior probes avoid backend-specific antialiasing at curve edges.
        for (x, y, expected) in [
            (12, 12, [255, 255, 255, 255]),
            (2, 2, [0, 0, 0, 255]),
            (40, 10, [255, 0, 0, 255]),
            (26, 2, [0, 0, 0, 255]),
            (16, 38, [0, 255, 0, 255]),
            (2, 28, [0, 0, 0, 255]),
            (44, 38, [0, 0, 255, 255]),
            (34, 28, [0, 0, 0, 255]),
            (72, 4, [255, 255, 255, 255]),
            (72, 14, [0, 0, 0, 255]),
        ] {
            let offset = (y * scale) as usize * pixels.stride() + (x * scale * 4) as usize;
            assert_eq!(
                &pixels.pixels()[offset..offset + 4],
                &expected,
                "({x}, {y}) at {scale}x"
            );
        }
    }
}

#[cfg(feature = "cpu")]
#[test]
fn shapes_fill_stroke_and_clip_cpu() {
    verify(&mut zui_render_cpu::CpuRenderer::default());
}

#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires a real GPU adapter; explicitly run --ignored"]
fn shapes_fill_stroke_and_clip_gpu() {
    verify(&mut pollster::block_on(zui_render_gpu::GpuRenderer::new(Default::default())).unwrap());
}
