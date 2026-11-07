//! Immutable presentation coordinates, independent of layout and widget kinds.
use crate::{Tree, TreeError, geometry::content_bounds};
use alloc::collections::BTreeMap;
use zui_api::widget::{ChildViewport, PaintContext};
use zui_types::{NodeId, Point, Rect};

pub(crate) struct PresentedNode {
    pub origin: Point,
    pub children: ChildViewport,
}
pub(crate) type Presentation = BTreeMap<NodeId, PresentedNode>;

impl Tree {
    /// One immutable snapshot per operation, shared by every coordinate consumer.
    pub(crate) fn presentation(&self) -> Result<Presentation, TreeError> {
        let mut result = BTreeMap::new();
        let mut pending = alloc::vec![(self.root, Point::ZERO)];
        while let Some((id, parent_origin)) = pending.pop() {
            let node = &self.nodes[&id];
            let bounds = node.layout.bounds;
            let origin = Point::new(
                parent_origin.x + bounds.origin.x,
                parent_origin.y + bounds.origin.y,
            );
            let children = node.widget.child_viewport(PaintContext {
                id,
                size: bounds.size,
                content_bounds: content_bounds(node, bounds),
            });
            let child_origin = Point::new(
                origin.x + children.translation.x,
                origin.y + children.translation.y,
            );
            if !(Rect {
                origin,
                size: bounds.size,
            })
            .is_valid()
                || !children.translation.is_finite()
                || !child_origin.is_finite()
                || children.clip.is_some_and(|clip| {
                    !clip.is_valid()
                        || !Rect::new(
                            origin.x + clip.origin.x,
                            origin.y + clip.origin.y,
                            clip.size.width,
                            clip.size.height,
                        )
                        .is_valid()
                })
            {
                return Err(TreeError::InvalidChildViewport(id));
            }
            pending.extend(node.children.iter().map(|&child| (child, child_origin)));
            result.insert(id, PresentedNode { origin, children });
        }
        Ok(result)
    }
}
