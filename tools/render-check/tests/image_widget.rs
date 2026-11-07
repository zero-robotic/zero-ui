use zui_api::{
    layout::{Dimension, Edges, LayoutStyle},
    view::View,
    widget::Invalidation,
};
use zui_core::Tree;
use zui_paint::{Frame, ImageResource, ImageSampling, OffscreenRenderer};
use zui_types::{Color, Extent, Point, Size};
use zui_widgets::{ImageAlign, ImageFit, ImageView};
fn source(width: u32, height: u32) -> ImageResource {
    let mut pixels = Vec::new();
    for _ in 0..height {
        for x in 0..width {
            pixels.extend_from_slice(if x < width / 2 {
                &[255, 0, 0, 255]
            } else {
                &[0, 255, 0, 255]
            });
        }
    }
    let mut encoded = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut encoded, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&pixels).unwrap();
        writer.finish().unwrap();
    }
    zui_image::decode_png(&encoded, zui_image::DecodeLimits::default()).unwrap()
}
fn layout(tree: &mut Tree, size: Size) {
    tree.layout(&mut zui_layout::TaffyLayout::default(), size)
        .unwrap();
}
#[test]
fn intrinsic_size_aspect_constraints_and_resource_updates_use_public_layout() {
    let mut view = ImageView::new(source(40, 20));
    let describe = |v: &ImageView, width, height| {
        View::new(v.clone()).styled(LayoutStyle {
            width,
            height,
            ..Default::default()
        })
    };
    let mut tree = Tree::from_view(
        describe(&view, Dimension::Auto, Dimension::Auto),
        Default::default(),
    )
    .unwrap();
    for (width, height, viewport, expected) in [
        (
            Dimension::Auto,
            Dimension::Auto,
            Size::new(100.0, 100.0),
            Size::new(40.0, 20.0),
        ),
        (
            Dimension::Length(80.0),
            Dimension::Auto,
            Size::new(100.0, 100.0),
            Size::new(80.0, 40.0),
        ),
        (
            Dimension::Auto,
            Dimension::Length(30.0),
            Size::new(100.0, 100.0),
            Size::new(60.0, 30.0),
        ),
        (
            Dimension::Auto,
            Dimension::Auto,
            Size::new(20.0, 100.0),
            Size::new(20.0, 10.0),
        ),
    ] {
        tree.reconcile(describe(&view, width, height)).unwrap();
        layout(&mut tree, viewport);
        assert_eq!(tree.layout_box(tree.root()).unwrap().bounds.size, expected);
    }
    let root = tree.root();
    tree.scene().unwrap();
    view.image = source(40, 20);
    tree.reconcile(describe(&view, Dimension::Auto, Dimension::Auto))
        .unwrap();
    assert_eq!(tree.invalidation(), Invalidation::Paint);
    view.fit = ImageFit::Cover;
    tree.reconcile(describe(&view, Dimension::Auto, Dimension::Auto))
        .unwrap();
    assert_eq!(tree.invalidation(), Invalidation::Paint);
    tree.scene().unwrap();
    view.image = source(20, 40);
    tree.reconcile(describe(&view, Dimension::Auto, Dimension::Auto))
        .unwrap();
    assert_eq!(tree.invalidation(), Invalidation::Layout);
    layout(&mut tree, Size::new(100.0, 100.0));
    assert_eq!(tree.root(), root);
    assert!(tree.hit_test(Point::new(1.0, 1.0)).unwrap().is_none());
    assert!(tree.request_focus(Some(root)).is_err());
}
fn verify(renderer: &mut impl OffscreenRenderer) {
    let mut view = ImageView::new(source(4, 2));
    view.sampling = ImageSampling::Nearest;
    let describe = |v: &ImageView| {
        View::new(v.clone()).styled(LayoutStyle {
            width: Dimension::Length(12.0),
            height: Dimension::Length(12.0),
            padding: Edges::all(2.0),
            ..Default::default()
        })
    };
    let mut tree = Tree::from_view(describe(&view), Default::default()).unwrap();
    layout(&mut tree, Size::new(12.0, 12.0));
    for scale in [1, 2] {
        for fit in [
            ImageFit::Contain,
            ImageFit::Cover,
            ImageFit::Fill,
            ImageFit::None,
        ] {
            view.fit = fit;
            view.vertical = ImageAlign::Center;
            tree.reconcile(describe(&view)).unwrap();
            let frame = Frame {
                extent: Extent::new(12 * scale, 12 * scale),
                scale: scale as f64,
                background: Color::BLACK,
            };
            let pixels = renderer.render(&tree.scene().unwrap(), frame).unwrap();
            let at = |x, y| {
                let i = (y * scale) as usize * pixels.stride() + (x * scale * 4) as usize;
                &pixels.pixels()[i..i + 4]
            };
            assert_eq!(at(1, 6), &[0, 0, 0, 255]); // Padding is always clipped.
            assert_eq!(at(10, 6), &[0, 0, 0, 255]);
            assert_eq!(at(4, 6), &[255, 0, 0, 255]);
            assert_eq!(at(7, 6), &[0, 255, 0, 255]);
            let fills_height = matches!(fit, ImageFit::Cover | ImageFit::Fill);
            assert_eq!(
                at(4, 2),
                if fills_height {
                    &[255, 0, 0, 255]
                } else {
                    &[0, 0, 0, 255]
                }
            );
        }
        for (alignment, y) in [(ImageAlign::Start, 2), (ImageAlign::End, 8)] {
            view.fit = ImageFit::Contain;
            view.vertical = alignment;
            tree.reconcile(describe(&view)).unwrap();
            let pixels = renderer
                .render(
                    &tree.scene().unwrap(),
                    Frame {
                        extent: Extent::new(12 * scale, 12 * scale),
                        scale: scale as f64,
                        background: Color::BLACK,
                    },
                )
                .unwrap();
            let i = (y * scale) as usize * pixels.stride() + (3 * scale * 4) as usize;
            assert_eq!(&pixels.pixels()[i..i + 4], &[255, 0, 0, 255]);
        }
    }
    tree.reconcile(View::new(view).styled(LayoutStyle {
        width: Dimension::Length(0.0),
        height: Dimension::Length(0.0),
        ..Default::default()
    }))
    .unwrap();
    layout(&mut tree, Size::new(12.0, 12.0));
    assert!(tree.scene().unwrap().commands().is_empty());
}
#[cfg(feature = "cpu")]
#[test]
fn image_fits_alignment_padding_and_zero_size_cpu() {
    verify(&mut zui_render_cpu::CpuRenderer::default());
}
#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires a real GPU adapter; explicitly run --ignored"]
fn image_fits_alignment_padding_and_zero_size_gpu() {
    verify(&mut pollster::block_on(zui_render_gpu::GpuRenderer::new(Default::default())).unwrap());
}

