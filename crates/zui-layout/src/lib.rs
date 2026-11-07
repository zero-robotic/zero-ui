//! Taffy adapter for the portable ZUI layout contracts.
mod adapter;
mod style;

use std::collections::BTreeMap;
use zui_api::layout::{LayoutBox, LayoutEngine, LayoutError, LayoutKind, LayoutTree, NodeId};
use zui_types::{Rect, Size};

#[derive(Clone, Copy, Debug)]
pub struct LayoutLimits {
    pub max_nodes: usize,
    pub max_depth: usize,
    pub max_tracks_per_axis: usize,
}
impl Default for LayoutLimits {
    fn default() -> Self {
        Self {
            max_nodes: 10_000,
            max_depth: 128,
            max_tracks_per_axis: 256,
        }
    }
}

/// Uses the caller's stable IDs and topology, never a second TaffyTree.
/// Caches are per compute pass until the runtime supplies an invalidation protocol.
#[derive(Default)]
pub struct TaffyLayout {
    limits: LayoutLimits,
}
impl TaffyLayout {
    pub fn new(limits: LayoutLimits) -> Self {
        Self { limits }
    }
}

impl LayoutEngine for TaffyLayout {
    fn compute(
        &mut self,
        tree: &mut dyn LayoutTree,
        root: NodeId,
        viewport: Size,
    ) -> Result<(), LayoutError> {
        if !viewport.is_valid()
            || viewport.width > f32::MAX as f64
            || viewport.height > f32::MAX as f64
        {
            return Err(LayoutError::InvalidViewport);
        }
        let mut nodes = BTreeMap::new();
        let mut pending = vec![(root, 1, false)];
        while let Some((id, depth, stacked)) = pending.pop() {
            if nodes.contains_key(&id) {
                return Err(LayoutError::RepeatedNode(id));
            }
            if nodes.len() >= self.limits.max_nodes || depth > self.limits.max_depth {
                return Err(LayoutError::BudgetExceeded);
            }
            let node = tree.node(id).ok_or(LayoutError::MissingNode(id))?;
            if node.children.len()
                > self
                    .limits
                    .max_nodes
                    .saturating_sub(nodes.len() + pending.len())
                || node.style.columns.len() > self.limits.max_tracks_per_axis
                || node.style.rows.len() > self.limits.max_tracks_per_axis
                || (node.style.kind == LayoutKind::Stack && self.limits.max_tracks_per_axis == 0)
            {
                return Err(LayoutError::BudgetExceeded);
            }
            if node.style.kind == LayoutKind::Leaf && !node.children.is_empty() {
                return Err(LayoutError::InvalidStyle(id));
            }
            let mut converted = style::convert(node.style).ok_or(LayoutError::InvalidStyle(id))?;
            if let Some(ratio) = tree.preferred_aspect_ratio(id) {
                if !ratio.is_finite() || ratio <= 0.0 {
                    return Err(LayoutError::InvalidStyle(id));
                }
                converted.aspect_ratio = Some(ratio);
            }
            // Stack uses one shared grid cell. Placement is an adapter detail;
            // children keep their own sizing/alignment and nested layout kind.
            if stacked {
                converted.grid_column = taffy::Line {
                    start: taffy::style_helpers::line(1),
                    end: taffy::style_helpers::span(1),
                };
                converted.grid_row = taffy::Line {
                    start: taffy::style_helpers::line(1),
                    end: taffy::style_helpers::span(1),
                };
            }
            nodes.insert(id, adapter::Entry::new(node.style.kind, converted));
            pending.extend(
                node.children
                    .iter()
                    .rev()
                    .map(|child| (*child, depth + 1, node.style.kind == LayoutKind::Stack)),
            );
        }
        let mut adapter = adapter::Adapter {
            tree,
            nodes,
            failure: None,
        };
        taffy::compute_root_layout(
            &mut adapter,
            taffy::NodeId::new(root.0),
            taffy::Size {
                width: taffy::AvailableSpace::Definite(viewport.width as f32),
                height: taffy::AvailableSpace::Definite(viewport.height as f32),
            },
        );
        if let Some(error) = adapter.failure {
            return Err(error);
        }
        // Validate everything before publishing any geometry.
        let results: Result<Vec<_>, _> = adapter
            .nodes
            .iter()
            .map(|(&id, entry)| {
                let layout = entry.layout;
                let bounds = Rect::new(
                    layout.location.x as f64,
                    layout.location.y as f64,
                    layout.size.width as f64,
                    layout.size.height as f64,
                );
                let extent = layout.scrollable_overflow_rect;
                let scroll_extent = Size::new(
                    f64::from(extent.right).max(bounds.size.width),
                    f64::from(extent.bottom).max(bounds.size.height),
                );
                if bounds.is_valid()
                    && [extent.left, extent.right, extent.top, extent.bottom]
                        .into_iter()
                        .all(f32::is_finite)
                    && scroll_extent.is_valid()
                {
                    Ok((
                        id,
                        LayoutBox {
                            bounds,
                            scroll_extent,
                        },
                    ))
                } else {
                    Err(LayoutError::InvalidResult(id))
                }
            })
            .collect();
        for (id, layout) in results? {
            adapter.tree.set_layout(id, layout);
        }
        Ok(())
    }
}
