use std::collections::BTreeMap;
use taffy::{CacheTree, LayoutPartialTree};
use zui_api::layout::{AvailableSpace, LayoutError, LayoutKind, LayoutTree, MeasureInput, NodeId};

pub(crate) struct Entry {
    kind: LayoutKind,
    style: taffy::Style,
    cache: taffy::Cache,
    pub layout: taffy::Layout,
    hoisted: Vec<taffy::NodeId>,
}
impl Entry {
    pub fn new(kind: LayoutKind, style: taffy::Style) -> Self {
        Self {
            kind,
            style,
            cache: taffy::Cache::new(),
            layout: taffy::Layout::default(),
            hoisted: Vec::new(),
        }
    }
}
pub(crate) struct Adapter<'a> {
    pub tree: &'a mut dyn LayoutTree,
    pub nodes: BTreeMap<NodeId, Entry>,
    pub failure: Option<LayoutError>,
}
fn id(node: taffy::NodeId) -> NodeId {
    NodeId(u64::from(node))
}
fn native(node: &NodeId) -> taffy::NodeId {
    taffy::NodeId::new(node.0)
}
fn space(value: taffy::AvailableSpace) -> AvailableSpace {
    match value {
        taffy::AvailableSpace::Definite(v) => AvailableSpace::Definite(non_negative(v)),
        taffy::AvailableSpace::MinContent => AvailableSpace::MinContent,
        taffy::AvailableSpace::MaxContent => AvailableSpace::MaxContent,
    }
}
// Taffy may subtract padding from an already exhausted available size. Public
// measurement describes usable content space; negative space is zero, while
// non-finite values remain visible to the normal validation/error path.
fn non_negative(value: f32) -> f32 {
    if value.is_finite() && value < 0.0 {
        0.0
    } else {
        value
    }
}

impl taffy::TraversePartialTree for Adapter<'_> {
    type ChildIter<'a>
        = std::iter::Map<std::slice::Iter<'a, NodeId>, fn(&NodeId) -> taffy::NodeId>
    where
        Self: 'a;
    fn child_ids(&self, node: taffy::NodeId) -> Self::ChildIter<'_> {
        self.tree
            .node(id(node))
            .expect("validated topology")
            .children
            .iter()
            .map(native)
    }
    fn child_count(&self, node: taffy::NodeId) -> usize {
        self.tree
            .node(id(node))
            .expect("validated topology")
            .children
            .len()
    }
    fn get_child_id(&self, node: taffy::NodeId, index: usize) -> taffy::NodeId {
        native(
            &self
                .tree
                .node(id(node))
                .expect("validated topology")
                .children[index],
        )
    }
}
impl taffy::TraverseTree for Adapter<'_> {}

