use custom_widget::{Meter, MeterView};
use zui_api::{
    layout::{Dimension, LayoutStyle},
    view::View,
    widget::Invalidation,
};
use zui_core::{Tree, TreeLimits};
use zui_layout::TaffyLayout;
use zui_paint::{Frame, OffscreenRenderer};
use zui_types::{Color, Extent, Size};

fn verify(renderer: &mut impl OffscreenRenderer) {
    let description = |value| {
        View::new(MeterView::new(value, Color::WHITE, Color::BLACK).unwrap())
            .keyed("meter")
            .styled(LayoutStyle {
                width: Dimension::Length(100.0),
                height: Dimension::Length(20.0),
                ..Default::default()
            })
    };
    let mut tree = Tree::from_view(description(0.25), TreeLimits::default()).unwrap();
    let mut layout = TaffyLayout::default();
    let viewport = Size::new(100.0, 20.0);
    let frame = Frame {
        extent: Extent::new(100, 20),
        scale: 1.0,
        background: Color::BLACK,
    };
    tree.layout(&mut layout, viewport).unwrap();
    let before = renderer.render(&tree.scene().unwrap(), frame).unwrap();
    let root = tree.root();
    tree.update_widget::<Meter>(root, Invalidation::Paint, |meter| {
        meter.set_value(0.75).unwrap()
    })
    .unwrap();
    assert!(!tree.layout(&mut layout, viewport).unwrap());
    let after = renderer.render(&tree.scene().unwrap(), frame).unwrap();
    let report = tree.reconcile(description(0.50)).unwrap();
    assert_eq!((report.created, report.reused, report.removed), (0, 1, 0));
    assert_eq!(tree.root(), root);
    assert_eq!(report.invalidation, Invalidation::Paint);
    assert!(!tree.layout(&mut layout, viewport).unwrap());
    let declarative = renderer.render(&tree.scene().unwrap(), frame).unwrap();
    for (image, white_width) in [(before, 25), (after, 75), (declarative, 50)] {
        for y in 0..20 {
            for x in 0..100 {
                let offset = y * image.stride() + x * 4;
                let expected = if x < white_width {
                    [255, 255, 255, 255]
                } else {
                    [0, 0, 0, 255]
                };
                assert_eq!(&image.pixels()[offset..offset + 4], &expected);
            }
        }
    }
    assert!(Meter::new(f64::NAN, Color::WHITE, Color::BLACK).is_err());
}

#[cfg(feature = "cpu")]
#[test]
fn external_widget_updates_real_cpu_pixels_without_relayout() {
    verify(&mut zui_render_cpu::CpuRenderer::default());
}

#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires a real GPU adapter; explicitly run --ignored"]
fn external_widget_updates_real_gpu_pixels_without_relayout() {
    verify(&mut pollster::block_on(zui_render_gpu::GpuRenderer::new(Default::default())).unwrap());
}
