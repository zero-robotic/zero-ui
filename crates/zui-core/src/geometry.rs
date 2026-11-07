use crate::{Tree, TreeError, presentation::Presentation, tree::Node};
use alloc::collections::BTreeMap;
use zui_api::{
    layout::{LayoutBox, LayoutEngine, LayoutError, LayoutNode, LayoutTree, MeasureInput},
    text::{TextError, TextLayout, TextRequest, TextService},
    widget::{Invalidation, MeasureContext, PaintContext, PrepareContext},
};
use zui_paint::{Painter, Path, Scene};
use zui_types::{Affine, NodeId, Point, Rect, Size};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub id: NodeId,
    pub local: Point,
}

struct LayoutAdapter<'a> {
    nodes: &'a mut BTreeMap<NodeId, Node>,
    outputs: BTreeMap<NodeId, LayoutBox>,
    invalid: bool,
    text: &'a mut dyn TextService,
}
impl LayoutTree for LayoutAdapter<'_> {
    fn preferred_aspect_ratio(&self, id: NodeId) -> Option<f32> {
        self.nodes
            .get(&id)
            .and_then(|node| node.widget.preferred_aspect_ratio())
    }
    fn node(&self, id: NodeId) -> Option<LayoutNode<'_>> {
        self.nodes.get(&id).map(|node| LayoutNode {
            style: &node.style,
            children: &node.children,
        })
    }
    fn measure(&mut self, id: NodeId, input: MeasureInput) -> Result<Size, LayoutError> {
        self.nodes
            .get_mut(&id)
            .ok_or(LayoutError::MissingNode(id))?
            .widget
            .measure(MeasureContext {
                id,
                input,
                text: self.text,
            })
    }
    fn set_layout(&mut self, id: NodeId, layout: LayoutBox) {
        if !self.nodes.contains_key(&id)
            || !layout.bounds.is_valid()
            || !layout.scroll_extent.is_valid()
            || layout.scroll_extent.width < layout.bounds.size.width
            || layout.scroll_extent.height < layout.bounds.size.height
            || self.outputs.insert(id, layout).is_some()
        {
            self.invalid = true;
        }
    }
}

struct UnavailableText;
impl TextService for UnavailableText {
    fn font_epoch(&self) -> u64 {
        0
    }
    fn layout(&mut self, _: TextRequest<'_>) -> Result<TextLayout, TextError> {
        Err(TextError::ServiceUnavailable)
    }
}

pub(crate) fn content_bounds(node: &Node, bounds: Rect) -> Rect {
    let padding = node.style.padding;
    Rect::new(
        padding.left as f64,
        padding.top as f64,
        (bounds.size.width - padding.left as f64 - padding.right as f64).max(0.0),
        (bounds.size.height - padding.top as f64 - padding.bottom as f64).max(0.0),
    )
}

impl Tree {
    /// Returns whether geometry was recomputed. Inject any implementation of the
    /// layout contract; the runtime never imports a concrete algorithm crate.
    pub fn layout(
        &mut self,
        engine: &mut dyn LayoutEngine,
        viewport: Size,
    ) -> Result<bool, TreeError> {
        self.layout_with_text(engine, &mut UnavailableText, viewport)
    }
    /// Font epoch changes force layout. A replacement service must use a distinct
    /// epoch (or the owner must rebuild the tree) to invalidate widget text caches.
    pub fn layout_with_text(
        &mut self,
        engine: &mut dyn LayoutEngine,
        text: &mut dyn TextService,
        viewport: Size,
    ) -> Result<bool, TreeError> {
        if self.dirty == Invalidation::None
            && self.viewport == Some(viewport)
            && self.text_epoch == Some(text.font_epoch())
        {
            // Focus scope/reveal work remains observable even in a clean tree.
            self.sync_focus_scopes();
            self.reveal_pending_focus()?;
            if self.dirty == Invalidation::None {
                return Ok(false);
            }
        }
        let mut changed = false;
        // Bound callbacks even if a custom owner changes layout on every query.
        for _ in 0..16 {
            changed |= self.layout_once(engine, text, viewport)?;
            if let Err(error) = self.refresh_owned_children() {
                self.dirty = Invalidation::Layout;
                return Err(error);
            }
            if self.dirty != Invalidation::Layout {
                self.sync_focus_scopes();
                if self.dirty == Invalidation::Layout {
                    continue;
                }
                self.reveal_pending_focus()?;
                return Ok(changed);
            }
        }
        Err(TreeError::ChildLayoutDidNotConverge)
    }

