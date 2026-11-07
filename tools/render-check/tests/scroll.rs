use zui_api::{
    action::ActionId,
    event::*,
    layout::{Dimension, Edges},
    view::View,
    widget::Invalidation,
};
use zui_core::Tree;
use zui_custom_widget_example::MeterView;
use zui_paint::{Frame, OffscreenRenderer};
use zui_types::{Color, Extent, Point, Size};
use zui_widgets::{Scroll, ScrollChanged, ScrollView, column};

fn rows(count: u64) -> View {
    column((0..count).map(|i| {
        let color = if i % 2 == 0 {
            Color::rgb(200, 0, 0)
        } else {
            Color::rgb(0, 180, 0)
        };
        View::new(MeterView::new(1.0, color, color).unwrap())
            .keyed(i)
            .styled(zui_api::layout::LayoutStyle {
                height: Dimension::Length(20.0),
                ..Default::default()
            })
    }))
    .style(|s| s.gap = Size::new(0.0, 2.0))
    .build()
}
fn view(count: u64, height: f32, request: Option<(f64, u64)>, disabled: bool) -> View {
    let mut scroll = ScrollView::vertical(rows(count))
        .scrollbar(None)
        .keyed("scroll")
        .on_scroll(ActionId(1))
        .disabled(disabled)
        .style(|s| {
            s.width = Dimension::Length(100.0);
            s.height = Dimension::Length(height);
            s.padding = Edges::all(10.0);
        });
    if let Some((offset, revision)) = request {
        scroll = scroll.scroll_to(Point::new(0.0, offset), revision);
    }
    scroll.build()
}
fn layout(tree: &mut Tree) -> bool {
    tree.layout(
        &mut zui_layout::TaffyLayout::default(),
        Size::new(200.0, 300.0),
    )
    .unwrap()
}
fn state(tree: &mut Tree, id: zui_types::NodeId) -> (f64, f64) {
    let mut result = (0.0, 0.0);
    tree.update_widget::<Scroll>(id, Invalidation::None, |s| {
        result = (s.offset().y, s.max_offset().y)
    })
    .unwrap();
    result
}
fn wheel(y: f64) -> InputEvent {
    InputEvent::Scroll(ScrollDelta::Pixels { x: 0.0, y })
}

#[test]
fn content_range_wheel_requests_resize_and_keyed_state_use_actual_layout() {
    let mut tree = Tree::from_view(view(8, 80.0, None, false), Default::default()).unwrap();
    layout(&mut tree);
    let root = tree.root();
    assert_eq!(
        tree.layout_box(root).unwrap().scroll_extent,
        Size::new(100.0, 194.0)
    );
    assert_eq!(state(&mut tree, root), (0.0, 114.0));
    let content = tree.children(tree.children(root).unwrap()[0]).unwrap()[0];
    let first_row = tree.children(content).unwrap()[0];
    assert_eq!(
        tree.hit_test(Point::new(15.0, 15.0)).unwrap().unwrap().id,
        first_row
    );
    tree.dispatch_input(InputEvent::PointerMoved(Point::new(15.0, 15.0)))
        .unwrap();
    let response = tree.dispatch_input(wheel(-30.0)).unwrap();
    assert!(response.handled);
    assert_eq!(
        response.actions[0]
            .action
            .payload::<ScrollChanged>()
            .unwrap()
            .offset
            .y,
        30.0
    );
    assert!(!layout(&mut tree));
    assert_eq!(state(&mut tree, root), (30.0, 114.0));
    assert_ne!(
        tree.hit_test(Point::new(15.0, 15.0)).unwrap().unwrap().id,
        first_row
    );
    tree.dispatch_input(InputEvent::Scroll(ScrollDelta::Lines { x: 0.0, y: -1.0 }))
        .unwrap();
    assert_eq!(state(&mut tree, root).0, 70.0);
    tree.reconcile(view(8, 80.0, None, false)).unwrap();
    layout(&mut tree);
    assert_eq!(state(&mut tree, root).0, 70.0);
    assert_eq!(tree.children(content).unwrap()[0], first_row);
    tree.reconcile(view(8, 80.0, Some((40.0, 1)), false))
        .unwrap();
    layout(&mut tree);
    assert_eq!(state(&mut tree, root).0, 40.0);
    tree.dispatch_input(wheel(-20.0)).unwrap();
    tree.reconcile(view(8, 80.0, Some((40.0, 1)), false))
        .unwrap();
    layout(&mut tree);
    assert_eq!(state(&mut tree, root).0, 60.0);
    tree.reconcile(view(8, 80.0, Some((40.0, 2)), false))
        .unwrap();
    layout(&mut tree);
    assert_eq!(state(&mut tree, root).0, 40.0);
    tree.reconcile(view(2, 80.0, Some((40.0, 2)), false))
        .unwrap();
    layout(&mut tree);
    assert_eq!(state(&mut tree, root), (0.0, 0.0));
    assert!(!tree.dispatch_input(wheel(-20.0)).unwrap().handled);
    tree.reconcile(view(8, 200.0, None, false)).unwrap();
    layout(&mut tree);
    assert_eq!(state(&mut tree, root), (0.0, 0.0));
}

