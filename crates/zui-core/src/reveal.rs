//! Descendant reveal requests travel inside-out through public widget contracts.
use crate::{Tree, TreeError, geometry::content_bounds};
use zui_api::widget::{Invalidation, PaintContext};
use zui_types::{NodeId, Rect};

fn translated(area: Rect, x: f64, y: f64, id: NodeId) -> Result<Rect, TreeError> {
    let result = Rect::new(
        area.origin.x + x,
        area.origin.y + y,
        area.size.width,
        area.size.height,
    );
    if result.is_valid() {
        Ok(result)
    } else {
        Err(TreeError::InvalidRevealArea(id))
    }
}
fn intersect(area: Rect, clip: Rect) -> Option<Rect> {
    if clip.size.width <= 0.0 || clip.size.height <= 0.0 {
        return None;
    }
    let left = area.origin.x.max(clip.origin.x);
    let top = area.origin.y.max(clip.origin.y);
    let right = (area.origin.x + area.size.width).min(clip.origin.x + clip.size.width);
    let bottom = (area.origin.y + area.size.height).min(clip.origin.y + clip.size.height);
    (right >= left && bottom > top).then(|| Rect::new(left, top, right - left, bottom - top))
}
impl Tree {
    /// Ask ancestors to reveal an area in the target's local border-box space.
    /// Does not change focus. Static clips can still make an area unreachable;
    /// the return value reports presentation changes, not guaranteed visibility.
    pub fn reveal_rect(&mut self, id: NodeId, mut area: Rect) -> Result<bool, TreeError> {
        if self.dirty == Invalidation::Layout {
            return Err(TreeError::LayoutRequired);
        }
        if !area.is_valid() {
            return Err(TreeError::InvalidRevealArea(id));
        }
        self.get(id)?;
        // Validate the committed presentation before invoking any callbacks.
        let presentation = self.presentation()?;
        let mut child = id;
        let mut changed = false;
        loop {
            let node = &self.nodes[&child];
            if node.clip {
                let Some(visible) = intersect(
                    area,
                    Rect::new(
                        0.0,
                        0.0,
                        node.layout.bounds.size.width,
                        node.layout.bounds.size.height,
                    ),
                ) else {
                    break;
                };
                area = visible;
            }
            let Some(parent) = node.parent else {
                break;
            };
            let position = node.layout.bounds.origin;
            let before = presentation[&parent].children;
            area = translated(
                area,
                position.x + before.translation.x,
                position.y + before.translation.y,
                id,
            )?;
            let parent_node = self.nodes.get_mut(&parent).expect("validated parent");
            let bounds = parent_node.layout.bounds;
            let context = PaintContext {
                id: parent,
                size: bounds.size,
                content_bounds: content_bounds(parent_node, bounds),
            };
            let invalidated = parent_node.widget.reveal_child(context, area);
            if invalidated {
                self.paint_dirty.insert(parent);
            }
            self.dirty = self.dirty.max(if invalidated {
                Invalidation::Paint
            } else {
                Invalidation::None
            });
            changed |= invalidated;
            let after = parent_node.widget.child_viewport(context);
            if !after.translation.is_finite() || after.clip.is_some_and(|clip| !clip.is_valid()) {
                return Err(TreeError::InvalidChildViewport(parent));
            }
            // The inner reveal moves the descendant before the next outer
            // receiver sees it. Only the visible inner portion travels outward.
            area = translated(
                area,
                after.translation.x - before.translation.x,
                after.translation.y - before.translation.y,
                id,
            )?;
            if let Some(clip) = after.clip {
                let Some(visible) = intersect(area, clip) else {
                    break;
                };
                area = visible;
            }
            child = parent;
        }
        // Reject overflow in accumulated global coordinates as for paint/hits.
        self.presentation()?;
        Ok(changed)
    }
    pub(crate) fn reveal_pending_focus(&mut self) -> Result<(), TreeError> {
        if self.dirty == Invalidation::Layout {
            return Ok(());
        }
        let Some(id) = self.events.reveal_focus else {
            return Ok(());
        };
        let bounds = self.get(id)?.layout.bounds;
        self.reveal_rect(
            id,
            Rect::new(0.0, 0.0, bounds.size.width, bounds.size.height),
        )?;
        self.events.reveal_focus = None;
        Ok(())
    }
}