    fn layout_once(
        &mut self,
        engine: &mut dyn LayoutEngine,
        text: &mut dyn TextService,
        viewport: Size,
    ) -> Result<bool, TreeError> {
        let epoch = text.font_epoch();
        if self.dirty != Invalidation::Layout
            && self.viewport == Some(viewport)
            && self.text_epoch == Some(epoch)
        {
            return Ok(false);
        }
        self.dirty = Invalidation::Layout;
        if !viewport.is_valid() {
            return Err(LayoutError::InvalidViewport.into());
        }
        let mut adapter = LayoutAdapter {
            nodes: &mut self.nodes,
            outputs: BTreeMap::new(),
            invalid: false,
            text,
        };
        engine.compute(&mut adapter, self.root, viewport)?;
        if adapter.invalid || adapter.outputs.len() != adapter.nodes.len() {
            return Err(TreeError::IncompleteLayout);
        }
        for (&id, layout) in &adapter.outputs {
            let node = adapter.nodes.get_mut(&id).expect("validated layout node");
            let content_size = content_bounds(node, layout.bounds).size;
            node.widget.prepare(PrepareContext {
                id,
                layout: *layout,
                content_size,
                text: adapter.text,
            })?;
        }
        for (id, layout) in adapter.outputs {
            adapter
                .nodes
                .get_mut(&id)
                .expect("validated layout node")
                .layout = layout;
        }
        self.viewport = Some(viewport);
        self.text_epoch = Some(epoch);
        self.dirty = Invalidation::Paint;
        self.reveal_pending_focus()?;
        if let Err(error) = self.notify_owned_children() {
            self.dirty = Invalidation::Layout;
            return Err(error);
        }
        Ok(true)
    }
    pub fn layout_box(&self, id: NodeId) -> Result<LayoutBox, TreeError> {
        self.ready()?;
        Ok(self.get(id)?.layout)
    }
    fn ready(&self) -> Result<(), TreeError> {
        if self.dirty == Invalidation::Layout {
            Err(TreeError::LayoutRequired)
        } else {
            Ok(())
        }
    }
    /// Rebuilds the scene in child order; renderers receive no widget references.
    pub fn scene(&mut self) -> Result<Scene, TreeError> {
        self.ready()?;
        let mut scene = Scene::new();
        let presentation = self.presentation()?;
        self.paint_node(self.root, &presentation, &mut scene.painter());
        self.dirty = Invalidation::None;
        Ok(scene)
    }
    fn paint_node(&self, id: NodeId, presentation: &Presentation, painter: &mut Painter<'_>) {
        let node = &self.nodes[&id];
        let bounds = node.layout.bounds;
        painter.with_transform(
            Affine::translate(bounds.origin.x, bounds.origin.y),
            |painter| {
                let draw = |painter: &mut Painter<'_>| {
                    let context = PaintContext {
                        id,
                        size: bounds.size,
                        content_bounds: content_bounds(node, bounds),
                    };
                    node.widget.paint(context, painter);
                    let viewport = presentation[&id].children;
                    let draw_children = |painter: &mut Painter<'_>| {
                        painter.with_transform(
                            Affine::translate(viewport.translation.x, viewport.translation.y),
                            |painter| {
                                for &child in &node.children {
                                    self.paint_node(child, presentation, painter);
                                }
                            },
                        );
                    };
                    if let Some(clip) = viewport.clip {
                        painter.with_clip(Path::rectangle(clip), draw_children);
                    } else {
                        draw_children(painter);
                    }
                    node.widget.paint_foreground(context, painter);
                };
                if node.clip {
                    painter.with_clip(
                        Path::rectangle(Rect::new(0.0, 0.0, bounds.size.width, bounds.size.height)),
                        draw,
                    );
                } else {
                    draw(painter);
                }
            },
        );
    }
    /// Reverse paint order, sharing the same transforms and rectangular clips.
    /// An unclipped parent may contain a hittable overflowing child.
    pub fn hit_test(&self, point: Point) -> Result<Option<Hit>, TreeError> {
        self.ready()?;
        if !point.is_finite() {
            return Ok(None);
        }
        let presentation = self.presentation()?;
        Ok(self.hit_node(self.root, point, &presentation))
    }
    pub(crate) fn hit_node(
        &self,
        id: NodeId,
        point: Point,
        presentation: &Presentation,
    ) -> Option<Hit> {
        let node = &self.nodes[&id];
        let bounds = node.layout.bounds;
        let presented = &presentation[&id];
        let local = Point::new(point.x - presented.origin.x, point.y - presented.origin.y);
        if node.clip && !Rect::new(0.0, 0.0, bounds.size.width, bounds.size.height).contains(local)
        {
            return None;
        }
        if node.widget.hit_test_foreground(
            local,
            PaintContext {
                id,
                size: bounds.size,
                content_bounds: content_bounds(node, bounds),
            },
        ) {
            return Some(Hit { id, local });
        }
        if presented
            .children
            .clip
            .is_none_or(|clip| clip.contains(local))
        {
            for &child in node.children.iter().rev() {
                if let Some(hit) = self.hit_node(child, point, presentation) {
                    return Some(hit);
                }
            }
        }
        node.widget
            .hit_test(local, bounds.size)
            .then_some(Hit { id, local })
    }
}
