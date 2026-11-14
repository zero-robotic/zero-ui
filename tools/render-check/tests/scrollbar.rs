use custom_widget::MeterView;
use zui_api::{
    event::*,
    layout::{Dimension, Edges, LayoutStyle},
    view::View,
    widget::Invalidation,
};
use zui_core::Tree;
use zui_paint::{Frame, OffscreenRenderer};
use zui_types::{Color, Extent, Point, Size};
use zui_widgets::{Scroll, ScrollView, ScrollbarStyle, column};

fn style() -> ScrollbarStyle {
    ScrollbarStyle {
        track: Color::rgb(0, 0, 200),
        thumb: Color::rgb(0, 200, 0),
        dragging: Color::rgb(220, 220, 0),
        ..Default::default()
    }
}
fn view(offset: f64, height: f32, disabled: bool, bar: Option<ScrollbarStyle>) -> View {
    let content = column((0..10).map(|i| {
        View::new(MeterView::new(1.0, Color::rgb(200, 0, 0), Color::BLACK).unwrap())
            .keyed(i as u64)
            .styled(LayoutStyle {
                height: Dimension::Length(20.0),
                ..Default::default()
            })
    }))
    .build();
    ScrollView::vertical(content)
        .scroll_to(Point::new(0.0, offset), 0)
        .scrollbar(bar)
        .disabled(disabled)
        .style(|s| {
            s.width = Dimension::Length(120.0);
            s.height = Dimension::Length(height);
            s.padding = Edges::all(10.0);
        })
        .build()
}
fn layout(tree: &mut Tree) {
    tree.layout(
        &mut zui_layout::TaffyLayout::default(),
        Size::new(200.0, 200.0),
    )
    .unwrap();
}
fn offset(tree: &mut Tree) -> f64 {
    let mut value = 0.0;
    tree.update_widget::<Scroll>(tree.root(), Invalidation::None, |s| value = s.offset().y)
        .unwrap();
    value
}
fn button(pressed: bool, x: f64, y: f64) -> InputEvent {
    InputEvent::PointerButton {
        button: PointerButton::Primary,
        pressed,
        position: Some(Point::new(x, y)),
    }
}
#[test]
fn overlay_intercepts_children_and_thumb_capture_drags_outside_then_releases() {
    let mut tree =
        Tree::from_view(view(0.0, 120.0, false, Some(style())), Default::default()).unwrap();
    layout(&mut tree);
    let root = tree.root();
    assert_ne!(
        tree.hit_test(Point::new(100.0, 35.0)).unwrap().unwrap().id,
        root
    );
    assert_eq!(
        tree.hit_test(Point::new(105.0, 35.0)).unwrap().unwrap().id,
        root
    );
    let press = tree.dispatch_input(button(true, 105.0, 35.0)).unwrap();
    assert!(press.handled && press.actions.is_empty());
    assert_eq!(tree.pointer_capture(), Some(root));
    tree.dispatch_input(InputEvent::PointerMoved(Point::new(200.0, 60.0)))
        .unwrap();
    assert_eq!(offset(&mut tree), 50.0);
    tree.dispatch_input(button(false, 200.0, 200.0)).unwrap();
    assert_eq!(offset(&mut tree), 100.0);
    assert!(tree.pointer_capture().is_none());
    // Page once on the upper track; this is not a drag and does not capture.
    tree.dispatch_input(button(true, 105.0, 20.0)).unwrap();
    assert_eq!(offset(&mut tree), 0.0);
    assert!(tree.pointer_capture().is_none());
}
#[test]
fn resize_cancel_disable_hide_and_short_track_keep_gestures_bounded() {
    let mut tree =
        Tree::from_view(view(0.0, 120.0, false, Some(style())), Default::default()).unwrap();
    layout(&mut tree);
    tree.dispatch_input(button(true, 105.0, 35.0)).unwrap();
    tree.reconcile(view(0.0, 100.0, false, Some(style())))
        .unwrap();
    layout(&mut tree);
    // New thumb is 32 high on an 80 track: pointer at 50 => 24/48 * 120.
    tree.dispatch_input(InputEvent::PointerMoved(Point::new(105.0, 50.0)))
        .unwrap();
    assert!((offset(&mut tree) - 60.0).abs() < 1e-8);
    tree.cancel_input();
    assert!(tree.pointer_capture().is_none());
    tree.dispatch_input(InputEvent::PointerMoved(Point::new(105.0, 80.0)))
        .unwrap();
    assert_eq!(offset(&mut tree), 60.0);
    tree.reconcile(view(0.0, 100.0, true, Some(style())))
        .unwrap();
    layout(&mut tree);
    tree.dispatch_input(button(true, 105.0, 50.0)).unwrap();
    assert!(tree.pointer_capture().is_none());
    assert_eq!(offset(&mut tree), 60.0);
    tree.reconcile(view(0.0, 120.0, false, Some(style())))
        .unwrap();
    layout(&mut tree);
    tree.dispatch_input(button(true, 105.0, 65.0)).unwrap();
    assert!(tree.pointer_capture().is_some());
    tree.reconcile(view(0.0, 120.0, false, None)).unwrap();
    layout(&mut tree);
    tree.dispatch_input(InputEvent::PointerMoved(Point::new(105.0, 70.0)))
        .unwrap();
    assert!(tree.pointer_capture().is_none());
    let large_thumb = ScrollbarStyle {
        min_thumb: 200.0,
        ..style()
    };
    tree.reconcile(view(0.0, 40.0, false, Some(large_thumb)))
        .unwrap();
    layout(&mut tree);
    tree.dispatch_input(button(true, 105.0, 15.0)).unwrap();
    assert!(tree.pointer_capture().is_none());
    for invalid in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        tree.reconcile(view(
            0.0,
            120.0,
            false,
            Some(ScrollbarStyle {
                width: invalid,
                ..style()
            }),
        ))
        .unwrap();
        assert!(
            tree.layout(
                &mut zui_layout::TaffyLayout::default(),
                Size::new(200.0, 200.0)
            )
            .is_err()
        );
    }
    tree.reconcile(view(0.0, 240.0, false, Some(style())))
        .unwrap();
    layout(&mut tree);
    assert_ne!(
        tree.hit_test(Point::new(105.0, 35.0)).unwrap().unwrap().id,
        tree.root()
    );
}
fn verify(renderer: &mut impl OffscreenRenderer) {
    for offset in [0.0, 50.0, 100.0] {
        let mut tree = Tree::from_view(
            view(offset, 120.0, false, Some(style())),
            Default::default(),
        )
        .unwrap();
        layout(&mut tree);
        for scale in [1, 2] {
            let pixels = renderer
                .render(
                    &tree.scene().unwrap(),
                    Frame {
                        extent: Extent::new(120 * scale, 120 * scale),
                        scale: scale as f64,
                        background: Color::BLACK,
                    },
                )
                .unwrap();
            for y in 0..120 * scale as usize {
                for x in 0..120 * scale as usize {
                    let px = x / scale as usize;
                    let py = y / scale as usize;
                    let color = if (102..110).contains(&px) && (10..110).contains(&py) {
                        if (10 + offset as usize / 2..60 + offset as usize / 2).contains(&py) {
                            style().thumb
                        } else {
                            style().track
                        }
                    } else if (10..110).contains(&px) && (10..110).contains(&py) {
                        Color::rgb(200, 0, 0)
                    } else {
                        Color::BLACK
                    };
                    let i = y * pixels.stride() + x * 4;
                    assert_eq!(
                        &pixels.pixels()[i..i + 4],
                        &[color.r, color.g, color.b, 255],
                        "offset={offset}, pixel=({x},{y})"
                    );
                }
            }
        }
    }
}
#[cfg(feature = "cpu")]
#[test]
fn overlay_scrollbar_renders_above_content_cpu() {
    verify(&mut zui_render_cpu::CpuRenderer::default());
}
#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires a real GPU adapter; explicitly run --ignored"]
fn overlay_scrollbar_renders_above_content_gpu() {
    verify(&mut pollster::block_on(zui_render_gpu::GpuRenderer::new(Default::default())).unwrap());
}
