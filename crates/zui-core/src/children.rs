//! Layout-dependent child realization, independent of specific containers.
use crate::{Tree, TreeError, geometry::content_bounds};
use alloc::vec::Vec;
use zui_api::{
    view::ChildOwnership,
    widget::{ChildLayout, PaintContext},
};

impl Tree {
    pub(crate) fn notify_owned_children(&mut self) -> Result<(), TreeError> {
        for id in self.subtree(self.root) {
            let node = &self.nodes[&id];
            if node.child_ownership != ChildOwnership::Widget {
                continue;
            }
            let context = PaintContext {
                id,
                size: node.layout.bounds.size,
                content_bounds: content_bounds(node, node.layout.bounds),
            };
            let children: Vec<_> = node
                .children
                .iter()
                .map(|&id| {
                    let child = &self.nodes[&id];
                    ChildLayout {
                        id,
                        key: child.key.clone(),
                        tag: child.layout_tag,
                        layout: child.layout,
                    }
                })
                .collect();
            let invalidation = self
                .nodes
                .get_mut(&id)
                .expect("layout node")
                .widget
                .children_prepared(context, &children)?;
            self.dirty = self.dirty.max(invalidation);
        }
        Ok(())
    }

    pub(crate) fn refresh_owned_children(&mut self) -> Result<(), TreeError> {
        // Each pass uses the last published geometry. Batch existing owners so
        // independent siblings do not consume one layout pass each. Newly created
        // owners wait for layout; removed descendants are skipped.
        for id in self.subtree(self.root) {
            let Some(node) = self.nodes.get(&id) else {
                continue;
            };
            if node.child_ownership != ChildOwnership::Widget {
                continue;
            }
            let views = node.widget.child_views(PaintContext {
                id,
                size: node.layout.bounds.size,
                content_bounds: content_bounds(node, node.layout.bounds),
            })?;
            self.reconcile_children(id, views)?;
        }
        Ok(())
    }
}
