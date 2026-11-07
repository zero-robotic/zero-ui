use std::{cell::Cell, rc::Rc};
use zui_api::{
    layout::{Dimension, Edges, LayoutStyle},
    view::View,
    widget::Invalidation,
};
use zui_core::Tree;
use zui_paint::{Frame, OffscreenRenderer};
use zui_types::{Color, Extent, Point, Rect, Size};
use zui_widgets::CanvasView;
fn description(canvas: &CanvasView) -> View {
    View::new(canvas.clone()).styled(LayoutStyle {
        width: Dimension::Length(20.0),
        height: Dimension::Length(20.0),
        padding: Edges::all(4.0),
        ..Default::default()
    })
}
fn layout(tree: &mut Tree) {
    tree.layout(
        &mut zui_layout::TaffyLayout::default(),
        Size::new(30.0, 30.0),
    )
    .unwrap();
}
#[test]
fn callback_identity_revision_and_natural_size_have_distinct_invalidation() {
    let state = Rc::new(Cell::new(Color::WHITE));
    let captured = state.clone();
    let mut canvas = CanvasView::new(Size::new(12.0, 12.0), move |ctx, p| {
        p.fill_rect(ctx.content_bounds, captured.get())
    });
    let mut tree = Tree::from_view(description(&canvas), Default::default()).unwrap();
    layout(&mut tree);
    let root = tree.root();
    tree.scene().unwrap();
    tree.reconcile(description(&canvas)).unwrap();
    assert_eq!(tree.invalidation(), Invalidation::None);
    state.set(Color::rgb(255, 0, 0));
    canvas.revision += 1;
    tree.reconcile(description(&canvas)).unwrap();
    assert_eq!(tree.invalidation(), Invalidation::Paint);
    let scene = tree.scene().unwrap();
    assert!(scene.commands().iter().any(|command|matches!(command,zui_paint::Command::Fill{color,..} if *color==Color::rgb(255,0,0))));
    canvas = CanvasView::new(canvas.natural_size, |ctx, p| {
        p.fill_rect(ctx.content_bounds, Color::WHITE)
    });
    tree.reconcile(description(&canvas)).unwrap();
    assert_eq!(tree.invalidation(), Invalidation::Paint);
    tree.scene().unwrap();
    canvas.natural_size = Size::new(15.0, 10.0);
    tree.reconcile(description(&canvas)).unwrap();
    assert_eq!(tree.invalidation(), Invalidation::Layout);
    layout(&mut tree);
    assert_eq!(tree.root(), root);
    assert!(tree.hit_test(Point::new(5.0, 5.0)).unwrap().is_none());
    assert!(tree.request_focus(Some(root)).is_err());
    canvas.natural_size.width = f64::NAN;
    tree.reconcile(description(&canvas)).unwrap();
    assert!(
        tree.layout(
            &mut zui_layout::TaffyLayout::default(),
            Size::new(30.0, 30.0)
        )
        .is_err()
    );
    canvas.natural_size = Size::new(12.0, 12.0);
    tree.reconcile(description(&canvas)).unwrap();
    layout(&mut tree);
}
fn verify(renderer: &mut impl OffscreenRenderer) {
    let mut canvas = CanvasView::new(Size::new(12.0, 12.0), |ctx, p| {
        assert_eq!(ctx.content_bounds, Rect::new(4.0, 4.0, 12.0, 12.0));
        p.fill_rect(Rect::new(0.0, 0.0, 20.0, 20.0), Color::WHITE);
        p.fill_rect(Rect::new(8.0, 8.0, 4.0, 4.0), Color::rgb(255, 0, 0));
    });
    let mut tree = Tree::from_view(description(&canvas), Default::default()).unwrap();
    layout(&mut tree);
    for scale in [1, 2] {
        for clip in [true, false] {
            canvas.clip = clip;
            tree.reconcile(description(&canvas)).unwrap();
            let pixels = renderer
                .render(
                    &tree.scene().unwrap(),
                    Frame {
                        extent: Extent::new(30 * scale, 30 * scale),
                        scale: scale as f64,
                        background: Color::BLACK,
                    },
                )
                .unwrap();
            let at = |x, y| {
                let i = (y * scale) as usize * pixels.stride() + (x * scale * 4) as usize;
                &pixels.pixels()[i..i + 4]
            };
            assert_eq!(
                at(2, 2),
                if clip {
                    &[0, 0, 0, 255]
                } else {
                    &[255, 255, 255, 255]
                }
            );
            assert_eq!(at(5, 5), &[255, 255, 255, 255]);
            assert_eq!(at(9, 9), &[255, 0, 0, 255]);
            assert_eq!(at(22, 22), &[0, 0, 0, 255]);
        }
    }
    let zero = CanvasView::new(Size::ZERO, |_, _| panic!("zero canvas should not draw"));
    tree.reconcile(View::new(zero).styled(LayoutStyle {
        width: Dimension::Length(0.0),
        height: Dimension::Length(0.0),
        ..Default::default()
    }))
    .unwrap();
    layout(&mut tree);
    assert!(tree.scene().unwrap().commands().is_empty());
}
#[cfg(feature = "cpu")]
#[test]
fn canvas_callback_clips_and_observes_content_geometry_cpu() {
    verify(&mut zui_render_cpu::CpuRenderer::default());
}
#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires a real GPU adapter; explicitly run --ignored"]
fn canvas_callback_clips_and_observes_content_geometry_gpu() {
    verify(&mut pollster::block_on(zui_render_gpu::GpuRenderer::new(Default::default())).unwrap());
}

#[test]
fn natural_size_is_clamped_per_axis_without_an_implicit_aspect_ratio() {
    let canvas = CanvasView::new(Size::new(40.0, 20.0), |_, _| {});
    let mut tree = Tree::from_view(View::new(canvas), Default::default()).unwrap();
    for (viewport, expected) in [
        (Size::new(100.0, 100.0), Size::new(40.0, 20.0)),
        (Size::new(10.0, 10.0), Size::new(10.0, 10.0)),
    ] {
        tree.layout(&mut zui_layout::TaffyLayout::default(), viewport)
            .unwrap();
        assert_eq!(tree.layout_box(tree.root()).unwrap().bounds.size, expected);
    }
}
