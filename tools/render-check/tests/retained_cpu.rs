#![cfg(feature = "cpu")]
use zui_paint::{Frame, OffscreenRenderer, RenderLimits, Scene};
use zui_render_cpu::{CpuRenderer, RetainedCpuRenderer};
use zui_types::{Affine, Color, Extent, Rect};

fn frame() -> Frame {
    Frame {
        extent: Extent::new(80, 60),
        scale: 1.0,
        background: Color::TRANSPARENT,
    }
}
fn scene(color: Color, offset: f64) -> Scene {
    let mut scene = Scene::new();
    scene
        .painter()
        .with_transform(Affine::translate(offset, 0.0), |p| {
            p.with_clip(
                zui_paint::Path::rectangle(Rect::new(2.0, 2.0, 40.0, 40.0)),
                |p| {
                    p.fill_rect(Rect::new(0.0, 0.0, 50.0, 50.0), color);
                },
            );
        });
    scene
}
fn verify(renderer: &mut RetainedCpuRenderer, scene: &Scene, frame: Frame, reused: bool) {
    let expected = CpuRenderer::default().render(scene, frame).unwrap();
    let actual = renderer.render(scene, frame).unwrap();
    assert_eq!(expected.pixels(), actual.pixels());
    assert_eq!(actual.extent(), frame.extent);
    assert_eq!(renderer.reused_frame(), reused);
}
#[test]
fn equal_commands_reuse_pixels_and_changed_commands_replace_them() {
    let mut renderer = RetainedCpuRenderer::default();
    let a = scene(Color::rgba(255, 0, 0, 128), 0.0);
    verify(&mut renderer, &a, frame(), false);
    let allocation = renderer.render(&a, frame()).unwrap().pixels().as_ptr();
    // Independently built commands, not pointer equality or a hash shortcut.
    verify(
        &mut renderer,
        &scene(Color::rgba(255, 0, 0, 128), 0.0),
        frame(),
        true,
    );
    assert_eq!(
        renderer.render(&a, frame()).unwrap().pixels().as_ptr(),
        allocation
    );
    verify(&mut renderer, &scene(Color::WHITE, 0.0), frame(), false);
    verify(&mut renderer, &scene(Color::WHITE, 3.0), frame(), false);
    verify(&mut renderer, &Scene::new(), frame(), false);
    verify(&mut renderer, &Scene::new(), frame(), true);
    renderer.invalidate();
    assert!(!renderer.reused_frame());
    verify(&mut renderer, &Scene::new(), frame(), false);
}
#[test]
fn extent_scale_background_and_failed_frames_invalidate_the_key() {
    let mut renderer = RetainedCpuRenderer::default();
    let scene = scene(Color::WHITE, 0.0);
    let mut frame = frame();
    verify(&mut renderer, &scene, frame, false);
    frame.scale = 2.0;
    verify(&mut renderer, &scene, frame, false);
    frame.background = Color::rgb(1, 2, 3);
    verify(&mut renderer, &scene, frame, false);
    frame.extent = Extent::new(90, 70);
    verify(&mut renderer, &scene, frame, false);
    for bad in [
        Frame {
            scale: f64::NAN,
            ..frame
        },
        Frame {
            extent: Extent::new(0, 1),
            ..frame
        },
    ] {
        assert!(renderer.render(&scene, bad).is_err());
        assert!(!renderer.reused_frame());
        verify(&mut renderer, &scene, frame, false);
    }
    let mut invalid = Scene::new();
    invalid
        .painter()
        .fill_rect(Rect::new(f64::NAN, 0.0, 1.0, 1.0), Color::WHITE);
    assert!(renderer.render(&invalid, frame).is_err());
    verify(&mut renderer, &scene, frame, false);
}
#[test]
fn resource_replacement_and_limits_are_not_hidden_by_cache_hits() {
    use zui_paint::{ImageResource, ImageSampling};
    let make = |color| {
        let image = ImageResource::from_rgba8(
            Extent::new(1, 1),
            vec![color, 0, 0, 255],
            Default::default(),
        )
        .unwrap();
        let mut scene = Scene::new();
        scene.painter().image(image, ImageSampling::Nearest);
        scene
    };
    let mut renderer = RetainedCpuRenderer::default();
    let a = make(200);
    verify(&mut renderer, &a, frame(), false);
    verify(&mut renderer, &a, frame(), true);
    verify(&mut renderer, &make(100), frame(), false);
    let mut limited = RetainedCpuRenderer::new(RenderLimits {
        max_commands: 0,
        ..Default::default()
    });
    verify(&mut limited, &Scene::new(), frame(), false);
    assert!(limited.render(&a, frame()).is_err());
    verify(&mut limited, &Scene::new(), frame(), false);
}
