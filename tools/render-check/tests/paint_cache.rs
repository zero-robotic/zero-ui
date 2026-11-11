use std::{cell::Cell, rc::Rc};
use zui_api::{
    layout::{Dimension, LayoutKind, LayoutStyle},
    view::{View, ViewSpec},
    widget::*,
};
use zui_core::Tree;
use zui_paint::Painter;
use zui_types::{Color, Rect, Size};

#[derive(Clone)]
struct Counted {
    calls: Rc<Cell<usize>>,
    color: Color,
}
impl ViewSpec for Counted {
    type Widget = Self;
    fn build(&self) -> Self {
        self.clone()
    }
    fn update(&self, widget: &mut Self) -> Invalidation {
        let changed = self.color != widget.color;
        *widget = self.clone();
        if changed {
            Invalidation::Paint
        } else {
            Invalidation::None
        }
    }
}
impl Widget for Counted {
    fn paint(&self, context: PaintContext, painter: &mut Painter<'_>) {
        self.calls.set(self.calls.get() + 1);
        painter.fill_rect(
            Rect::new(0.0, 0.0, context.size.width, context.size.height),
            self.color,
        );
    }
}

#[test]
fn clean_subtrees_skip_callbacks_and_global_invalidation_and_budgets_rebuild() {
    let calls = Rc::new(Cell::new(0));
    let make = |changed| {
        View::new(Counted {
            calls: calls.clone(),
            color: Color::BLACK,
        })
        .styled(LayoutStyle {
            kind: LayoutKind::Row,
            ..Default::default()
        })
        .with_children((0..50).map(|index| {
            View::new(Counted {
                calls: calls.clone(),
                color: if changed && index == 20 {
                    Color::BLACK
                } else {
                    Color::WHITE
                },
            })
            .keyed(index)
            .styled(LayoutStyle {
                width: Dimension::Length(5.0),
                height: Dimension::Length(20.0),
                ..Default::default()
            })
        }))
    };
    let mut tree = Tree::from_view(make(false), Default::default()).unwrap();
    let mut layout = zui_layout::TaffyLayout::default();
    tree.layout(&mut layout, Size::new(400.0, 100.0)).unwrap();
    let original = tree.scene().unwrap();
    assert_eq!(calls.get(), 51);
    assert!(original.same_commands(&tree.scene().unwrap()));
    assert_eq!(calls.get(), 51);
    tree.reconcile(make(true)).unwrap();
    let changed = tree.scene().unwrap();
    assert_eq!(tree.paint_cache_stats().painted_nodes, 2);
    assert_eq!(tree.paint_cache_stats().reused_subtrees, 49);
    assert_ne!(original.commands(), changed.commands());
    let leaf = tree.children(tree.root()).unwrap()[20];
    tree.update_widget::<Counted>(leaf, Invalidation::Layout, |widget| {
        widget.color = Color::WHITE
    })
    .unwrap();
    tree.layout(&mut layout, Size::new(400.0, 100.0)).unwrap();
    assert!(original.same_commands(&tree.scene().unwrap()));
    assert_eq!(tree.paint_cache_stats().painted_nodes, 2);
    tree.reconcile(make(true)).unwrap();
    assert!(changed.same_commands(&tree.scene().unwrap()));
    tree.invalidate(Invalidation::Paint);
    assert!(changed.same_commands(&tree.scene().unwrap()));
    assert_eq!(tree.paint_cache_stats().painted_nodes, 51);
    tree.set_paint_cache_budget(0);
    for _ in 0..2 {
        assert!(changed.same_commands(&tree.scene().unwrap()));
        assert_eq!(tree.paint_cache_stats().retained_bytes, 0);
        assert_eq!(tree.paint_cache_stats().painted_nodes, 51);
    }
}

#[test]
fn changing_owned_children_and_removing_nodes_cannot_reuse_old_content() {
    let calls = Rc::new(Cell::new(0));
    let node = |color| {
        View::new(Counted {
            calls: calls.clone(),
            color,
        })
        .styled(LayoutStyle {
            width: Dimension::Length(20.0),
            height: Dimension::Length(20.0),
            ..Default::default()
        })
    };
    let root = View::new(Counted {
        calls: calls.clone(),
        color: Color::BLACK,
    })
    .styled(LayoutStyle {
        kind: LayoutKind::Row,
        ..Default::default()
    })
    .with_children([
        node(Color::WHITE).keyed(1u64),
        node(Color::rgb(40, 80, 120)).keyed(2u64),
    ]);
    let mut tree = Tree::from_view(root, Default::default()).unwrap();
    let mut layout = zui_layout::TaffyLayout::default();
    let viewport = Size::new(100.0, 50.0);
    tree.layout(&mut layout, viewport).unwrap();
    let original = tree.scene().unwrap();
    let children = tree.children(tree.root()).unwrap().to_vec();
    tree.reorder(tree.root(), &[children[1], children[0]])
        .unwrap();
    tree.layout(&mut layout, viewport).unwrap();
    let reordered = tree.scene().unwrap();
    assert!(!original.same_commands(&reordered));
    tree.set_paint_cache_budget(0);
    assert!(reordered.same_commands(&tree.scene().unwrap()));
    tree.set_paint_cache_budget(1024 * 1024);
    tree.scene().unwrap();
    tree.remove(children[0]).unwrap();
    tree.layout(&mut layout, viewport).unwrap();
    let removed = tree.scene().unwrap();
    tree.invalidate(Invalidation::Paint);
    assert!(removed.same_commands(&tree.scene().unwrap()));
    assert!(removed.commands().len() < reordered.commands().len());
}