#[test]
fn preferred_ratio_is_a_generic_contract_and_invalid_values_fail_layout() {
    use zui_api::{
        view::ViewSpec,
        widget::{PaintContext, Widget},
    };
    #[derive(Clone, Copy)]
    struct Ratio(f32);
    impl ViewSpec for Ratio {
        type Widget = Self;
        fn build(&self) -> Self {
            *self
        }
        fn update(&self, widget: &mut Self) -> Invalidation {
            *widget = *self;
            Invalidation::Layout
        }
    }
    impl Widget for Ratio {
        fn preferred_aspect_ratio(&self) -> Option<f32> {
            Some(self.0)
        }
        fn paint(&self, _: PaintContext, _: &mut zui_paint::Painter<'_>) {}
    }
    let describe = |ratio| {
        View::new(Ratio(ratio)).styled(LayoutStyle {
            width: Dimension::Length(80.0),
            ..Default::default()
        })
    };
    let mut tree = Tree::from_view(describe(2.0), Default::default()).unwrap();
    layout(&mut tree, Size::new(200.0, 200.0));
    assert_eq!(
        tree.layout_box(tree.root()).unwrap().bounds.size,
        Size::new(80.0, 40.0)
    );
    for invalid in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        tree.reconcile(describe(invalid)).unwrap();
        assert!(
            tree.layout(
                &mut zui_layout::TaffyLayout::default(),
                Size::new(200.0, 200.0)
            )
            .is_err()
        );
    }
    tree.reconcile(describe(4.0)).unwrap();
    layout(&mut tree, Size::new(200.0, 200.0));
    assert_eq!(
        tree.layout_box(tree.root()).unwrap().bounds.size,
        Size::new(80.0, 20.0)
    );
}
