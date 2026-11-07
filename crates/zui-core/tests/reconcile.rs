use std::{cell::RefCell, rc::Rc};
use zui_api::{
    layout::*,
    view::{Key, View, ViewSpec},
    widget::*,
};
use zui_core::{Tree, TreeError, TreeLimits};
use zui_paint::Painter;
use zui_types::{Rect, Size};

#[derive(Default)]
struct Stats {
    builds: usize,
    updates: usize,
    lifecycle: Vec<(NodeId, Lifecycle)>,
}
type Shared = Rc<RefCell<Stats>>;
struct Stateful {
    stats: Shared,
    property: u8,
    local_state: u32,
}
impl Widget for Stateful {
    fn lifecycle(&mut self, id: NodeId, event: Lifecycle) {
        self.stats.borrow_mut().lifecycle.push((id, event));
    }
    fn paint(&self, _: PaintContext, _: &mut Painter<'_>) {}
}
struct Props {
    stats: Shared,
    value: u8,
}
impl ViewSpec for Props {
    type Widget = Stateful;
    fn build(&self) -> Stateful {
        self.stats.borrow_mut().builds += 1;
        Stateful {
            stats: self.stats.clone(),
            property: self.value,
            local_state: 0,
        }
    }
    fn update(&self, widget: &mut Stateful) -> Invalidation {
        self.stats.borrow_mut().updates += 1;
        if widget.property == self.value {
            return Invalidation::None;
        }
        widget.property = self.value;
        Invalidation::Paint
    }
}
fn leaf(stats: &Shared, value: u8) -> View {
    View::new(Props {
        stats: stats.clone(),
        value,
    })
}
fn keyed(stats: &Shared, key: u64) -> View {
    leaf(stats, key as u8).keyed(key)
}
fn parent(stats: &Shared, children: Vec<View>) -> View {
    leaf(stats, 0)
        .styled(LayoutStyle {
            kind: LayoutKind::Column,
            ..Default::default()
        })
        .with_children(children)
}
fn state(tree: &mut Tree, id: NodeId) -> (u8, u32) {
    let mut result = (0, 0);
    tree.update_widget::<Stateful>(id, Invalidation::None, |w| {
        result = (w.property, w.local_state)
    })
    .unwrap();
    result
}
struct Fixed;
impl LayoutEngine for Fixed {
    fn compute(
        &mut self,
        tree: &mut dyn LayoutTree,
        root: NodeId,
        _: Size,
    ) -> Result<(), LayoutError> {
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            stack.extend_from_slice(tree.node(id).unwrap().children);
            tree.set_layout(id, LayoutBox::new(Rect::new(0.0, 0.0, 10.0, 10.0)));
        }
        Ok(())
    }
}
fn clean(tree: &mut Tree) {
    tree.layout(&mut Fixed, Size::new(100.0, 100.0)).unwrap();
    tree.scene().unwrap();
}

#[test]
fn keyed_insert_reorder_remove_and_reinsert_preserve_only_matching_state() {
    let stats = Shared::default();
    let mut tree = Tree::from_view(
        parent(&stats, vec![keyed(&stats, 1), keyed(&stats, 2)]),
        TreeLimits::default(),
    )
    .unwrap();
    let root = tree.root();
    let original = tree.children(root).unwrap().to_vec();
    tree.update_widget::<Stateful>(original[0], Invalidation::None, |w| w.local_state = 42)
        .unwrap();
    let report = tree
        .reconcile(parent(
            &stats,
            vec![keyed(&stats, 2), keyed(&stats, 3), keyed(&stats, 1)],
        ))
        .unwrap();
    let reordered = tree.children(root).unwrap().to_vec();
    assert_eq!(reordered[0], original[1]);
    assert_eq!(reordered[2], original[0]);
    assert_eq!(state(&mut tree, original[0]), (1, 42));
    assert_eq!((report.created, report.reused, report.removed), (1, 3, 0));
    assert_eq!(stats.borrow().builds, 4);
    tree.reconcile(parent(&stats, vec![keyed(&stats, 2), keyed(&stats, 3)]))
        .unwrap();
    assert!(!tree.contains(original[0]));
    assert!(
        stats
            .borrow()
            .lifecycle
            .contains(&(original[0], Lifecycle::Unmounted))
    );
    tree.reconcile(parent(
        &stats,
        vec![keyed(&stats, 1), keyed(&stats, 2), keyed(&stats, 3)],
    ))
    .unwrap();
    let replacement = tree.children(root).unwrap()[0];
    assert_ne!(replacement, original[0]);
    assert_eq!(state(&mut tree, replacement), (1, 0));
}

