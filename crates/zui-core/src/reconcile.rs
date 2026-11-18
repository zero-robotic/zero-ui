use crate::{Tree, TreeError, TreeLimits, tree::Node};
use alloc::{
    collections::{BTreeMap, BTreeSet},
    vec::Vec,
};
use zui_api::{
    layout::{LayoutBox, LayoutKind},
    view::{ChildOwnership, Key, View},
    widget::{Invalidation, Lifecycle},
};
use zui_types::NodeId;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReconcileReport {
    pub created: usize,
    pub reused: usize,
    pub removed: usize,
    pub invalidation: Invalidation,
}

struct Plan {
    view: View,
    id: NodeId,
    reused: bool,
    children: Vec<Plan>,
    preserved: Vec<NodeId>,
}

impl Tree {
    pub fn from_view(view: View, limits: TreeLimits) -> Result<Self, TreeError> {
        let mut tree = Self {
            presentation_cache: Default::default(),
            paint_dirty: BTreeSet::new(),
            paint_cache: Default::default(),
            events: Default::default(),
            nodes: BTreeMap::new(),
            root: NodeId(0),
            next_id: 1,
            limits,
            dirty: Invalidation::Layout,
            viewport: None,
            text_epoch: None,
            focus_scopes: Vec::new(),
        };
        tree.reconcile(view)?;
        Ok(tree)
    }

    /// Reconcile the complete root description. Returned validation errors occur
    /// before any build/update/lifecycle callback or tree mutation. User callback
    /// panics are not transactionally recovered.
    pub fn reconcile(&mut self, view: View) -> Result<ReconcileReport, TreeError> {
        validate(&view, self.limits)?;
        let mut next_id = self.next_id;
        let old_root = self.nodes.contains_key(&self.root).then_some(self.root);
        let plan = plan(self, view, old_root, &mut next_id)?;
        validate_planned(self, core::slice::from_ref(&plan), 0)?;
        let mut report = ReconcileReport::default();
        let old_nodes = old_root.map(|root| self.subtree(root)).unwrap_or_default();
        remove_unretained(self, old_nodes, core::slice::from_ref(&plan), &mut report);
        self.root = plan.id;
        self.next_id = next_id;
        apply(self, plan, None, &mut report);
        self.prune_input();
        if report.created != 0 || report.removed != 0 {
            self.dirty = Invalidation::Layout;
        }
        report.invalidation = self.dirty;
        Ok(report)
    }

    /// Reconcile one subtree using the same identity and validation rules as a
    /// full update. Unrelated branches receive no update/lifecycle callbacks.
    /// The caller selects scope; later full/owner updates remain authoritative.
    pub fn reconcile_subtree(
        &mut self,
        root: NodeId,
        view: View,
    ) -> Result<ReconcileReport, TreeError> {
        let parent = self.get(root)?.parent;
        if parent.is_none() {
            return self.reconcile(view);
        }
        let parent = parent.unwrap();
        if let Some(key) = &view.key
            && self.nodes[&parent]
                .children
                .iter()
                .any(|id| *id != root && self.nodes[id].key.as_ref() == Some(key))
        {
            return Err(TreeError::DuplicateKey(key.clone()));
        }
        let old_nodes = self.subtree(root);
        let outside = self.nodes.len() - old_nodes.len();
        validate_forest(
            core::slice::from_ref(&view),
            self.depth(root),
            outside,
            self.limits,
        )?;
        let mut next_id = self.next_id;
        let plan = plan(self, view, Some(root), &mut next_id)?;
        validate_planned(self, core::slice::from_ref(&plan), outside)?;
        let replacement = plan.id;
        let mut report = ReconcileReport::default();
        remove_unretained(self, old_nodes, core::slice::from_ref(&plan), &mut report);
        if replacement != root {
            let siblings = &mut self
                .nodes
                .get_mut(&parent)
                .expect("validated parent")
                .children;
            *siblings
                .iter_mut()
                .find(|id| **id == root)
                .expect("attached subtree") = replacement;
            self.presentation_cache.invalidate();
            self.paint_dirty.insert(parent);
        }
        self.next_id = next_id;
        apply(self, plan, Some(parent), &mut report);
        self.prune_input();
        if report.created != 0 || report.removed != 0 {
            self.dirty = Invalidation::Layout;
        }
        report.invalidation = self.dirty;
        Ok(report)
    }