#[test]
fn keyboard_disabled_and_nested_edge_bubbling() {
    let inner = view(8, 80.0, None, false);
    let outer = ScrollView::vertical(column([inner, rows(10)]).build())
        .style(|s| {
            s.width = Dimension::Length(100.0);
            s.height = Dimension::Length(100.0);
        })
        .build();
    let mut tree = Tree::from_view(outer, Default::default()).unwrap();
    layout(&mut tree);
    let root = tree.root();
    let content = tree.children(tree.children(root).unwrap()[0]).unwrap()[0];
    let inner = tree.children(content).unwrap()[0];
    tree.dispatch_input(InputEvent::PointerMoved(Point::new(15.0, 15.0)))
        .unwrap();
    tree.dispatch_input(wheel(-999.0)).unwrap();
    assert_eq!(state(&mut tree, inner).0, 114.0);
    assert_eq!(state(&mut tree, root).0, 0.0);
    tree.dispatch_input(wheel(-20.0)).unwrap();
    assert_eq!(state(&mut tree, root).0, 20.0);

    let mut tree = Tree::from_view(view(8, 80.0, None, false), Default::default()).unwrap();
    layout(&mut tree);
    let root = tree.root();
    // Padding belongs to the scroll container, so it can receive focus itself.
    tree.dispatch_input(InputEvent::PointerButton {
        button: PointerButton::Primary,
        pressed: true,
        position: Some(Point::new(5.0, 5.0)),
    })
    .unwrap();
    assert_eq!(tree.focused(), Some(root));
    let key = |key| {
        InputEvent::Key(KeyInput {
            key,
            pressed: true,
            repeat: false,
            text: None,
            modifiers: Default::default(),
        })
    };
    tree.dispatch_input(key(Key::PageDown)).unwrap();
    assert_eq!(state(&mut tree, root).0, 60.0);
    tree.dispatch_input(key(Key::End)).unwrap();
    assert_eq!(state(&mut tree, root).0, 114.0);
    tree.dispatch_input(key(Key::Home)).unwrap();
    assert_eq!(state(&mut tree, root).0, 0.0);
    tree.reconcile(view(8, 80.0, None, true)).unwrap();
    layout(&mut tree);
    assert!(tree.focused().is_none());
    assert!(!tree.dispatch_input(wheel(-20.0)).unwrap().handled);
    assert_eq!(state(&mut tree, root).0, 0.0);
}

