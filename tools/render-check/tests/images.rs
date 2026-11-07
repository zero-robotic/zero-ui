use std::sync::Arc;
use zui_paint::{ImageLimits, ImageResource, ImageSampling, RenderError, RenderLimits, Scene};
use zui_types::{Affine, Extent};
fn image() -> ImageResource {
    ImageResource::from_rgba8(
        Extent::new(2, 1),
        vec![255, 0, 0, 255, 0, 255, 0, 128],
        ImageLimits::default(),
    )
    .unwrap()
}
#[test]
fn scene_budget_counts_unique_resources_and_rejects_invalid_transforms() {
    let image = image();
    let mut scene = Scene::new();
    for _ in 0..3 {
        scene.painter().image(image.clone(), ImageSampling::Nearest);
    }
    let limits = RenderLimits {
        max_images: 1,
        max_image_bytes: 8,
        ..Default::default()
    };
    scene.validate(limits).unwrap();
    assert!(matches!(
        scene.validate(RenderLimits {
            max_image_bytes: 7,
            ..limits
        }),
        Err(RenderError::SceneTooComplex)
    ));
    let second = ImageResource::from_premultiplied_rgba8(
        image.extent(),
        Arc::from(image.bytes()),
        ImageLimits::default(),
    )
    .unwrap();
    scene.painter().image(second, ImageSampling::Bilinear);
    assert!(matches!(
        scene.validate(limits),
        Err(RenderError::SceneTooComplex)
    ));
    scene.clear();
    scene
        .painter()
        .with_transform(Affine::scale(f64::NAN, 1.0), |p| {
            p.image(image, ImageSampling::Nearest)
        });
    assert!(matches!(
        scene.validate(RenderLimits::default()),
        Err(RenderError::InvalidGeometry)
    ));
}
fn verify_images(renderer: &mut impl zui_paint::OffscreenRenderer) {
    use zui_paint::{Frame, Path};
    use zui_types::{Color, Rect};
    for scale in [1, 2] {
        let frame = Frame {
            extent: Extent::new(12 * scale, 8 * scale),
            scale: scale as f64,
            background: Color::rgb(0, 0, 255),
        };
        let mut scene = Scene::new();
        scene
            .painter()
            .with_clip(Path::rectangle(Rect::new(2.0, 2.0, 5.0, 4.0)), |p| {
                p.with_transform(
                    Affine::translate(2.0, 2.0).compose(Affine::scale(4.0, 4.0)),
                    |p| p.image(image(), ImageSampling::Nearest),
                );
            });
        let pixels = renderer.render(&scene, frame).unwrap();
        let pixel = |x, y| {
            let offset = (y * scale) as usize * pixels.stride() + (x * scale * 4) as usize;
            &pixels.pixels()[offset..offset + 4]
        };
        assert_eq!(pixel(3, 3), &[255, 0, 0, 255]);
        assert_eq!(pixel(6, 3), &[0, 128, 127, 255]);
        assert_eq!(pixel(7, 3), &[0, 0, 255, 255]);
        assert_eq!(pixel(3, 1), &[0, 0, 255, 255]);
        scene.clear();
        let empty = renderer.render(&scene, frame).unwrap();
        assert!(
            empty
                .pixels()
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| *p == [0, 0, 255, 255])
        );
        // A new resource with identical dimensions must replace old cached colors.
        let replacement =
            ImageResource::from_rgba8(Extent::new(2, 1), vec![255; 8], ImageLimits::default())
                .unwrap();
        scene.painter().image(replacement, ImageSampling::Bilinear);
        let pixels = renderer.render(&scene, frame).unwrap();
        assert_eq!(&pixels.pixels()[..4], &[255, 255, 255, 255]);
        let colored = image();
        scene
            .painter()
            .with_transform(Affine::translate(3.0, 0.0), |p| {
                p.image(colored.clone(), ImageSampling::Nearest)
            });
        scene
            .painter()
            .with_transform(Affine::translate(6.0, 0.0), |p| {
                p.image(colored, ImageSampling::Nearest)
            });
        let multiple = renderer.render(&scene, frame).unwrap();
        for x in [3, 6] {
            let offset = (x * scale * 4) as usize;
            assert_eq!(&multiple.pixels()[offset..offset + 4], &[255, 0, 0, 255]);
        }
        assert_eq!(&multiple.pixels()[..4], &[255, 255, 255, 255]);
        assert_eq!(
            renderer.render(&scene, frame).unwrap().pixels(),
            multiple.pixels()
        );
    }
}