    /// Replace one parent's child descriptions, preserving the parent and all
    /// unrelated branches. Key/type matching and lifecycle rules match reconcile.
    /// The caller owns range selection; omitted children are unmounted, including
    /// focus/capture cleanup. A later full reconcile remains authoritative.
    /// Validation errors occur before callbacks or mutation; callback panics are
    /// not recovered. Reports count descendants only, but invalidation is global.
    pub fn reconcile_children(
        &mut self,
        parent: NodeId,
        views: Vec<View>,
    ) -> Result<ReconcileReport, TreeError> {
        let node = self.get(parent)?;
        if node.style.kind == LayoutKind::Leaf && !views.is_empty() {
            return Err(TreeError::LeafHasChildren);
        }
        let old_children = node.children.clone();
        let old_nodes: Vec<_> = old_children
            .iter()
            .flat_map(|&id| self.subtree(id))
            .collect();
        validate_forest(
            &views,
            self.depth(parent) + 1,
            self.nodes.len() - old_nodes.len(),
            self.limits,
        )?;
        let mut next_id = self.next_id;
        let plans = plan_children(self, views, &old_children, &mut next_id)?;
        validate_planned(self, &plans, self.nodes.len() - old_nodes.len())?;
        let order: Vec<_> = plans.iter().map(|plan| plan.id).collect();
        let mut report = ReconcileReport::default();
        remove_unretained(self, old_nodes, &plans, &mut report);
        if order != old_children {
            self.presentation_cache.invalidate();
            self.paint_dirty.insert(parent);
            self.dirty = Invalidation::Layout;
        }
        self.nodes
            .get_mut(&parent)
            .expect("validated parent")
            .children = order;
        self.next_id = next_id;
        for plan in plans {
            apply(self, plan, Some(parent), &mut report);
        }
        self.prune_input();
        if report.created != 0 || report.removed != 0 {
            self.dirty = Invalidation::Layout;
        }
        report.invalidation = self.dirty;
        Ok(report)
    }
}

fn validate(view: &View, limits: TreeLimits) -> Result<(), TreeError> {
    validate_forest(core::slice::from_ref(view), 1, 0, limits)
}

fn validate_keys(views: &[View]) -> Result<(), TreeError> {
    let mut keys = BTreeSet::new();
    for view in views {
        if let Some(key) = &view.key
            && !keys.insert(key)
        {
            return Err(TreeError::DuplicateKey(key.clone()));
        }
    }
    Ok(())
}

fn validate_forest(
    views: &[View],
    depth: usize,
    mut count: usize,
    limits: TreeLimits,
) -> Result<(), TreeError> {
    if views.len() > limits.max_nodes.saturating_sub(count) {
        return Err(TreeError::BudgetExceeded);
    }
    validate_keys(views)?;
    let mut pending: Vec<_> = views.iter().rev().map(|view| (view, depth)).collect();
    while let Some((view, depth)) = pending.pop() {
        count += 1;
        if count > limits.max_nodes
            || depth > limits.max_depth
            || view.children.len() > limits.max_nodes.saturating_sub(count + pending.len())
        {
            return Err(TreeError::BudgetExceeded);
        }
        if view.child_ownership == ChildOwnership::Widget && !view.children.is_empty() {
            return Err(TreeError::ConflictingChildOwnership);
        }
        if view.style.kind == LayoutKind::Leaf
            && (!view.children.is_empty() || view.child_ownership == ChildOwnership::Widget)
        {
            return Err(TreeError::LeafHasChildren);
        }
        validate_keys(&view.children)?;
        pending.extend(view.children.iter().rev().map(|child| (child, depth + 1)));
    }
    Ok(())
}

fn plan(
    tree: &Tree,
    mut view: View,
    candidate: Option<NodeId>,
    next_id: &mut u64,
) -> Result<Plan, TreeError> {
    let reuse = candidate.filter(|id| {
        let node = &tree.nodes[id];
        node.key == view.key && node.widget.as_ref().type_id() == view.widget_type()
    });
    let id = if let Some(id) = reuse {
        id
    } else {
        let id = NodeId(*next_id);
        *next_id = next_id.checked_add(1).ok_or(TreeError::IdentityExhausted)?;
        id
    };
    let old_children = reuse
        .map(|id| tree.nodes[&id].children.as_slice())
        .unwrap_or(&[]);
    let preserved = if view.child_ownership == ChildOwnership::Widget {
        old_children.to_vec()
    } else {
        Vec::new()
    };
    let children = plan_children(
        tree,
        core::mem::take(&mut view.children),
        old_children,
        next_id,
    )?;
    Ok(Plan {
        view,
        id,
        reused: reuse.is_some(),
        children,
        preserved,
    })
}

