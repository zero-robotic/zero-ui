use zui_api::{
    layout::{Dimension, LayoutStyle},
    view::{View, ViewSpec},
    widget::Invalidation,
};
use zui_core::Tree;
use zui_paint::{Frame, OffscreenRenderer, Path};
use zui_types::{Color, Extent, Point, Rect, Size};
use zui_widgets::{Icon, IconKind, IconView};
fn spec(icon: Icon) -> IconView {
    IconView {
        icon,
        size: 24.0,
        color: Color::WHITE,
    }
}
#[test]
fn icon_validation_invalidation_and_passive_geometry() {
    let mut view = spec(Icon::builtin(IconKind::ChevronDown));
    let mut widget = view.build();
    assert_eq!(view.update(&mut widget), Invalidation::None);
    view.color = Color::BLACK;
    assert_eq!(view.update(&mut widget), Invalidation::Paint);
    view.icon = Icon::builtin(IconKind::Check);
    assert_eq!(view.update(&mut widget), Invalidation::Paint);
    view.size = 32.0;
    assert_eq!(view.update(&mut widget), Invalidation::Layout);
    let mut tree = Tree::from_view(View::new(view.clone()), Default::default()).unwrap();
    tree.layout(
        &mut zui_layout::TaffyLayout::default(),
        Size::new(64.0, 64.0),
    )
    .unwrap();
    assert!(tree.hit_test(Point::new(10.0, 10.0)).unwrap().is_none());
    assert!(tree.request_focus(Some(tree.root())).is_err());
    view.size = f64::NAN;
    tree.reconcile(View::new(view)).unwrap();
    assert!(
        tree.layout(
            &mut zui_layout::TaffyLayout::default(),
            Size::new(64.0, 64.0)
        )
        .is_err()
    );
    let mut path = Path::new();
    path.move_to(Point::new(f64::NAN, 0.0));
    assert!(Icon::custom(path, None).is_none());
    assert!(Icon::custom(Path::rectangle(Rect::new(0.0, 0.0, 24.0, 24.0)), Some(-1.0)).is_none());
}
fn verify(renderer: &mut impl OffscreenRenderer) {
    // No fonts are registered: both built-in and custom icons must render independently.
    for icon in [
        Icon::builtin(IconKind::Minus),
        Icon::custom(Path::rectangle(Rect::new(5.0, 11.0, 14.0, 2.0)), None).unwrap(),
    ] {
        let view = View::new(spec(icon)).styled(LayoutStyle {
            width: Dimension::Length(48.0),
            height: Dimension::Length(24.0),
            ..Default::default()
        });
        let mut tree = Tree::from_view(view, Default::default()).unwrap();
        tree.layout(
            &mut zui_layout::TaffyLayout::default(),
            Size::new(48.0, 24.0),
        )
        .unwrap();
        for scale in [1, 2] {
            let pixels = renderer
                .render(
                    &tree.scene().unwrap(),
                    Frame {
                        extent: Extent::new(48 * scale, 24 * scale),
                        scale: scale as f64,
                        background: Color::BLACK,
                    },
                )
                .unwrap();
            let at = |x: u32, y: u32| {
                let i = (y * scale) as usize * pixels.stride() + (x * scale * 4) as usize;
                &pixels.pixels()[i..i + 4]
            };
            assert_eq!(at(24, 12), &[255, 255, 255, 255]);
            assert_eq!(at(3, 12), &[0, 0, 0, 255]);
            assert_eq!(at(24, 3), &[0, 0, 0, 255]);
        }
    }
}
#[cfg(feature = "cpu")]
#[test]
fn icons_cpu() {
    verify(&mut zui_render_cpu::CpuRenderer::default());
}
#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires a GPU adapter"]
fn icons_gpu() {
    verify(&mut pollster::block_on(zui_render_gpu::GpuRenderer::new(Default::default())).unwrap());
}
