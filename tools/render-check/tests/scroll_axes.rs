use zui_api::{
    event::*,
    layout::{Dimension, Edges, LayoutStyle, Track},
    view::View,
    widget::Invalidation,
};
use zui_core::Tree;
use zui_custom_widget_example::MeterView;
use zui_paint::{Frame, OffscreenRenderer};
use zui_types::{Color, Extent, Point, Rect, Size};
use zui_widgets::{Scroll, ScrollAxes, ScrollView, ScrollbarStyle, grid};
fn color(x: usize, y: usize) -> Color {
    if (x + y).is_multiple_of(2) {
        Color::rgb(200, 0, 0)
    } else {
        Color::rgb(0, 180, 0)
    }
}
fn content() -> View {
    grid(
        [Track::Length(50.0); 4],
        (0..16).map(|i| {
            View::new(MeterView::new(1.0, color(i % 4, i / 4), Color::BLACK).unwrap())
                .keyed(i as u64)
                .styled(LayoutStyle {
                    height: Dimension::Length(50.0),
                    ..Default::default()
                })
        }),
    )
    .build()
}
fn bar() -> ScrollbarStyle {
    ScrollbarStyle {
        track: Color::rgb(0, 0, 200),
        thumb: Color::rgb(220, 220, 0),
        ..Default::default()
    }
}
fn view(axes: ScrollAxes, request: Point, bars: bool) -> View {
    let builder = match axes {
        ScrollAxes::Vertical => ScrollView::vertical(content()),
        ScrollAxes::Horizontal => ScrollView::horizontal(content()),
        ScrollAxes::Both => ScrollView::both(content()),
    };
    builder
        .on_scroll(zui_api::action::ActionId(7))
        .scroll_to(request, 0)
        .scrollbar(bars.then_some(bar()))
        .style(|s| {
            s.width = Dimension::Length(120.0);
            s.height = Dimension::Length(120.0);
            s.padding = Edges::all(10.0);
        })
        .build()
}
fn layout(tree: &mut Tree) {
    tree.layout(
        &mut zui_layout::TaffyLayout::default(),
        Size::new(240.0, 240.0),
    )
    .unwrap();
}
fn state(tree: &mut Tree) -> (Point, Point) {
    let mut result = (Point::ZERO, Point::ZERO);
    tree.update_widget::<Scroll>(tree.root(), Invalidation::None, |s| {
        result = (s.offset(), s.max_offset())
    })
    .unwrap();
    result
}
fn button(pressed: bool, x: f64, y: f64) -> InputEvent {
    InputEvent::PointerButton {
        button: PointerButton::Primary,
        pressed,
        position: Some(Point::new(x, y)),
    }
}
fn key(key: Key) -> InputEvent {
    InputEvent::Key(KeyInput {
        key,
        pressed: true,
        repeat: false,
        text: None,
        modifiers: Default::default(),
    })
}