fn verify_bilinear(renderer: &mut impl zui_paint::OffscreenRenderer) -> (Scene, zui_paint::Frame) {
    use zui_paint::Frame;
    use zui_types::Color;
    let source = ImageResource::from_rgba8(
        Extent::new(2, 1),
        vec![255, 0, 0, 255, 0, 0, 255, 255],
        ImageLimits::default(),
    )
    .unwrap();
    let frame = Frame {
        extent: Extent::new(8, 4),
        scale: 1.0,
        background: Color::BLACK,
    };
    let mut scene = Scene::new();
    scene
        .painter()
        .with_transform(Affine::scale(4.0, 4.0), |p| {
            p.image(source, ImageSampling::Bilinear)
        });
    let pixels = renderer.render(&scene, frame).unwrap();
    let pixel = &pixels.pixels()[pixels.stride() + 3 * 4..pixels.stride() + 4 * 4];
    assert!(pixel[0] > 0 && pixel[0] < 255);
    assert!(pixel[2] > 0 && pixel[2] < 255);
    assert_eq!(pixel[1], 0);
    assert_eq!(pixel[3], 255);
    (scene, frame)
}

#[cfg(feature = "cpu")]
#[test]
fn bilinear_sampling_interpolates_and_budget_failure_leaves_target_untouched() {
    use zui_paint::PixelBuffer;
    let (scene, frame) = verify_bilinear(&mut zui_render_cpu::CpuRenderer::default());
    let mut target = PixelBuffer::new(frame.extent, RenderLimits::default()).unwrap();
    target.pixels_mut().fill(17);
    let limits = RenderLimits {
        max_images: 0,
        ..Default::default()
    };
    let mut renderer = zui_render_cpu::CpuRenderer::new(limits);
    assert!(matches!(
        renderer.render_into(&scene, frame, &mut target),
        Err(RenderError::SceneTooComplex)
    ));
    assert!(target.pixels().iter().all(|byte| *byte == 17));
}

#[cfg(feature = "cpu")]
#[test]
fn images_render_scaled_clipped_transparent_and_clear_between_frames_cpu() {
    verify_images(&mut zui_render_cpu::CpuRenderer::default());
}
#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires a real GPU adapter; explicitly run --ignored"]
fn images_render_scaled_clipped_transparent_and_clear_between_frames_gpu() {
    use zui_paint::{Frame, OffscreenRenderer};
    let mut renderer =
        pollster::block_on(zui_render_gpu::GpuRenderer::new(Default::default())).unwrap();
    verify_images(&mut renderer);
    verify_bilinear(&mut renderer);
    let width = renderer.device().limits().max_texture_dimension_2d + 1;
    let oversized = ImageResource::from_rgba8(
        Extent::new(width, 1),
        vec![0; width as usize * 4],
        ImageLimits {
            max_dimension: width,
            max_bytes: width as usize * 4,
        },
    )
    .unwrap();
    let mut scene = Scene::new();
    scene.painter().image(oversized, ImageSampling::Nearest);
    let frame = Frame {
        extent: Extent::new(8, 8),
        scale: 1.0,
        background: zui_types::Color::BLACK,
    };
    assert!(matches!(
        renderer.render(&scene, frame),
        Err(RenderError::TargetTooLarge)
    ));
    verify_images(&mut renderer); // A failed preparation does not poison subsequent frames.
}