fn plan_children(
    tree: &Tree,
    views: Vec<View>,
    old_children: &[NodeId],
    next_id: &mut u64,
) -> Result<Vec<Plan>, TreeError> {
    let mut keyed: BTreeMap<&Key, NodeId> = old_children
        .iter()
        .filter_map(|id| tree.nodes[id].key.as_ref().map(|key| (key, *id)))
        .collect();
    let mut children = Vec::with_capacity(views.len());
    for (index, child) in views.into_iter().enumerate() {
        let candidate = if let Some(key) = &child.key {
            keyed.remove(key)
        } else {
            old_children
                .get(index)
                .copied()
                .filter(|id| tree.nodes[id].key.is_none())
        };
        children.push(plan(tree, child, candidate, next_id)?);
    }
    Ok(children)
}

// Declared views omit realized owned descendants; include those before callbacks.
fn validate_planned(tree: &Tree, plans: &[Plan], mut count: usize) -> Result<(), TreeError> {
    let mut pending: Vec<_> = plans.iter().collect();
    while let Some(plan) = pending.pop() {
        count = count.checked_add(1).ok_or(TreeError::BudgetExceeded)?;
        for &id in &plan.preserved {
            count = count
                .checked_add(tree.subtree(id).len())
                .ok_or(TreeError::BudgetExceeded)?;
        }
        if count > tree.limits.max_nodes {
            return Err(TreeError::BudgetExceeded);
        }
        pending.extend(&plan.children);
    }
    Ok(())
}

fn remove_unretained(
    tree: &mut Tree,
    old_nodes: Vec<NodeId>,
    plans: &[Plan],
    report: &mut ReconcileReport,
) {
    let mut retained = BTreeSet::new();
    let mut pending: Vec<_> = plans.iter().collect();
    while let Some(item) = pending.pop() {
        if item.reused {
            retained.insert(item.id);
        }
        for &id in &item.preserved {
            retained.extend(tree.subtree(id));
        }
        pending.extend(&item.children);
    }
    for id in old_nodes.into_iter().rev() {
        if !retained.contains(&id) {
            tree.paint_cache.remove(id);
            tree.detach_input(id);
            let mut node = tree.nodes.remove(&id).expect("planned node");
            node.widget.lifecycle(id, Lifecycle::Unmounted);
            report.removed += 1;
        }
    }
}

fn apply(tree: &mut Tree, plan: Plan, parent: Option<NodeId>, report: &mut ReconcileReport) {
    let Plan {
        view,
        id,
        reused,
        children,
        preserved,
    } = plan;
    let order: Vec<_> = if view.child_ownership == ChildOwnership::Widget {
        preserved
    } else {
        children.iter().map(|child| child.id).collect()
    };
    if reused {
        let node = tree.nodes.get_mut(&id).expect("retained node");
        let invalidation = view
            .update_widget(node.widget.as_mut())
            .expect("type checked during planning");
        tree.dirty = tree.dirty.max(invalidation);
        if invalidation != Invalidation::None {
            tree.presentation_cache.invalidate();
            tree.paint_dirty.insert(id);
        }
        if node.style != view.style
            || node.children != order
            || node.child_ownership != view.child_ownership
            || node.layout_tag != view.layout_tag
        {
            tree.presentation_cache.invalidate();
            tree.paint_dirty.insert(id);
            tree.dirty = Invalidation::Layout;
        }
        if node.clip != view.clip {
            tree.presentation_cache.invalidate();
            tree.paint_dirty.insert(id);
            tree.dirty = tree.dirty.max(Invalidation::Paint);
        }
        node.marker = view.marker;
        node.layout_tag = view.layout_tag;
        node.child_ownership = view.child_ownership;
        node.style = view.style;
        node.children = order;
        node.clip = view.clip;
        node.parent = parent;
        report.reused += 1;
    } else {
        let mut widget = view.build_widget();
        widget.lifecycle(id, Lifecycle::Mounted);
        tree.nodes.insert(
            id,
            Node {
                marker: view.marker,
                key: view.key,
                layout_cache_key: Default::default(),
                parent,
                children: order,
                child_ownership: view.child_ownership,
                layout_tag: view.layout_tag,
                style: view.style,
                clip: view.clip,
                widget,
                layout: LayoutBox::default(),
            },
        );
        tree.presentation_cache.invalidate();
        report.created += 1;
    }
    for child in children {
        apply(tree, child, Some(id), report);
    }
}
