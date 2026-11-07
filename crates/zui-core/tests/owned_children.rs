use std::{cell::Cell, rc::Rc};
use zui_api::{layout::*, view::*, widget::*};
use zui_core::{Tree, TreeError, TreeLimits};
use zui_paint::Painter;
use zui_types::{Rect, Size};

#[derive(Clone)]
struct Row;
impl Widget for Row {
    fn paint(&self, _: PaintContext, _: &mut Painter<'_>) {}
}
impl ViewSpec for Row {
    type Widget = Self;
    fn build(&self) -> Self {
        self.clone()
    }
    fn update(&self, _: &mut Self) -> Invalidation {
        Invalidation::None
    }
}
#[derive(Clone)]
struct Owner {
    first: u64,
    calls: Rc<Cell<usize>>,
    unstable: Rc<Cell<bool>>,
    invalid: Rc<Cell<bool>>,
}
impl Widget for Owner {
    fn paint(&self, _: PaintContext, _: &mut Painter<'_>) {}
    fn child_views(&self, context: PaintContext) -> Result<Vec<View>, LayoutError> {
        self.calls.set(self.calls.get() + 1);
        assert!(
            context.size.height > 0.0,
            "new owners must be laid out before querying"
        );
        if self.invalid.get() {
            return Ok(vec![
                View::new(Row).keyed(0_u64),
                View::new(Row).keyed(0_u64),
            ]);
        }
        let count = (context.size.height / 10.0) as usize;
        let first = if self.unstable.get() {
            self.calls.get() as u64
        } else {
            self.first
        };
        Ok((0..count)
            .map(|i| View::new(Row).keyed(first + i as u64))
            .collect())
    }
}
impl ViewSpec for Owner {
    type Widget = Self;
    fn build(&self) -> Self {
        self.clone()
    }
    fn update(&self, widget: &mut Self) -> Invalidation {
        let changed = self.first != widget.first;
        *widget = self.clone();
        if changed {
            Invalidation::Paint
        } else {
            Invalidation::None
        }
    }
}
fn owner() -> Owner {
    Owner {
        first: 0,
        calls: Rc::default(),
        unstable: Rc::default(),
        invalid: Rc::default(),
    }
}
fn container(spec: impl ViewSpec) -> View {
    View::new(spec).styled(LayoutStyle {
        kind: LayoutKind::Column,
        ..Default::default()
    })
}
fn owned(owner: &Owner) -> View {
    container(owner.clone()).managed_children()
}
struct Layout(usize);
impl LayoutEngine for Layout {
    fn compute(
        &mut self,
        tree: &mut dyn LayoutTree,
        root: NodeId,
        size: Size,
    ) -> Result<(), LayoutError> {
        self.0 += 1;
        let mut pending = vec![root];
        while let Some(id) = pending.pop() {
            pending.extend_from_slice(tree.node(id).unwrap().children);
            tree.set_layout(
                id,
                LayoutBox::new(Rect::new(0.0, 0.0, size.width, size.height)),
            );
        }
        Ok(())
    }
}

#[test]
fn realization_uses_viewport_and_survives_root_rebuild_and_paint_only_updates() {
    let mut props = owner();
    let mut tree = Tree::from_view(owned(&props), TreeLimits::default()).unwrap();
    let mut layout = Layout(0);
    assert_eq!(tree.len(), 1);
    tree.layout(&mut layout, Size::new(100.0, 30.0)).unwrap();
    assert_eq!(layout.0, 2);
    let root = tree.root();
    let rows = tree.children(root).unwrap().to_vec();
    assert_eq!(rows.len(), 3);
    tree.scene().unwrap();
    let report = tree.reconcile(owned(&props)).unwrap();
    assert_eq!((report.created, report.removed), (0, 0));
    assert_eq!(tree.children(root).unwrap(), rows);
    assert!(!tree.layout(&mut layout, Size::new(100.0, 30.0)).unwrap());
    props.first = 1;
    assert_eq!(
        tree.reconcile(owned(&props)).unwrap().invalidation,
        Invalidation::Paint
    );
    tree.layout(&mut layout, Size::new(100.0, 30.0)).unwrap();
    assert_eq!(&tree.children(root).unwrap()[..2], &rows[1..]);
    assert!(!tree.contains(rows[0]));
    tree.layout(&mut layout, Size::new(100.0, 10.0)).unwrap();
    assert_eq!(tree.children(root).unwrap(), &[rows[1]]);
    // Switching back to declarative children restores root description ownership.
    tree.reconcile(container(props).with_children([View::new(Row).keyed(99_u64)]))
        .unwrap();
    assert!(!tree.contains(rows[1]));
    assert_eq!(
        tree.key(tree.children(root).unwrap()[0]).unwrap(),
        Some(&Key::Number(99))
    );
}

#[test]
fn validation_errors_retain_children_block_paint_and_allow_retry() {
    let props = owner();
    let mut tree = Tree::from_view(owned(&props), TreeLimits::default()).unwrap();
    let mut layout = Layout(0);
    let size = Size::new(100.0, 20.0);
    tree.layout(&mut layout, size).unwrap();
    let root = tree.root();
    let rows = tree.children(root).unwrap().to_vec();
    props.invalid.set(true);
    tree.invalidate(Invalidation::Paint);
    assert_eq!(
        tree.layout(&mut layout, size),
        Err(TreeError::DuplicateKey(Key::Number(0)))
    );
    assert_eq!(tree.children(root).unwrap(), rows);
    assert!(matches!(tree.scene(), Err(TreeError::LayoutRequired)));
    props.invalid.set(false);
    tree.layout(&mut layout, size).unwrap();
    assert_eq!(tree.children(root).unwrap(), rows);
    assert!(tree.scene().is_ok());
}

#[test]
fn independent_owners_batch_and_nonconverging_owner_is_bounded() {
    let props = owner();
    let root = container(Row).with_children((0..100).map(|key| owned(&props).keyed(key as u64)));
    let mut tree = Tree::from_view(root, TreeLimits::default()).unwrap();
    let mut layout = Layout(0);
    let size = Size::new(100.0, 10.0);
    tree.layout(&mut layout, size).unwrap();
    assert_eq!(tree.len(), 201);
    assert_eq!(layout.0, 2);

    props.unstable.set(true);
    let mut tree = Tree::from_view(owned(&props), TreeLimits::default()).unwrap();
    let calls = props.calls.get();
    assert_eq!(
        tree.layout(&mut layout, size),
        Err(TreeError::ChildLayoutDidNotConverge)
    );
    assert_eq!(props.calls.get() - calls, 16);
    assert!(matches!(tree.scene(), Err(TreeError::LayoutRequired)));
    props.unstable.set(false);
    tree.layout(&mut layout, size).unwrap();
    assert!(tree.scene().is_ok());
}

#[test]
fn explicit_children_and_leaf_owners_are_rejected_and_realized_nodes_count_toward_budget() {
    let props = owner();
    assert!(matches!(
        Tree::from_view(
            owned(&props).with_children([View::new(Row)]),
            TreeLimits::default()
        ),
        Err(TreeError::ConflictingChildOwnership)
    ));
    assert!(matches!(
        Tree::from_view(
            View::new(props.clone()).managed_children(),
            TreeLimits::default()
        ),
        Err(TreeError::LeafHasChildren)
    ));
    let mut tree = Tree::from_view(
        container(Row).with_children([owned(&props).keyed(1_u64)]),
        TreeLimits {
            max_nodes: 4,
            max_depth: 4,
        },
    )
    .unwrap();
    let mut layout = Layout(0);
    tree.layout(&mut layout, Size::new(100.0, 20.0)).unwrap();
    assert_eq!(tree.len(), 4);
    let root = tree.root();
    let branch = tree.children(root).unwrap()[0];
    let rows = tree.children(branch).unwrap().to_vec();
    assert_eq!(
        tree.reconcile(container(Row).with_children([owned(&props).keyed(1_u64), View::new(Row)])),
        Err(TreeError::BudgetExceeded)
    );
    assert_eq!(tree.children(branch).unwrap(), rows);
    assert_eq!(tree.len(), 4);
}

#[derive(Clone)]
struct Nested {
    owner: Owner,
    show: bool,
}
impl Widget for Nested {
    fn paint(&self, _: PaintContext, _: &mut Painter<'_>) {}
    fn child_views(&self, _: PaintContext) -> Result<Vec<View>, LayoutError> {
        Ok(if self.show {
            vec![owned(&self.owner).keyed("nested")]
        } else {
            vec![]
        })
    }
}
impl ViewSpec for Nested {
    type Widget = Self;
    fn build(&self) -> Self {
        self.clone()
    }
    fn update(&self, widget: &mut Self) -> Invalidation {
        let changed = self.show != widget.show;
        *widget = self.clone();
        if changed {
            Invalidation::Paint
        } else {
            Invalidation::None
        }
    }
}

#[test]
fn new_nested_owner_waits_for_geometry_and_removed_owner_is_not_queried() {
    let props = owner();
    let mut nested = Nested {
        owner: props.clone(),
        show: true,
    };
    let mut tree = Tree::from_view(
        container(nested.clone()).managed_children(),
        TreeLimits::default(),
    )
    .unwrap();
    let mut layout = Layout(0);
    let size = Size::new(100.0, 20.0);
    tree.layout(&mut layout, size).unwrap();
    assert_eq!(tree.len(), 4);
    assert_eq!(layout.0, 3);
    let calls = props.calls.get();
    nested.show = false;
    tree.reconcile(container(nested).managed_children())
        .unwrap();
    tree.layout(&mut layout, size).unwrap();
    assert_eq!(tree.len(), 1);
    assert_eq!(props.calls.get(), calls);
}
