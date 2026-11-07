use zui_api::{
    event::*,
    layout::{Dimension, Edges, LayoutStyle},
    view::{View, ViewSpec},
    widget::{Invalidation, PaintContext, Widget},
};
use zui_core::Tree;
use zui_paint::Painter;
use zui_types::{NodeId, Point, Rect, Size};
use zui_widgets::{Scroll, ScrollView, column};

#[derive(Clone, Copy)]
struct Focusable;
impl ViewSpec for Focusable {
    type Widget = Self;
    fn build(&self) -> Self {
        *self
    }
    fn update(&self, _: &mut Self) -> Invalidation {
        Invalidation::None
    }
}
impl Widget for Focusable {
    fn focusable(&self) -> bool {
        true
    }
    fn paint(&self, _: PaintContext, _: &mut Painter<'_>) {}
}
fn rows() -> View {
    column((0..10).map(|i| {
        View::new(Focusable).keyed(i as u64).styled(LayoutStyle {
            height: Dimension::Length(20.0),
            ..Default::default()
        })
    }))
    .style(|s| s.gap = Size::new(0.0, 10.0))
    .build()
}
fn view(height: f32) -> View {
    ScrollView::vertical(rows())
        .scrollbar(None)
        .style(|s| {
            s.width = Dimension::Length(100.0);
            s.height = Dimension::Length(height);
            s.padding = Edges::all(10.0);
        })
        .build()
}
fn layout(tree: &mut Tree) {
    tree.layout(
        &mut zui_layout::TaffyLayout::default(),
        Size::new(200.0, 400.0),
    )
    .unwrap();
}
fn offset(tree: &mut Tree, node: NodeId) -> f64 {
    let mut value = 0.0;
    tree.update_widget::<Scroll>(node, Invalidation::None, |s| value = s.offset().y)
        .unwrap();
    value
}
fn tab(tree: &mut Tree, shift: bool) {
    tree.dispatch_input(InputEvent::Key(KeyInput {
        key: Key::Tab,
        pressed: true,
        repeat: false,
        text: None,
        modifiers: Modifiers {
            shift,
            ..Default::default()
        },
    }))
    .unwrap();
}
fn row_ids(tree: &Tree, scroll: NodeId) -> Vec<NodeId> {
    let wrapper = tree.children(scroll).unwrap()[0];
    let column = tree.children(wrapper).unwrap()[0];
    tree.children(column).unwrap().to_vec()
}

#[test]
fn tab_and_reverse_tab_reveal_nearest_edge_without_overriding_user_scroll() {
    let mut tree = Tree::from_view(view(70.0), Default::default()).unwrap();
    layout(&mut tree);
    let root = tree.root();
    let rows = row_ids(&tree, root);
    tab(&mut tree, false);
    assert_eq!(tree.focused(), Some(root));
    tab(&mut tree, false);
    assert_eq!(tree.focused(), Some(rows[0]));
    tab(&mut tree, false);
    assert_eq!(tree.focused(), Some(rows[1]));
    assert_eq!(offset(&mut tree, root), 0.0);
    tab(&mut tree, false);
    assert_eq!(tree.focused(), Some(rows[2]));
    assert_eq!(offset(&mut tree, root), 30.0);
    assert_eq!(
        tree.hit_test(Point::new(15.0, 45.0)).unwrap().unwrap().id,
        rows[2]
    );
    tab(&mut tree, true);
    tab(&mut tree, true);
    assert_eq!(tree.focused(), Some(rows[0]));
    assert_eq!(offset(&mut tree, root), 0.0);
    // Wheel movement is intentional even if it hides the currently focused row.
    tree.dispatch_input(InputEvent::PointerMoved(Point::new(15.0, 15.0)))
        .unwrap();
    tree.dispatch_input(InputEvent::Scroll(ScrollDelta::Pixels { x: 0.0, y: -80.0 }))
        .unwrap();
    assert_eq!(offset(&mut tree, root), 80.0);
    tree.reconcile(view(80.0)).unwrap();
    layout(&mut tree);
    assert_eq!(offset(&mut tree, root), 80.0);
    assert_eq!(tree.focused(), Some(rows[0]));
    tab(&mut tree, false);
    assert_eq!(tree.focused(), Some(rows[1]));
    assert_eq!(offset(&mut tree, root), 30.0);
}

#[test]
fn explicit_reveal_composes_nested_scrollers_and_clips_inner_visible_area() {
    let spacer = || {
        zui_widgets::Container::empty()
            .style(|s| s.height = Dimension::Length(120.0))
            .build()
    };
    let outer = ScrollView::vertical(column([spacer(), view(70.0), spacer()]).build())
        .scrollbar(None)
        .style(|s| {
            s.width = Dimension::Length(100.0);
            s.height = Dimension::Length(100.0);
        })
        .build();
    let mut tree = Tree::from_view(outer, Default::default()).unwrap();
    layout(&mut tree);
    let root = tree.root();
    let inner = row_ids(&tree, root)[1];
    let rows = row_ids(&tree, inner);
    assert!(
        tree.reveal_rect(rows[9], Rect::new(0.0, 0.0, 80.0, 20.0))
            .unwrap()
    );
    assert_eq!(offset(&mut tree, inner), 240.0);
    // Last row is now y=160..180 in outer content, so outer reveals its bottom.
    assert_eq!(offset(&mut tree, root), 80.0);
    assert_eq!(
        tree.hit_test(Point::new(15.0, 85.0)).unwrap().unwrap().id,
        rows[9]
    );
    assert!(
        !tree
            .reveal_rect(rows[9], Rect::new(0.0, 0.0, 80.0, 20.0))
            .unwrap()
    );
    assert!(tree.focused().is_none());
    assert!(
        tree.reveal_rect(rows[0], Rect::new(0.0, 0.0, 80.0, 20.0))
            .unwrap()
    );
    assert_eq!(offset(&mut tree, inner), 0.0);
    // It is already visible at y=50..70; nearest-edge reveal keeps outer still.
    assert_eq!(offset(&mut tree, root), 80.0);
    // Oversized targets settle on the nearest edge instead of alternating.
    tree.reveal_rect(inner, Rect::new(0.0, 0.0, 100.0, 200.0))
        .unwrap();
    let stable = offset(&mut tree, root);
    assert!(
        !tree
            .reveal_rect(inner, Rect::new(0.0, 0.0, 100.0, 200.0))
            .unwrap()
    );
    assert_eq!(offset(&mut tree, root), stable);
}

#[test]
fn programmatic_focus_reveals_mounted_rows_and_defers_until_layout_when_needed() {
    let mut tree = Tree::from_view(view(70.0), Default::default()).unwrap();
    let root = tree.root();
    let rows = row_ids(&tree, root);
    assert!(tree.request_focus(Some(rows[9])).unwrap());
    assert_eq!(tree.focused(), Some(rows[9]));
    layout(&mut tree);
    assert_eq!(offset(&mut tree, root), 240.0);
    assert_eq!(
        tree.hit_test(Point::new(15.0, 45.0)).unwrap().unwrap().id,
        rows[9]
    );
    assert!(tree.request_focus(Some(rows[0])).unwrap());
    assert_eq!(offset(&mut tree, root), 0.0);
    tree.dispatch_input(InputEvent::PointerMoved(Point::new(15.0, 15.0)))
        .unwrap();
    tree.dispatch_input(InputEvent::Scroll(ScrollDelta::Pixels { x: 0.0, y: -80.0 }))
        .unwrap();
    assert_eq!(offset(&mut tree, root), 80.0);
    assert!(!tree.request_focus(Some(rows[0])).unwrap());
    assert_eq!(offset(&mut tree, root), 80.0);
    assert!(tree.request_focus(None).unwrap());
    assert_eq!(offset(&mut tree, root), 80.0);
}