#[test]
fn horizontal_intrinsic_content_wheel_keyboard_and_axis_switch_share_state() {
    let mut tree = Tree::from_view(
        view(ScrollAxes::Horizontal, Point::ZERO, false),
        Default::default(),
    )
    .unwrap();
    layout(&mut tree);
    let root = tree.root();
    let wrapper = tree.children(root).unwrap()[0];
    assert_eq!(tree.layout_box(wrapper).unwrap().bounds.size.width, 200.0);
    assert_eq!(state(&mut tree), (Point::ZERO, Point::new(100.0, 0.0)));
    tree.dispatch_input(InputEvent::PointerMoved(Point::new(15.0, 15.0)))
        .unwrap();
    assert!(
        !tree
            .dispatch_input(InputEvent::Scroll(ScrollDelta::Pixels { x: 0.0, y: -20.0 }))
            .unwrap()
            .handled
    );
    tree.dispatch_input(InputEvent::Scroll(ScrollDelta::Lines { x: -1.0, y: 0.0 }))
        .unwrap();
    assert_eq!(state(&mut tree).0, Point::new(40.0, 0.0));
    tree.dispatch_input(button(true, 5.0, 5.0)).unwrap();
    tree.dispatch_input(key(Key::PageDown)).unwrap();
    assert_eq!(state(&mut tree).0, Point::new(100.0, 0.0));
    tree.dispatch_input(key(Key::ArrowLeft)).unwrap();
    assert_eq!(state(&mut tree).0, Point::new(60.0, 0.0));
    tree.reconcile(view(ScrollAxes::Both, Point::ZERO, false))
        .unwrap();
    layout(&mut tree);
    assert_eq!(tree.root(), root);
    assert_eq!(
        state(&mut tree),
        (Point::new(60.0, 0.0), Point::new(100.0, 100.0))
    );
    tree.dispatch_input(key(Key::End)).unwrap();
    assert_eq!(state(&mut tree).0, Point::new(100.0, 100.0));
    tree.reconcile(view(ScrollAxes::Vertical, Point::ZERO, false))
        .unwrap();
    layout(&mut tree);
    assert_eq!(
        state(&mut tree),
        (Point::new(0.0, 100.0), Point::new(0.0, 100.0))
    );
}
#[test]
fn dual_axis_bars_drag_independently_and_reveal_both_dimensions() {
    let mut tree = Tree::from_view(
        view(ScrollAxes::Both, Point::ZERO, true),
        Default::default(),
    )
    .unwrap();
    layout(&mut tree);
    let root = tree.root();
    let wrapper = tree.children(root).unwrap()[0];
    let grid = tree.children(wrapper).unwrap()[0];
    let last = tree.children(grid).unwrap()[15];
    tree.dispatch_input(button(true, 33.0, 105.0)).unwrap();
    assert_eq!(tree.pointer_capture(), Some(root));
    tree.dispatch_input(button(false, 56.0, 105.0)).unwrap();
    assert_eq!(state(&mut tree).0, Point::new(50.0, 0.0));
    tree.dispatch_input(button(true, 105.0, 33.0)).unwrap();
    tree.dispatch_input(button(false, 105.0, 56.0)).unwrap();
    assert_eq!(state(&mut tree).0, Point::new(50.0, 50.0));
    // Tracks exclude their shared corner; it remains ordinary content.
    assert_ne!(
        tree.hit_test(Point::new(106.0, 106.0)).unwrap().unwrap().id,
        root
    );
    tree.reveal_rect(last, Rect::new(0.0, 0.0, 50.0, 50.0))
        .unwrap();
    assert_eq!(state(&mut tree).0, Point::new(100.0, 100.0));
    assert_eq!(
        tree.hit_test(Point::new(75.0, 75.0)).unwrap().unwrap().id,
        last
    );
    assert!(
        !tree
            .reveal_rect(last, Rect::new(0.0, 0.0, 50.0, 50.0))
            .unwrap()
    );
    tree.dispatch_input(key(Key::Home)).unwrap();
    let result = tree
        .dispatch_input(InputEvent::Scroll(ScrollDelta::Pixels {
            x: -25.0,
            y: -40.0,
        }))
        .unwrap();
    assert_eq!(state(&mut tree).0, Point::new(25.0, 40.0));
    assert_eq!(result.actions.len(), 1);
    assert_eq!(
        result.actions[0]
            .action
            .payload::<zui_widgets::ScrollChanged>()
            .unwrap()
            .offset,
        Point::new(25.0, 40.0)
    );
}
#[test]
fn horizontal_text_uses_natural_width_instead_of_wrapping_to_the_viewport() {
    use std::sync::Arc;
    let mut text = zui_text::ParleyText::default();
    let families = text
        .register_font(Arc::from(
            include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
        ))
        .unwrap();
    let make = |axes| {
        let content = View::new(zui_widgets::TextView::themed(
            &zui_theme::Theme::dashboard(),
            "横向内容使用自然宽度，无需应用预先计算文字宽度。".repeat(4),
            families.clone(),
            zui_theme::TypeRole::Body,
        ));
        let scroll = match axes {
            ScrollAxes::Horizontal => ScrollView::horizontal(content),
            ScrollAxes::Vertical => ScrollView::vertical(content),
            ScrollAxes::Both => ScrollView::both(content),
        };
        scroll
            .style(|s| {
                s.width = Dimension::Length(160.0);
                s.height = Dimension::Length(80.0);
            })
            .build()
    };
    let mut tree = Tree::from_view(make(ScrollAxes::Horizontal), Default::default()).unwrap();
    tree.layout_with_text(
        &mut zui_layout::TaffyLayout::default(),
        &mut text,
        Size::new(200.0, 100.0),
    )
    .unwrap();
    assert!(state(&mut tree).1.x > 160.0);
    assert_eq!(state(&mut tree).1.y, 0.0);
    tree.reconcile(make(ScrollAxes::Both)).unwrap();
    tree.layout_with_text(
        &mut zui_layout::TaffyLayout::default(),
        &mut text,
        Size::new(200.0, 100.0),
    )
    .unwrap();
    assert!(state(&mut tree).1.x > 160.0);
    assert_eq!(state(&mut tree).1.y, 0.0);
    tree.reconcile(make(ScrollAxes::Vertical)).unwrap();
    tree.layout_with_text(
        &mut zui_layout::TaffyLayout::default(),
        &mut text,
        Size::new(200.0, 100.0),
    )
    .unwrap();
    assert_eq!(state(&mut tree).1.x, 0.0);
    assert!(state(&mut tree).1.y > 80.0);
}
fn verify(renderer: &mut impl OffscreenRenderer) {
    for offset in [0.0, 50.0, 100.0] {
        let mut tree = Tree::from_view(
            view(ScrollAxes::Both, Point::new(offset, offset), true),
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
                    let start = 10 + (offset * 0.46) as usize;
                    let expected = if (10..102).contains(&px) && (102..110).contains(&py) {
                        if (start..start + 46).contains(&px) {
                            bar().thumb
                        } else {
                            bar().track
                        }
                    } else if (102..110).contains(&px) && (10..102).contains(&py) {
                        if (start..start + 46).contains(&py) {
                            bar().thumb
                        } else {
                            bar().track
                        }
                    } else if (10..110).contains(&px) && (10..110).contains(&py) {
                        color(
                            (px - 10 + offset as usize) / 50,
                            (py - 10 + offset as usize) / 50,
                        )
                    } else {
                        Color::BLACK
                    };
                    let i = y * pixels.stride() + x * 4;
                    assert_eq!(
                        &pixels.pixels()[i..i + 4],
                        &[expected.r, expected.g, expected.b, 255],
                        "offset={offset}, pixel=({x},{y})"
                    );
                }
            }
        }
    }
}
#[cfg(feature = "cpu")]
#[test]
fn dual_axis_content_and_scrollbars_render_cpu() {
    verify(&mut zui_render_cpu::CpuRenderer::default());
}
#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires a real GPU adapter; explicitly run --ignored"]
fn dual_axis_content_and_scrollbars_render_gpu() {
    verify(&mut pollster::block_on(zui_render_gpu::GpuRenderer::new(Default::default())).unwrap());
}
