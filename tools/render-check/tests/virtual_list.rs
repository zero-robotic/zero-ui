use std::{cell::Cell, rc::Rc};
use zui_api::{
    event::*,
    layout::{Dimension, Edges, LayoutError},
    view::{Key as ViewKey, View},
    widget::Invalidation,
};
use zui_core::{Tree, TreeError};
use zui_custom_widget_example::MeterView;
use zui_paint::{Frame, OffscreenRenderer};
use zui_types::{Color, Extent, Point, Size};
use zui_widgets::{ScrollAlignment, VirtualList, VirtualListView};

fn builder(count: usize, calls: Rc<Cell<usize>>) -> VirtualListView {
    VirtualListView::new(count, 20.0, move |i| {
        calls.set(calls.get() + 1);
        let color = if i.is_multiple_of(2) {
            Color::rgb(200, 0, 0)
        } else {
            Color::rgb(0, 180, 0)
        };
        View::new(MeterView::new(1.0, color, color).unwrap()).keyed(i as u64)
    })
    .keyed("list")
    .gap(2.0)
    .overscan(2)
    .scrollbar(None)
    .style(|s| {
        s.width = Dimension::Length(100.0);
        s.height = Dimension::Length(80.0);
        s.padding = Edges::all(10.0);
    })
}
fn layout(tree: &mut Tree) {
    tree.layout(
        &mut zui_layout::TaffyLayout::default(),
        Size::new(100.0, 80.0),
    )
    .unwrap();
}
fn state(tree: &mut Tree) -> (f64, f64) {
    let mut result = (0.0, 0.0);
    tree.update_widget::<VirtualList>(tree.root(), Invalidation::None, |list| {
        result = (list.offset(), list.max_offset())
    })
    .unwrap();
    result
}
#[test]
fn million_rows_realize_only_window_and_preserve_overlapping_keys() {
    let calls = Rc::new(Cell::new(0));
    let mut tree = Tree::from_view(
        builder(1_000_000, calls.clone()).build(),
        Default::default(),
    )
    .unwrap();
    layout(&mut tree);
    assert_eq!(tree.len(), 11); // owner + five slots + five actual rows
    assert!(calls.get() <= 15);
    assert_eq!(state(&mut tree), (0.0, 21_999_938.0));
    let root = tree.root();
    let original = tree.children(root).unwrap().to_vec();
    tree.reconcile(builder(1_000_000, calls.clone()).scroll_to(66.0, 1).build())
        .unwrap();
    layout(&mut tree);
    assert_eq!(tree.children(root).unwrap()[0], original[1]);
    assert!(!tree.contains(original[0]));
    assert!(tree.len() <= 15);
    let hit = tree.hit_test(Point::new(15.0, 15.0)).unwrap().unwrap().id;
    assert_eq!(tree.key(hit).unwrap(), Some(&ViewKey::Number(3)));
    tree.dispatch_input(InputEvent::PointerMoved(Point::new(15.0, 15.0)))
        .unwrap();
    tree.dispatch_input(InputEvent::Scroll(ScrollDelta::Pixels { x: 0.0, y: -7.0 }))
        .unwrap();
    layout(&mut tree);
    assert_eq!(state(&mut tree).0, 73.0);
    let before = calls.get();
    tree.reconcile(
        builder(1_000_000, calls.clone())
            .scroll_to_index(900_000, ScrollAlignment::Start, 2)
            .build(),
    )
    .unwrap();
    layout(&mut tree);
    assert_eq!(state(&mut tree).0, 19_800_000.0);
    assert!(calls.get() - before <= 21);
    assert!(tree.len() <= 15);
    for &id in tree.children(root).unwrap() {
        assert!(tree.layout_box(id).unwrap().bounds.origin.y < 200.0);
    }
    let hit = tree.hit_test(Point::new(15.0, 15.0)).unwrap().unwrap().id;
    assert_eq!(tree.key(hit).unwrap(), Some(&ViewKey::Number(900_000)));
    // A fixed request is not replayed on an ordinary root rebuild.
    tree.dispatch_input(InputEvent::PointerMoved(Point::new(15.0, 15.0)))
        .unwrap();
    tree.dispatch_input(InputEvent::Scroll(ScrollDelta::Pixels { x: 0.0, y: -7.0 }))
        .unwrap();
    layout(&mut tree);
    tree.reconcile(
        builder(1_000_000, calls)
            .scroll_to_index(900_000, ScrollAlignment::Start, 2)
            .build(),
    )
    .unwrap();
    layout(&mut tree);
    assert_eq!(state(&mut tree).0, 19_800_007.0);
}
#[test]
fn empty_resize_shrink_and_realization_budget_have_explicit_behavior() {
    let calls = Rc::new(Cell::new(0));
    let mut tree = Tree::from_view(
        builder(1_000_000, calls.clone())
            .scroll_to(f64::MAX, 1)
            .build(),
        Default::default(),
    )
    .unwrap();
    layout(&mut tree);
    assert_eq!(state(&mut tree).0, 21_999_938.0);
    tree.reconcile(builder(2, calls.clone()).build()).unwrap();
    layout(&mut tree);
    assert_eq!(state(&mut tree), (0.0, 0.0));
    assert_eq!(tree.len(), 5);
    tree.reconcile(builder(0, calls.clone()).build()).unwrap();
    layout(&mut tree);
    assert_eq!(tree.len(), 1);
    let before = calls.get();
    tree.reconcile(
        builder(1_000_000, calls.clone())
            .style(|s| s.height = Dimension::Length(20.0))
            .build(),
    )
    .unwrap();
    layout(&mut tree); // padding leaves a zero-height viewport
    assert_eq!(tree.len(), 1);
    assert_eq!(calls.get(), before);
    tree.reconcile(builder(1_000_000, calls.clone()).max_realized(2).build())
        .unwrap();
    assert!(matches!(
        tree.layout(
            &mut zui_layout::TaffyLayout::default(),
            Size::new(100.0, 80.0)
        ),
        Err(TreeError::Layout(LayoutError::BudgetExceeded))
    ));
    assert_eq!(calls.get(), before); // rejected before any factory calls
    tree.reconcile(builder(1_000_000, calls).build()).unwrap();
    layout(&mut tree);
    assert_eq!(tree.len(), 11);
}