impl LayoutPartialTree for Adapter<'_> {
    type CustomIdent = String;
    type CoreContainerStyle<'a>
        = &'a taffy::Style
    where
        Self: 'a;
    fn get_core_container_style(&self, node: taffy::NodeId) -> Self::CoreContainerStyle<'_> {
        &self.nodes[&id(node)].style
    }
    fn set_unrounded_layout(&mut self, node: taffy::NodeId, layout: &taffy::Layout) {
        self.nodes
            .get_mut(&id(node))
            .expect("validated node")
            .layout = *layout;
    }
    fn resolve_calc_value(&self, _: *const (), _: f32) -> f32 {
        unreachable!("ZUI styles do not encode calc pointers")
    }
    fn compute_child_layout(
        &mut self,
        node: taffy::NodeId,
        inputs: taffy::LayoutInput,
    ) -> taffy::LayoutOutput {
        taffy::compute_cached_layout(self, node, inputs, |adapter, node, inputs| {
            let key = id(node);
            let entry = &adapter.nodes[&key];
            let mut output = match entry.kind {
                LayoutKind::Row | LayoutKind::Column => {
                    taffy::compute_flexbox_layout(adapter, node, inputs)
                }
                LayoutKind::Grid | LayoutKind::Stack => {
                    taffy::compute_grid_layout(adapter, node, inputs)
                }
                LayoutKind::Leaf => taffy::compute_leaf_layout(
                    inputs,
                    &entry.style,
                    |_, _| unreachable!("ZUI styles do not encode calc pointers"),
                    |known, available| {
                        if adapter.failure.is_some() {
                            return taffy::Size::ZERO;
                        }
                        let input = MeasureInput {
                            known_width: known.width.map(non_negative),
                            known_height: known.height.map(non_negative),
                            available_width: space(available.width),
                            available_height: space(available.height),
                        };
                        match adapter.tree.measure(key, input) {
                            Ok(size)
                                if size.is_valid()
                                    && size.width <= f32::MAX as f64
                                    && size.height <= f32::MAX as f64 =>
                            {
                                taffy::Size {
                                    width: size.width as f32,
                                    height: size.height as f32,
                                }
                            }
                            Ok(_) => {
                                adapter.failure = Some(LayoutError::InvalidMeasurement(key));
                                taffy::Size::ZERO
                            }
                            Err(error) => {
                                adapter.failure = Some(error);
                                taffy::Size::ZERO
                            }
                        }
                    },
                ),
            };
            if inputs.run_mode == taffy::RunMode::PerformLayout {
                taffy::compute_oof_layout(adapter, node, &mut output);
            }
            output
        })
    }
}
impl CacheTree for Adapter<'_> {
    fn cache_get(
        &mut self,
        node: taffy::NodeId,
        inputs: &taffy::LayoutInput,
    ) -> Option<taffy::LayoutOutput> {
        self.nodes
            .get_mut(&id(node))
            .expect("validated node")
            .cache
            .get(inputs)
    }
    fn cache_store(
        &mut self,
        node: taffy::NodeId,
        inputs: &taffy::LayoutInput,
        output: taffy::LayoutOutput,
    ) {
        self.nodes
            .get_mut(&id(node))
            .expect("validated node")
            .cache
            .store(inputs, output);
    }
    fn cache_clear(&mut self, node: taffy::NodeId) {
        self.nodes
            .get_mut(&id(node))
            .expect("validated node")
            .cache
            .clear();
    }
}
impl taffy::LayoutContainingBlock for Adapter<'_> {
    type OofItemStyle<'a>
        = &'a taffy::Style
    where
        Self: 'a;
    fn get_oof_item_style(&self, node: taffy::NodeId) -> Self::OofItemStyle<'_> {
        &self.nodes[&id(node)].style
    }
    fn clear_hoisted_children(&mut self, node: taffy::NodeId) {
        self.nodes
            .get_mut(&id(node))
            .expect("validated node")
            .hoisted
            .clear();
    }
    fn add_hoisted_children(&mut self, node: taffy::NodeId, children: &[taffy::NodeId]) {
        self.nodes
            .get_mut(&id(node))
            .expect("validated node")
            .hoisted
            .extend_from_slice(children);
    }
}
impl taffy::LayoutFlexboxContainer for Adapter<'_> {
    type FlexboxContainerStyle<'a>
        = &'a taffy::Style
    where
        Self: 'a;
    type FlexboxItemStyle<'a>
        = &'a taffy::Style
    where
        Self: 'a;
    fn get_flexbox_container_style(&self, node: taffy::NodeId) -> Self::FlexboxContainerStyle<'_> {
        &self.nodes[&id(node)].style
    }
    fn get_flexbox_child_style(&self, node: taffy::NodeId) -> Self::FlexboxItemStyle<'_> {
        &self.nodes[&id(node)].style
    }
}
impl taffy::LayoutGridContainer for Adapter<'_> {
    type GridContainerStyle<'a>
        = &'a taffy::Style
    where
        Self: 'a;
    type GridItemStyle<'a>
        = &'a taffy::Style
    where
        Self: 'a;
    fn get_grid_container_style(&self, node: taffy::NodeId) -> Self::GridContainerStyle<'_> {
        &self.nodes[&id(node)].style
    }
    fn get_grid_child_style(&self, node: taffy::NodeId) -> Self::GridItemStyle<'_> {
        &self.nodes[&id(node)].style
    }
}
