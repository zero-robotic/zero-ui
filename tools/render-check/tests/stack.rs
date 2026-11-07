use zui_api::{
    event::*,
    layout::{Align, Dimension, Edges, LayoutStyle},
    view::View,
};
use zui_core::Tree;
use zui_custom_widget_example::MeterView;
use zui_paint::{Frame, OffscreenRenderer};
use zui_types::{Color, Extent, Point, Rect, Size};
use zui_widgets::stack;
const BACKGROUND: Color = Color::rgb(20, 40, 60);
const BLUE: Color = Color::rgb(0, 60, 150);
const GREEN: Color = Color::rgb(0, 230, 220);
fn view(reverse: bool, clip: bool, overflow: bool) -> View {
    let blue = View::new(MeterView::new(1.0, BLUE, BLUE).unwrap())
        .keyed("blue")
        .styled(if overflow {
            LayoutStyle {
                width: Dimension::Length(140.0),
                height: Dimension::Length(20.0),
                ..Default::default()
            }
        } else {
            LayoutStyle::default()
        });
    let green = View::new(MeterView::new(1.0, GREEN, GREEN).unwrap())
        .keyed("green")
        .styled(LayoutStyle {
            width: Dimension::Length(40.0),
            height: Dimension::Length(20.0),
            align_self: Some(Align::Center),
            justify_self: Some(Align::Center),
            ..Default::default()
        });
    stack(if reverse {
        vec![green, blue]
    } else {
        vec![blue, green]
    })
    .keyed("stack")
    .background(BACKGROUND)
    .clipped(clip)
    .style(|style| {
        style.width = Dimension::Length(100.0);
        style.height = Dimension::Length(80.0);
        style.padding = Edges::all(10.0);
    })
    .build()
}
#[test]
fn keyed_layer_reordering_preserves_identity_and_changes_hit_and_event_target() {
    let mut tree = Tree::from_view(view(false, false, false), Default::default()).unwrap();
    let mut layout = zui_layout::TaffyLayout::default();
    tree.layout(&mut layout, Size::new(140.0, 100.0)).unwrap();
    let ids = tree.children(tree.root()).unwrap().to_vec();
    assert_eq!(
        tree.layout_box(ids[0]).unwrap().bounds,
        Rect::new(10.0, 10.0, 80.0, 60.0)
    );
    assert_eq!(
        tree.layout_box(ids[1]).unwrap().bounds,
        Rect::new(30.0, 30.0, 40.0, 20.0)
    );
    let hit = tree.hit_test(Point::new(40.0, 40.0)).unwrap().unwrap();
    assert_eq!(hit.id, ids[1]);
    assert_eq!(hit.local, Point::new(10.0, 10.0));
    let event = InputEvent::PointerButton {
        button: PointerButton::Primary,
        pressed: true,
        position: Some(Point::new(40.0, 40.0)),
    };
    assert_eq!(
        tree.dispatch_input(event.clone()).unwrap().target,
        Some(ids[1])
    );
    tree.reconcile(view(true, false, false)).unwrap();
    tree.layout(&mut layout, Size::new(140.0, 100.0)).unwrap();
    assert_eq!(tree.children(tree.root()).unwrap(), &[ids[1], ids[0]]);
    assert_eq!(
        tree.hit_test(Point::new(40.0, 40.0)).unwrap().unwrap().id,
        ids[0]
    );
    assert_eq!(tree.dispatch_input(event).unwrap().target, Some(ids[0]));
    assert!(tree.hit_test(Point::new(5.0, 5.0)).unwrap().is_none());
}
fn verify(renderer: &mut impl OffscreenRenderer) {
    let mut layout = zui_layout::TaffyLayout::default();
    for reverse in [false, true] {
        let mut tree = Tree::from_view(view(reverse, false, false), Default::default()).unwrap();
        tree.layout(&mut layout, Size::new(140.0, 100.0)).unwrap();
        let scene = tree.scene().unwrap();
        for scale in [1, 2] {
            let pixels = renderer
                .render(
                    &scene,
                    Frame {
                        extent: Extent::new(140 * scale, 100 * scale),
                        scale: scale as f64,
                        background: Color::BLACK,
                    },
                )
                .unwrap();
            for y in 0..100 * scale as usize {
                for x in 0..140 * scale as usize {
                    let px = x / scale as usize;
                    let py = y / scale as usize;
                    let expected = if !reverse && (30..70).contains(&px) && (30..50).contains(&py) {
                        GREEN
                    } else if (10..90).contains(&px) && (10..70).contains(&py) {
                        BLUE
                    } else if px < 100 && py < 80 {
                        BACKGROUND
                    } else {
                        Color::BLACK
                    };
                    let offset = y * pixels.stride() + x * 4;
                    assert_eq!(
                        &pixels.pixels()[offset..offset + 4],
                        &[expected.r, expected.g, expected.b, 255]
                    );
                }
            }
        }
    }
    for clip in [false, true] {
        let mut tree = Tree::from_view(view(false, clip, true), Default::default()).unwrap();
        tree.layout(&mut layout, Size::new(160.0, 100.0)).unwrap();
        let children = tree.children(tree.root()).unwrap();
        assert_eq!(
            tree.layout_box(children[1]).unwrap().bounds,
            Rect::new(30.0, 30.0, 40.0, 20.0)
        );
        assert_eq!(
            tree.hit_test(Point::new(110.0, 15.0)).unwrap().is_some(),
            !clip
        );
        let pixels = renderer
            .render(
                &tree.scene().unwrap(),
                Frame {
                    extent: Extent::new(160, 100),
                    scale: 1.0,
                    background: Color::BLACK,
                },
            )
            .unwrap();
        let offset = 15 * pixels.stride() + 110 * 4;
        let expected = if clip { Color::BLACK } else { BLUE };
        assert_eq!(
            &pixels.pixels()[offset..offset + 4],
            &[expected.r, expected.g, expected.b, 255]
        );
    }
}
#[cfg(feature = "cpu")]
#[test]
fn stack_paint_order_alignment_and_clipping_render_cpu() {
    verify(&mut zui_render_cpu::CpuRenderer::default());
}
#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires a real GPU adapter; explicitly run --ignored"]
fn stack_paint_order_alignment_and_clipping_render_gpu() {
    verify(&mut pollster::block_on(zui_render_gpu::GpuRenderer::new(Default::default())).unwrap());
}