fn verify(renderer: &mut impl OffscreenRenderer) {
    for offset in [0.0, 19_800_007.0, 21_999_938.0] {
        let mut tree = Tree::from_view(
            builder(1_000_000, Rc::default())
                .scroll_to(offset, 1)
                .build(),
            Default::default(),
        )
        .unwrap();
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
                    let logical = py as f64 - 10.0 + offset;
                    let index = (logical / 22.0).floor() as usize;
                    let inside = (10..90).contains(&px)
                        && (10..70).contains(&py)
                        && logical >= 0.0
                        && index < 1_000_000
                        && logical % 22.0 < 20.0;
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
fn virtual_rows_render_with_local_coordinates_cpu() {
    verify(&mut zui_render_cpu::CpuRenderer::default());
}
#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires a real GPU adapter; explicitly run --ignored"]
fn virtual_rows_render_with_local_coordinates_gpu() {
    verify(&mut pollster::block_on(zui_render_gpu::GpuRenderer::new(Default::default())).unwrap());
}

fn press(point: Point) -> InputEvent {
    InputEvent::PointerButton {
        button: PointerButton::Primary,
        pressed: true,
        position: Some(point),
    }
}
#[test]
fn list_scrollbar_capture_and_keyboard_share_logical_range() {
    let mut tree = Tree::from_view(
        builder(1_000_000, Rc::default())
            .scrollbar(Some(Default::default()))
            .build(),
        Default::default(),
    )
    .unwrap();
    layout(&mut tree);
    let root = tree.root();
    tree.dispatch_input(press(Point::new(86.0, 15.0))).unwrap();
    assert_eq!(tree.pointer_capture(), Some(root));
    assert_eq!(tree.focused(), Some(root));
    tree.dispatch_input(InputEvent::PointerMoved(Point::new(86.0, 100.0)))
        .unwrap();
    layout(&mut tree);
    assert_eq!(state(&mut tree).0, 21_999_938.0);
    assert!(tree.len() <= 15);
    tree.dispatch_input(InputEvent::PointerButton {
        button: PointerButton::Primary,
        pressed: false,
        position: Some(Point::new(86.0, 100.0)),
    })
    .unwrap();
    assert_eq!(tree.pointer_capture(), None);
    for (key, expected) in [
        (Key::Home, 0.0),
        (Key::PageDown, 60.0),
        (Key::End, 21_999_938.0),
    ] {
        tree.dispatch_input(InputEvent::Key(KeyInput {
            key,
            text: None,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        }))
        .unwrap();
        layout(&mut tree);
        assert_eq!(state(&mut tree).0, expected);
    }
}

#[derive(Clone)]
struct FocusRow;
impl zui_api::widget::Widget for FocusRow {
    fn focusable(&self) -> bool {
        true
    }
    fn paint(&self, _: zui_api::widget::PaintContext, _: &mut zui_paint::Painter<'_>) {}
    fn event(&mut self, context: EventContext, event: &WidgetEvent) -> EventResponse {
        if context.phase == EventPhase::Target
            && matches!(
                event,
                WidgetEvent::Input(InputEvent::PointerButton { pressed: true, .. })
            )
        {
            EventResponse {
                focus: FocusRequest::Focus,
                capture: CaptureRequest::Capture,
                ..Default::default()
            }
        } else {
            EventResponse::default()
        }
    }
}
impl zui_api::view::ViewSpec for FocusRow {
    type Widget = Self;
    fn build(&self) -> Self {
        self.clone()
    }
    fn update(&self, _: &mut Self) -> Invalidation {
        Invalidation::None
    }
}
#[test]
fn recycled_rows_cancel_input_and_invalid_row_keys_are_rejected() {
    let make = |offset| {
        VirtualListView::new(100, 20.0, |i| {
            View::new(FocusRow)
                .keyed(i as u64)
                .styled(zui_api::layout::LayoutStyle {
                    height: Dimension::Length(20.0),
                    ..Default::default()
                })
        })
        .scrollbar(None)
        .overscan(0)
        .scroll_to(offset, 1)
        .style(|s| {
            s.width = Dimension::Length(100.0);
            s.height = Dimension::Length(60.0);
        })
        .build()
    };
    let mut tree = Tree::from_view(make(0.0), Default::default()).unwrap();
    layout(&mut tree);
    tree.dispatch_input(press(Point::new(5.0, 5.0))).unwrap();
    let focused = tree.focused().unwrap();
    assert_eq!(tree.pointer_capture(), Some(focused));
    tree.reconcile(make(500.0)).unwrap();
    layout(&mut tree);
    assert!(!tree.contains(focused));
    assert_eq!(tree.focused(), None);
    assert_eq!(tree.pointer_capture(), None);

    let view = VirtualListView::new(10, 20.0, |_| View::new(FocusRow))
        .style(|s| s.height = Dimension::Length(60.0))
        .build();
    let mut tree = Tree::from_view(view, Default::default()).unwrap();
    assert!(matches!(
        tree.layout(
            &mut zui_layout::TaffyLayout::default(),
            Size::new(100.0, 80.0)
        ),
        Err(TreeError::Layout(LayoutError::InvalidStyle(_)))
    ));
    let view = VirtualListView::new(10, 20.0, |_| View::new(FocusRow).keyed("same"))
        .style(|s| s.height = Dimension::Length(60.0))
        .build();
    tree.reconcile(view).unwrap();
    assert!(matches!(
        tree.layout(
            &mut zui_layout::TaffyLayout::default(),
            Size::new(100.0, 80.0)
        ),
        Err(TreeError::DuplicateKey(_))
    ));
}

#[test]
fn application_can_focus_a_row_after_index_realization_without_reusing_old_ids() {
    let make = |index| {
        VirtualListView::new(1000, 20.0, |i| {
            View::new(FocusRow)
                .keyed(i as u64)
                .styled(zui_api::layout::LayoutStyle {
                    height: Dimension::Length(20.0),
                    ..Default::default()
                })
        })
        .scrollbar(None)
        .scroll_to_index(index, ScrollAlignment::Start, index as u64)
        .style(|s| {
            s.width = Dimension::Length(100.0);
            s.height = Dimension::Length(60.0);
        })
        .build()
    };
    let row_id = |tree: &Tree, index| {
        let slot = tree
            .children(tree.root())
            .unwrap()
            .iter()
            .copied()
            .find(|id| tree.key(*id).unwrap() == Some(&ViewKey::Number(index)))
            .unwrap();
        tree.children(slot).unwrap()[0]
    };
    let mut tree = Tree::from_view(make(0), Default::default()).unwrap();
    layout(&mut tree);
    let old = row_id(&tree, 0);
    assert!(tree.request_focus(Some(old)).unwrap());
    tree.reconcile(make(900)).unwrap();
    layout(&mut tree);
    assert!(!tree.contains(old));
    assert_eq!(tree.focused(), None);
    let target = row_id(&tree, 900);
    assert!(tree.request_focus(Some(target)).unwrap());
    assert_eq!(tree.focused(), Some(target));
    assert_eq!(
        tree.request_focus(Some(old)),
        Err(TreeError::MissingNode(old))
    );
    assert_eq!(tree.focused(), Some(target));
    assert_eq!(
        tree.hit_test(Point::new(5.0, 5.0)).unwrap().unwrap().id,
        target
    );
}