#[test]
fn wrapping_text_changes_range_without_an_explicit_content_height_and_invalid_config_recovers() {
    use std::sync::Arc;
    use zui_theme::{Theme, TypeRole};
    use zui_widgets::TextView;
    let mut text = zui_text::ParleyText::default();
    let families = text
        .register_font(Arc::from(
            include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
        ))
        .unwrap();
    let make = |width, step| {
        ScrollView::vertical(View::new(TextView::themed(
            &Theme::dashboard(),
            "滚动内容由文字测量得到。改变视口宽度后，文字重新换行，内容高度和可滚动范围随之更新。"
                .repeat(5),
            families.clone(),
            TypeRole::Body,
        )))
        .line_step(step)
        .style(|s| {
            s.width = Dimension::Length(width);
            s.height = Dimension::Length(80.0);
        })
        .build()
    };
    let mut tree = Tree::from_view(make(160.0, 40.0), Default::default()).unwrap();
    let mut engine = zui_layout::TaffyLayout::default();
    let viewport = Size::new(400.0, 100.0);
    tree.layout_with_text(&mut engine, &mut text, viewport)
        .unwrap();
    let root = tree.root();
    let narrow = state(&mut tree, root).1;
    assert!(narrow > 80.0);
    tree.reconcile(make(320.0, 40.0)).unwrap();
    tree.layout_with_text(&mut engine, &mut text, viewport)
        .unwrap();
    let wide = state(&mut tree, root).1;
    assert!(wide > 0.0 && wide < narrow);
    for step in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        tree.reconcile(make(320.0, step)).unwrap();
        assert!(
            tree.layout_with_text(&mut engine, &mut text, viewport)
                .is_err()
        );
        assert!(tree.scene().is_err());
    }
    tree.reconcile(make(320.0, 40.0)).unwrap();
    tree.layout_with_text(&mut engine, &mut text, viewport)
        .unwrap();
    assert_eq!(state(&mut tree, root).1, wide);
    assert!(tree.scene().is_ok());
}

#[test]
fn requests_validate_clamp_and_zero_viewport_does_not_consume_wheel() {
    let mut tree =
        Tree::from_view(view(8, 80.0, Some((9999.0, 0)), false), Default::default()).unwrap();
    layout(&mut tree);
    let root = tree.root();
    assert_eq!(state(&mut tree, root).0, 114.0);
    for value in [-1.0, f64::NAN, f64::INFINITY] {
        tree.reconcile(view(8, 80.0, Some((value, 1)), false))
            .unwrap();
        assert!(
            tree.layout(
                &mut zui_layout::TaffyLayout::default(),
                Size::new(200.0, 300.0)
            )
            .is_err()
        );
    }
    tree.reconcile(view(8, 0.0, Some((0.0, 2)), false)).unwrap();
    layout(&mut tree);
    tree.dispatch_input(InputEvent::PointerMoved(Point::new(5.0, 5.0)))
        .unwrap();
    assert!(!tree.dispatch_input(wheel(-40.0)).unwrap().handled);
    assert_eq!(state(&mut tree, root).0, 0.0);
}

fn verify(renderer: &mut impl OffscreenRenderer) {
    for offset in [0.0, 30.0, 114.0] {
        let mut tree =
            Tree::from_view(view(8, 80.0, Some((offset, 1)), false), Default::default()).unwrap();
        layout(&mut tree);
        for scale in [1, 2] {
            let pixels = renderer
                .render(
                    &tree.scene().unwrap(),
                    Frame {
                        extent: Extent::new(100 * scale, 80 * scale),
                        scale: scale as f64,
                        background: Color::BLACK,
                    },
                )
                .unwrap();
            for y in 0..80 * scale as usize {
                for x in 0..100 * scale as usize {
                    let px = x / scale as usize;
                    let py = y / scale as usize;
                    let content_y = py as f64 - 10.0 + offset;
                    let index = (content_y / 22.0).floor() as usize;
                    let inside = (10..90).contains(&px)
                        && (10..70).contains(&py)
                        && content_y >= 0.0
                        && index < 8
                        && content_y % 22.0 < 20.0;
                    let color = if inside {
                        if index.is_multiple_of(2) {
                            Color::rgb(200, 0, 0)
                        } else {
                            Color::rgb(0, 180, 0)
                        }
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
fn scroll_content_clips_and_renders_cpu() {
    verify(&mut zui_render_cpu::CpuRenderer::default());
}
#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires a real GPU adapter; explicitly run --ignored"]
fn scroll_content_clips_and_renders_gpu() {
    verify(&mut pollster::block_on(zui_render_gpu::GpuRenderer::new(Default::default())).unwrap());
}
