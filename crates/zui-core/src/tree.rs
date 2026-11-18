use alloc::{
    boxed::Box,
    collections::{BTreeMap, BTreeSet},
    vec,
    vec::Vec,
};
use core::{any::Any, fmt};
use zui_api::view::{ChildOwnership, Key};
use zui_api::{
    layout::{LayoutBox, LayoutError, LayoutKind, LayoutStyle},
    widget::{Invalidation, Lifecycle, Widget},
};
use zui_types::{NodeId, Size};

pub(crate) struct Node {
    pub marker: Option<zui_api::view::ViewMarker>,
    pub key: Option<Key>,
    pub layout_cache_key: zui_api::layout::LayoutCacheKey,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    pub child_ownership: ChildOwnership,
    pub layout_tag: Option<zui_api::widget::LayoutTag>,
    pub style: LayoutStyle,
    pub widget: Box<dyn Widget>,
    pub layout: LayoutBox,
    pub clip: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct TreeLimits {
    pub max_nodes: usize,
    pub max_depth: usize,
}
impl Default for TreeLimits {
    fn default() -> Self {
        Self {
            max_nodes: 10_000,
            max_depth: 128,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TreeError {
    MissingNode(NodeId),
    NotFocusable(NodeId),
    RootMutation,
    LeafHasChildren,
    InvalidOrder,
    ConflictingChildOwnership,
    ChildLayoutDidNotConverge,
    Cycle,
    BudgetExceeded,
    IdentityExhausted,
    TypeMismatch(NodeId),
    LayoutRequired,
    Layout(LayoutError),
    IncompleteLayout,
    DuplicateKey(Key),
    InvalidInput,
    InvalidTextInputArea(NodeId),
    InvalidChildViewport(NodeId),
    InvalidRevealArea(NodeId),
    InvalidAnimationDeadline(NodeId),
    UnsupportedInput,
}
impl fmt::Display for TreeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "widget tree error: {self:?}")
    }
}
impl core::error::Error for TreeError {}
impl From<LayoutError> for TreeError {
    fn from(e: LayoutError) -> Self {
        Self::Layout(e)
    }
}

/// IDs are scoped to one tree and never reused during its lifetime. Topology can
/// only change through validated operations; callers cannot mutate child arrays.
pub struct Tree {
    pub(crate) presentation_cache: crate::presentation::PresentationCache,
    pub(crate) paint_dirty: BTreeSet<NodeId>,
    pub(crate) paint_cache: crate::paint_cache::PaintCache,
    pub(crate) events: crate::event::EventState,
    pub(crate) nodes: BTreeMap<NodeId, Node>,
    pub(crate) root: NodeId,
    pub(crate) next_id: u64,
    pub(crate) limits: TreeLimits,
    pub(crate) dirty: Invalidation,
    pub(crate) viewport: Option<Size>,
    pub(crate) text_epoch: Option<u64>,
    pub(crate) focus_scopes: Vec<(NodeId, Option<NodeId>)>,
}
impl Tree {
    pub fn new(
        widget: impl Widget,
        style: LayoutStyle,
        limits: TreeLimits,
    ) -> Result<Self, TreeError> {
        if limits.max_nodes == 0 || limits.max_depth == 0 {
            return Err(TreeError::BudgetExceeded);
        }
        let root = NodeId(1);
        let mut widget: Box<dyn Widget> = Box::new(widget);
        widget.lifecycle(root, Lifecycle::Mounted);
        let node = Node {
            marker: None,
            key: None,
            layout_cache_key: Default::default(),
            parent: None,
            children: Vec::new(),
            child_ownership: ChildOwnership::Declarative,
            layout_tag: None,
            style,
            widget,
            layout: LayoutBox::default(),
            clip: false,
        };
        Ok(Self {
            presentation_cache: Default::default(),
            paint_dirty: BTreeSet::new(),
            paint_cache: Default::default(),
            events: Default::default(),
            nodes: BTreeMap::from([(root, node)]),
            root,
            next_id: 2,
            limits,
            dirty: Invalidation::Layout,
            viewport: None,
            text_epoch: None,
            focus_scopes: Vec::new(),
        })
    }
    pub fn root(&self) -> NodeId {
        self.root
    }
    pub fn len(&self) -> usize {
        self.nodes.len()
    }
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
    pub fn contains(&self, id: NodeId) -> bool {
        self.nodes.contains_key(&id)
    }
    pub fn invalidation(&self) -> Invalidation {
        self.dirty
    }
    /// Use for external resource changes or replacing the injected layout service.
    pub fn invalidate(&mut self, invalidation: Invalidation) {
        if invalidation != Invalidation::None {
            self.presentation_cache.invalidate();
            self.paint_cache.clear();
        }
        if invalidation == Invalidation::Layout {
            for node in self.nodes.values_mut() {
                node.layout_cache_key = Default::default();
            }
        }
        self.dirty = self.dirty.max(invalidation);
    }
    pub fn set_paint_cache_budget(&mut self, bytes: usize) {
        self.paint_cache.clear();
        self.paint_cache.budget = bytes;
    }
    pub fn paint_cache_stats(&self) -> crate::PaintCacheStats {
        self.paint_cache.stats
    }
    pub fn children(&self, id: NodeId) -> Result<&[NodeId], TreeError> {
        Ok(&self.get(id)?.children)
    }
    pub fn parent(&self, id: NodeId) -> Result<Option<NodeId>, TreeError> {
        Ok(self.get(id)?.parent)
    }
    pub fn key(&self, id: NodeId) -> Result<Option<&Key>, TreeError> {
        Ok(self.get(id)?.key.as_ref())
    }
    /// Read an opaque upper-layer association without interpreting its meaning.
    pub fn view_marker(&self, id: NodeId) -> Result<Option<&zui_api::view::ViewMarker>, TreeError> {
        Ok(self.get(id)?.marker.as_ref())
    }
    pub fn style(&self, id: NodeId) -> Result<&LayoutStyle, TreeError> {
        Ok(&self.get(id)?.style)
    }
    pub(crate) fn get(&self, id: NodeId) -> Result<&Node, TreeError> {
        self.nodes.get(&id).ok_or(TreeError::MissingNode(id))
    }
    pub(crate) fn depth(&self, mut id: NodeId) -> usize {
        let mut depth = 1;
        while let Some(parent) = self.nodes[&id].parent {
            depth += 1;
            id = parent;
        }
        depth
    }
    pub(crate) fn subtree(&self, id: NodeId) -> Vec<NodeId> {
        let mut pending = vec![id];
        let mut result = Vec::new();
        while let Some(id) = pending.pop() {
            result.push(id);
            pending.extend(self.nodes[&id].children.iter().rev().copied());
        }
        result
    }
    pub fn insert(
        &mut self,
        parent: NodeId,
        index: usize,
        widget: impl Widget,
        style: LayoutStyle,
    ) -> Result<NodeId, TreeError> {
        let node = self.get(parent)?;
        if node.style.kind == LayoutKind::Leaf {
            return Err(TreeError::LeafHasChildren);
        }
        if index > node.children.len() {
            return Err(TreeError::InvalidOrder);
        }
        if self.nodes.len() >= self.limits.max_nodes || self.depth(parent) >= self.limits.max_depth
        {
            return Err(TreeError::BudgetExceeded);
        }
        let next = self
            .next_id
            .checked_add(1)
            .ok_or(TreeError::IdentityExhausted)?;
        let id = NodeId(self.next_id);
        let mut widget: Box<dyn Widget> = Box::new(widget);
        widget.lifecycle(id, Lifecycle::Mounted);
        self.nodes.insert(
            id,
            Node {
                marker: None,
                key: None,
                layout_cache_key: Default::default(),
                parent: Some(parent),
                children: Vec::new(),
                child_ownership: ChildOwnership::Declarative,
                layout_tag: None,
                style,
                widget,
                layout: LayoutBox::default(),
                clip: false,
            },
        );
        self.nodes
            .get_mut(&parent)
            .expect("validated parent")
            .children
            .insert(index, id);
        self.next_id = next;
        self.presentation_cache.invalidate();
        self.paint_dirty.insert(parent);
        self.dirty = Invalidation::Layout;
        Ok(id)
    }
    pub fn set_style(&mut self, id: NodeId, style: LayoutStyle) -> Result<(), TreeError> {
        let node = self.nodes.get_mut(&id).ok_or(TreeError::MissingNode(id))?;
        if style.kind == LayoutKind::Leaf
            && (!node.children.is_empty() || node.child_ownership == ChildOwnership::Widget)
        {
            return Err(TreeError::LeafHasChildren);
        }
        if node.style != style {
            self.presentation_cache.invalidate();
            self.paint_dirty.insert(id);
            node.style = style;
            self.dirty = Invalidation::Layout;
        }
        Ok(())
    }
    pub fn set_clip(&mut self, id: NodeId, clip: bool) -> Result<(), TreeError> {
        let node = self.nodes.get_mut(&id).ok_or(TreeError::MissingNode(id))?;
        if node.clip != clip {
            self.presentation_cache.invalidate();
            self.paint_dirty.insert(id);
            node.clip = clip;
            self.dirty = self.dirty.max(Invalidation::Paint);
        }
        Ok(())
    }
    /// Generic typed access is chosen by the caller; core has no concrete widget branches.
    /// The caller must declare whether its change affects measurement or only paint.
    pub fn update_widget<W: Widget>(
        &mut self,
        id: NodeId,
        invalidation: Invalidation,
        update: impl FnOnce(&mut W),
    ) -> Result<(), TreeError> {
        let node = self.nodes.get_mut(&id).ok_or(TreeError::MissingNode(id))?;
        let widget = (node.widget.as_mut() as &mut dyn Any)
            .downcast_mut::<W>()
            .ok_or(TreeError::TypeMismatch(id))?;
        self.dirty = self.dirty.max(invalidation);
        update(widget);
        if invalidation != Invalidation::None {
            self.presentation_cache.invalidate();
            self.paint_dirty.insert(id);
        }
        self.prune_input();
        Ok(())
    }
    pub fn reorder(&mut self, parent: NodeId, order: &[NodeId]) -> Result<(), TreeError> {
        let node = self.get(parent)?;
        let requested: BTreeSet<_> = order.iter().copied().collect();
        let existing: BTreeSet<_> = node.children.iter().copied().collect();
        if requested.len() != order.len() || requested != existing {
            return Err(TreeError::InvalidOrder);
        }
        if node.children != order {
            self.presentation_cache.invalidate();
            self.paint_dirty.insert(parent);
            self.nodes
                .get_mut(&parent)
                .expect("validated parent")
                .children = order.to_vec();
            self.dirty = Invalidation::Layout;
        }
        Ok(())
    }
    /// Move a whole subtree, preserving IDs and widget state. `index` addresses
    /// the destination child list after removing the source when parent is unchanged.
    pub fn reparent(&mut self, id: NodeId, parent: NodeId, index: usize) -> Result<(), TreeError> {
        let old = self.get(id)?.parent.ok_or(TreeError::RootMutation)?;
        let destination = self.get(parent)?;
        if let Some(key) = &self.get(id)?.key
            && destination
                .children
                .iter()
                .any(|child| *child != id && self.nodes[child].key.as_ref() == Some(key))
        {
            return Err(TreeError::DuplicateKey(key.clone()));
        }
        if destination.style.kind == LayoutKind::Leaf {
            return Err(TreeError::LeafHasChildren);
        }
        let subtree = self.subtree(id);
        if subtree.contains(&parent) {
            return Err(TreeError::Cycle);
        }
        if index > destination.children.len() - usize::from(old == parent) {
            return Err(TreeError::InvalidOrder);
        }
        let height = subtree
            .iter()
            .map(|&child| self.depth(child) - self.depth(id) + 1)
            .max()
            .unwrap_or(1);
        if self.depth(parent) + height > self.limits.max_depth {
            return Err(TreeError::BudgetExceeded);
        }
        self.nodes
            .get_mut(&old)
            .expect("validated parent")
            .children
            .retain(|&child| child != id);
        self.nodes
            .get_mut(&parent)
            .expect("validated parent")
            .children
            .insert(index, id);
        self.nodes.get_mut(&id).expect("validated node").parent = Some(parent);
        self.presentation_cache.invalidate();
        self.paint_dirty.insert(old);
        self.paint_dirty.insert(parent);
        self.dirty = Invalidation::Layout;
        Ok(())
    }
    pub fn remove(&mut self, id: NodeId) -> Result<(), TreeError> {
        let parent = self.get(id)?.parent.ok_or(TreeError::RootMutation)?;
        let subtree = self.subtree(id);
        self.presentation_cache.invalidate();
        self.paint_dirty.insert(parent);
        self.nodes
            .get_mut(&parent)
            .expect("validated parent")
            .children
            .retain(|&child| child != id);
        for id in subtree.into_iter().rev() {
            self.paint_cache.remove(id);
            self.detach_input(id);
            let mut node = self.nodes.remove(&id).expect("validated node");
            node.widget.lifecycle(id, Lifecycle::Unmounted);
        }
        self.dirty = Invalidation::Layout;
        Ok(())
    }
}
impl Drop for Tree {
    fn drop(&mut self) {
        if self.nodes.is_empty() {
            return;
        }
        for id in self.subtree(self.root).into_iter().rev() {
            self.detach_input(id);
            if let Some(mut node) = self.nodes.remove(&id) {
                node.widget.lifecycle(id, Lifecycle::Unmounted);
            }
        }
    }
}