#[test]
fn unkeyed_children_match_absolute_positions_in_a_mixed_list() {
    let stats = Shared::default();
    let mut tree = Tree::from_view(
        parent(
            &stats,
            vec![leaf(&stats, 1), keyed(&stats, 2), leaf(&stats, 3)],
        ),
        TreeLimits::default(),
    )
    .unwrap();
    let root = tree.root();
    let old = tree.children(root).unwrap().to_vec();
    tree.update_widget::<Stateful>(old[0], Invalidation::None, |w| w.local_state = 99)
        .unwrap();
    tree.reconcile(parent(
        &stats,
        vec![leaf(&stats, 9), leaf(&stats, 8), keyed(&stats, 2)],
    ))
    .unwrap();
    let new = tree.children(root).unwrap().to_vec();
    assert_eq!(new[0], old[0]);
    assert_eq!(state(&mut tree, new[0]), (9, 99));
    assert_eq!(new[2], old[1]);
    assert_ne!(new[1], old[1]);
    assert!(!tree.contains(old[2]));
}

struct Other(Shared);
impl Widget for Other {
    fn lifecycle(&mut self, id: NodeId, event: Lifecycle) {
        self.0.borrow_mut().lifecycle.push((id, event));
    }
    fn paint(&self, _: PaintContext, _: &mut Painter<'_>) {}
}
struct OtherProps(Shared);
impl ViewSpec for OtherProps {
    type Widget = Other;
    fn build(&self) -> Other {
        self.0.borrow_mut().builds += 1;
        Other(self.0.clone())
    }
    fn update(&self, _: &mut Other) -> Invalidation {
        Invalidation::None
    }
}

#[test]
fn a_type_change_replaces_the_entire_subtree_and_unmounts_children_first() {
    let stats = Shared::default();
    let mut tree = Tree::from_view(
        parent(
            &stats,
            vec![parent(&stats, vec![keyed(&stats, 7)]).keyed(1_u64)],
        ),
        TreeLimits::default(),
    )
    .unwrap();
    let root = tree.root();
    let old_parent = tree.children(root).unwrap()[0];
    let old_child = tree.children(old_parent).unwrap()[0];
    stats.borrow_mut().lifecycle.clear();
    let replacement = View::new(OtherProps(stats.clone()))
        .keyed(1_u64)
        .styled(LayoutStyle {
            kind: LayoutKind::Column,
            ..Default::default()
        })
        .with_children([keyed(&stats, 7)]);
    let report = tree.reconcile(parent(&stats, vec![replacement])).unwrap();
    assert_eq!((report.created, report.reused, report.removed), (2, 1, 2));
    let new_parent = tree.children(root).unwrap()[0];
    assert_ne!(new_parent, old_parent);
    assert_ne!(tree.children(new_parent).unwrap()[0], old_child);
    assert_eq!(
        &stats.borrow().lifecycle[..2],
        &[
            (old_child, Lifecycle::Unmounted),
            (old_parent, Lifecycle::Unmounted)
        ]
    );
}

#[test]
fn duplicate_keys_and_budget_errors_do_not_run_any_callbacks_or_mutate_tree() {
    let stats = Shared::default();
    let mut tree = Tree::from_view(
        parent(&stats, vec![keyed(&stats, 1)]),
        TreeLimits {
            max_nodes: 4,
            max_depth: 3,
        },
    )
    .unwrap();
    clean(&mut tree);
    let root = tree.root();
    let children = tree.children(root).unwrap().to_vec();
    let builds = stats.borrow().builds;
    let lifecycle = stats.borrow().lifecycle.clone();
    let duplicate = parent(
        &stats,
        vec![parent(&stats, vec![keyed(&stats, 3), keyed(&stats, 3)])],
    );
    assert_eq!(
        tree.reconcile(duplicate),
        Err(TreeError::DuplicateKey(Key::Number(3)))
    );
    assert_eq!(
        tree.reconcile(parent(&stats, (0..4).map(|id| keyed(&stats, id)).collect())),
        Err(TreeError::BudgetExceeded)
    );
    assert_eq!(
        tree.reconcile(leaf(&stats, 5).with_children([leaf(&stats, 6)])),
        Err(TreeError::LeafHasChildren)
    );
    assert_eq!(stats.borrow().updates, 0);
    assert_eq!(stats.borrow().builds, builds);
    assert_eq!(stats.borrow().lifecycle, lifecycle);
    assert_eq!(tree.children(root).unwrap(), children);
    assert_eq!(tree.invalidation(), Invalidation::None);
}

#[test]
fn no_op_and_property_only_updates_do_not_force_layout() {
    let stats = Shared::default();
    let mut tree = Tree::from_view(leaf(&stats, 1), TreeLimits::default()).unwrap();
    clean(&mut tree);
    let report = tree.reconcile(leaf(&stats, 1)).unwrap();
    assert_eq!(report.invalidation, Invalidation::None);
    assert_eq!(report.created, 0);
    let report = tree.reconcile(leaf(&stats, 2)).unwrap();
    assert_eq!(report.invalidation, Invalidation::Paint);
    assert!(!tree.layout(&mut Fixed, Size::new(100.0, 100.0)).unwrap());
    tree.scene().unwrap();
    let report = tree
        .reconcile(leaf(&stats, 2).styled(LayoutStyle {
            width: Dimension::Length(25.0),
            ..Default::default()
        }))
        .unwrap();
    assert_eq!(report.invalidation, Invalidation::Layout);
    assert_eq!(stats.borrow().builds, 1);
}

#[test]
fn keys_are_scoped_to_the_parent_and_cannot_collide_after_manual_reparent() {
    let stats = Shared::default();
    let mut tree = Tree::from_view(
        parent(
            &stats,
            vec![
                parent(&stats, vec![keyed(&stats, 9)]).keyed(1_u64),
                parent(&stats, vec![keyed(&stats, 9)]).keyed(2_u64),
            ],
        ),
        TreeLimits::default(),
    )
    .unwrap();
    let parents = tree.children(tree.root()).unwrap().to_vec();
    let a = tree.children(parents[0]).unwrap()[0];
    let b = tree.children(parents[1]).unwrap()[0];
    assert_ne!(a, b);
    assert_eq!(
        tree.reparent(a, parents[1], 0),
        Err(TreeError::DuplicateKey(Key::Number(9)))
    );
    tree.reconcile(parent(
        &stats,
        vec![
            parent(&stats, vec![]).keyed(1_u64),
            parent(&stats, vec![keyed(&stats, 9)]).keyed(2_u64),
        ],
    ))
    .unwrap();
    assert!(!tree.contains(a));
    assert_eq!(tree.children(parents[1]).unwrap(), &[b]);
}

#[test]
fn changing_root_key_replaces_identity_and_resets_state() {
    let stats = Shared::default();
    let mut tree = Tree::from_view(leaf(&stats, 1).keyed("first"), TreeLimits::default()).unwrap();
    let old = tree.root();
    let report = tree.reconcile(leaf(&stats, 1).keyed("second")).unwrap();
    assert_ne!(tree.root(), old);
    assert_eq!((report.created, report.reused, report.removed), (1, 0, 1));
    let new = tree.root();
    drop(tree);
    assert_eq!(
        stats.borrow().lifecycle,
        vec![
            (old, Lifecycle::Mounted),
            (old, Lifecycle::Unmounted),
            (new, Lifecycle::Mounted),
            (new, Lifecycle::Unmounted)
        ]
    );
}

#[test]
fn invalid_initial_description_never_mounts_a_widget() {
    let stats = Shared::default();
    assert!(matches!(
        Tree::from_view(
            parent(&stats, vec![keyed(&stats, 1), keyed(&stats, 1)]),
            TreeLimits::default()
        ),
        Err(TreeError::DuplicateKey(_))
    ));
    assert!(matches!(
        Tree::from_view(
            leaf(&stats, 1),
            TreeLimits {
                max_nodes: 0,
                max_depth: 1
            }
        ),
        Err(TreeError::BudgetExceeded)
    ));
    assert_eq!(stats.borrow().builds, 0);
    assert!(stats.borrow().lifecycle.is_empty());
}

#[test]
fn partial_reconcile_preserves_parent_siblings_and_overlapping_row_state() {
    let stats = Shared::default();
    let outside = Shared::default();
    let mut tree = Tree::from_view(
        parent(
            &outside,
            vec![
                parent(&outside, vec![keyed(&stats, 10), keyed(&stats, 11)]).keyed("rows"),
                keyed(&outside, 99),
            ],
        ),
        TreeLimits::default(),
    )
    .unwrap();
    let root = tree.root();
    let branch = tree.children(root).unwrap()[0];
    let sibling = tree.children(root).unwrap()[1];
    let old = tree.children(branch).unwrap().to_vec();
    tree.update_widget::<Stateful>(old[1], Invalidation::None, |w| w.local_state = 42)
        .unwrap();
    clean(&mut tree);
    let report = tree
        .reconcile_children(branch, vec![keyed(&stats, 11), keyed(&stats, 12)])
        .unwrap();
    assert_eq!((report.created, report.reused, report.removed), (1, 1, 1));
    assert_eq!(report.invalidation, Invalidation::Layout);
    assert_eq!(tree.children(root).unwrap(), &[branch, sibling]);
    assert_eq!(tree.children(branch).unwrap()[0], old[1]);
    assert_eq!(state(&mut tree, old[1]), (11, 42));
    assert!(!tree.contains(old[0]));
    assert_eq!(outside.borrow().updates, 0);
    assert_eq!(outside.borrow().lifecycle.len(), 3);
    clean(&mut tree);
    assert_eq!(
        tree.reconcile_children(branch, vec![keyed(&stats, 11), keyed(&stats, 12)])
            .unwrap()
            .invalidation,
        Invalidation::None
    );
    let last = tree.children(branch).unwrap()[1];
    let report = tree
        .reconcile_children(branch, vec![keyed(&stats, 12), keyed(&stats, 11)])
        .unwrap();
    assert_eq!(report.invalidation, Invalidation::Layout);
    assert_eq!(tree.children(branch).unwrap(), &[last, old[1]]);
    tree.reconcile_children(branch, vec![]).unwrap();
    assert_eq!(tree.len(), 3);
    assert_eq!(outside.borrow().updates, 0);
}

#[test]
fn partial_validation_accounts_for_external_nodes_and_absolute_depth_atomically() {
    let stats = Shared::default();
    let mut tree = Tree::from_view(
        parent(
            &stats,
            vec![parent(&stats, vec![keyed(&stats, 1)]), keyed(&stats, 9)],
        ),
        TreeLimits {
            max_nodes: 4,
            max_depth: 3,
        },
    )
    .unwrap();
    let branch = tree.children(tree.root()).unwrap()[0];
    // Replacing a full subtree uses the final total, not old + new nodes.
    tree.reconcile_children(branch, vec![keyed(&stats, 2)])
        .unwrap();
    clean(&mut tree);
    let old = tree.children(branch).unwrap().to_vec();
    let before = (
        stats.borrow().builds,
        stats.borrow().updates,
        stats.borrow().lifecycle.clone(),
    );
    assert_eq!(
        tree.reconcile_children(branch, vec![keyed(&stats, 3), keyed(&stats, 4)]),
        Err(TreeError::BudgetExceeded)
    );
    assert_eq!(
        tree.reconcile_children(
            branch,
            vec![parent(&stats, vec![]).with_children([keyed(&stats, 3)])]
        ),
        Err(TreeError::BudgetExceeded)
    );
    assert_eq!(
        tree.reconcile_children(old[0], vec![keyed(&stats, 3)]),
        Err(TreeError::LeafHasChildren)
    );
    assert_eq!(
        tree.reconcile_children(NodeId(999), vec![]),
        Err(TreeError::MissingNode(NodeId(999)))
    );
    assert_eq!(tree.children(branch).unwrap(), old);
    assert_eq!(tree.invalidation(), Invalidation::None);
    assert_eq!(
        (
            stats.borrow().builds,
            stats.borrow().updates,
            stats.borrow().lifecycle.clone()
        ),
        before
    );

    let mut tree = Tree::from_view(
        parent(&stats, vec![parent(&stats, vec![])]),
        TreeLimits {
            max_nodes: 10,
            max_depth: 3,
        },
    )
    .unwrap();
    let branch = tree.children(tree.root()).unwrap()[0];
    assert_eq!(
        tree.reconcile_children(branch, vec![parent(&stats, vec![keyed(&stats, 1)])]),
        Err(TreeError::BudgetExceeded)
    );
    assert_eq!(
        tree.reconcile_children(branch, vec![keyed(&stats, 1), keyed(&stats, 1)]),
        Err(TreeError::DuplicateKey(Key::Number(1)))
    );
    assert_eq!(
        tree.reconcile_children(
            branch,
            vec![leaf(&stats, 0).with_children([keyed(&stats, 3)])]
        ),
        Err(TreeError::LeafHasChildren)
    );
    assert_eq!(tree.len(), 2);
}
