use zui_paint::{Frame, OffscreenRenderer, PixelBuffer, RenderError, RenderLimits, Scene};
#[cfg(feature = "cpu")]
use zui_types::Rect;
use zui_types::{Color, Extent};

fn frame() -> Frame {
    Frame {
        extent: Extent::new(65, 64),
        scale: 2.0,
        background: Color::TRANSPARENT,
    }
}

fn pixel(buffer: &PixelBuffer, x: usize, y: usize) -> [u8; 4] {
    let offset = y * buffer.stride() + x * 4;
    buffer.pixels()[offset..offset + 4].try_into().unwrap()
}

fn assert_scene(renderer: &mut impl OffscreenRenderer) {
    let target = renderer
        .render(&zui_render_check::regression_scene(), frame())
        .unwrap();
    assert_eq!(pixel(&target, 2, 2), [128, 0, 0, 128]);
    assert_eq!(pixel(&target, 25, 9), [0, 255, 0, 255]);
    assert_eq!(pixel(&target, 30, 14), [0, 0, 255, 255]);
    assert_eq!(pixel(&target, 25, 22), [255, 255, 255, 255]);
    assert_eq!(pixel(&target, 22, 10), [0, 0, 0, 0]);
    assert_eq!(pixel(&target, 50, 50), [255, 255, 255, 255]);
    assert_eq!(pixel(&target, 64, 63), [0, 0, 0, 0]);
    // A second frame must not retain old scene contents or painter state.
    let mut clear = frame();
    clear.extent = Extent::new(17, 13);
    clear.background = Color::rgb(10, 20, 30);
    let target = renderer.render(&Scene::new(), clear).unwrap();
    assert!(
        target
            .pixels()
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| *p == [10, 20, 30, 255])
    );
}

fn assert_layout_scene(renderer: &mut impl OffscreenRenderer) {
    for scale in [1.0, 2.0] {
        let scene = zui_render_check::layout_preview(zui_types::Size::new(640.0, 360.0)).unwrap();
        let pixels = renderer
            .render(
                &scene,
                Frame {
                    extent: Extent::new((640.0 * scale) as u32, (360.0 * scale) as u32),
                    scale,
                    background: Color::BLACK,
                },
            )
            .unwrap();
        for (x, y, color) in [
            (20, 20, [25, 50, 80, 255]),
            (180, 20, [41, 210, 230, 255]),
            (410, 20, [255, 192, 83, 255]),
            (180, 190, [95, 100, 220, 255]),
            (410, 190, [45, 170, 135, 255]),
            (170, 20, [0, 0, 0, 255]),
            (400, 20, [0, 0, 0, 255]),
        ] {
            assert_eq!(
                pixel(&pixels, x * scale as usize, y * scale as usize),
                color
            );
        }
    }
}

#[cfg(feature = "cpu")]
#[test]
fn cpu_renders_shared_layout_at_two_scales() {
    assert_layout_scene(&mut zui_render_cpu::CpuRenderer::default());
}

#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires a real GPU adapter; explicitly run --ignored"]
fn gpu_renders_shared_layout_at_two_scales() {
    let mut renderer =
        pollster::block_on(zui_render_gpu::GpuRenderer::new(RenderLimits::default())).unwrap();
    assert_layout_scene(&mut renderer);
}

#[cfg(feature = "cpu")]
#[test]
fn cpu_clipping_scale_alpha_and_resize() {
    assert_scene(&mut zui_render_cpu::CpuRenderer::default());
}

#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires a real GPU adapter; explicitly run --ignored"]
fn gpu_clipping_scale_alpha_and_resize() {
    let mut renderer =
        pollster::block_on(zui_render_gpu::GpuRenderer::new(RenderLimits::default())).unwrap();
    eprintln!("adapter: {}", renderer.adapter_name());
    assert_scene(&mut renderer);
}

#[cfg(all(feature = "cpu", feature = "gpu"))]
#[test]
#[ignore = "requires a real GPU adapter; explicitly run --ignored"]
fn cpu_gpu_preview_matches_with_antialiasing_tolerance() {
    let frame = Frame {
        extent: Extent::new(640, 360),
        scale: 1.0,
        background: Color::rgb(5, 14, 25),
    };
    let scene = zui_render_check::preview();
    let cpu = zui_render_cpu::CpuRenderer::default()
        .render(&scene, frame)
        .unwrap();
    let gpu = pollster::block_on(zui_render_gpu::GpuRenderer::new(RenderLimits::default()))
        .unwrap()
        .render(&scene, frame)
        .unwrap();
    let mut error = 0_u64;
    let mut substantially_different = 0_usize;
    for (a, b) in cpu.pixels().iter().zip(gpu.pixels()) {
        let difference = a.abs_diff(*b);
        error += u64::from(difference);
        substantially_different += usize::from(difference > 16);
    }
    assert!(
        error as f64 / (cpu.pixels().len() as f64) < 1.0,
        "mean channel error too high"
    );
    assert!(
        substantially_different < cpu.pixels().len() / 100,
        "more than 1% channels differ substantially"
    );
}

#[cfg(feature = "cpu")]
#[test]
fn cpu_reuses_output_and_rejects_invalid_input_before_drawing() {
    let mut renderer = zui_render_cpu::CpuRenderer::default();
    let mut target = PixelBuffer::new(frame().extent, RenderLimits::default()).unwrap();
    renderer
        .render_into(&zui_render_check::regression_scene(), frame(), &mut target)
        .unwrap();
    renderer
        .render_into(&Scene::new(), frame(), &mut target)
        .unwrap();
    assert!(target.pixels().iter().all(|&byte| byte == 0));
    let mut scene = Scene::new();
    scene
        .painter()
        .fill_rect(Rect::new(f64::NAN, 0.0, 10.0, 10.0), Color::WHITE);
    assert!(matches!(
        renderer.render(&scene, frame()),
        Err(RenderError::InvalidGeometry)
    ));
    let mut oversized = frame();
    oversized.extent = Extent::new(100_000, 1);
    assert!(matches!(
        renderer.render(&Scene::new(), oversized),
        Err(RenderError::TargetTooLarge)
    ));
}

#[test]
fn scene_complexity_and_target_budgets_are_enforced() {
    let limits = RenderLimits {
        max_commands: 1,
        ..Default::default()
    };
    assert!(matches!(
        zui_render_check::regression_scene().validate(limits),
        Err(RenderError::SceneTooComplex)
    ));
    assert!(matches!(
        Frame {
            extent: Extent::new(0, 10),
            ..frame()
        }
        .validate(limits),
        Err(RenderError::InvalidTarget)
    ));
    let limits = RenderLimits {
        max_pixels: 16,
        ..Default::default()
    };
    assert!(matches!(
        frame().validate(limits),
        Err(RenderError::TargetTooLarge)
    ));
}
