use std::collections::HashSet;

use crate::{
    event::{Action, EventContext, EventResult, UiEvent},
    layout::Constraints,
    theme::Theme,
    widget::{RenderBuildContext, Widget},
};
use zui_core::Rect;
use zui_render::{DirtyRegionSet, RenderNodeIndex};

pub struct WidgetTree {
    root: Box<dyn Widget>,
    theme: Theme,
    render_node: Option<zui_render::RenderNode>,
    render_index: RenderNodeIndex,
    layout_dirty: bool,
    paint_dirty: bool,
    dirty_regions: DirtyRegionSet,
    dirty_widget_ids: HashSet<u64>,
    full_rebuild: bool,
}

impl WidgetTree {
    pub fn new(root: impl Widget + 'static) -> Self {
        Self {
            root: Box::new(root),
            theme: Theme::default(),
            render_node: None,
            render_index: RenderNodeIndex::default(),
            layout_dirty: true,
            paint_dirty: true,
            dirty_regions: DirtyRegionSet::new(),
            dirty_widget_ids: HashSet::new(),
            full_rebuild: true,
        }
    }
    pub fn measure(&mut self, constraints: Constraints) -> zui_core::Size {
        self.root.measure(constraints)
    }
    pub fn arrange(&mut self, bounds: zui_core::Rect) {
        self.root.arrange(bounds);
    }
    pub fn layout(&mut self, constraints: Constraints) -> zui_core::Size {
        let size = self.measure(constraints);
        let origin = self.root.bounds().origin;
        self.arrange(zui_core::Rect { origin, size });
        self.layout_dirty = false;
        self.paint_dirty = true;
        self.render_node = None;
        self.render_index = RenderNodeIndex::default();
        self.dirty_regions.clear();
        self.full_rebuild = true;
        size
    }
    pub fn event(&mut self, event: &UiEvent) -> (EventResult, Vec<Action>) {
        let mut ctx = EventContext::new();
        let result = self.root.event(event, &mut ctx);
        let actions = ctx.take_actions();
        let invalidations = ctx.take_invalidations();
        let dirty_regions = ctx.take_dirty_regions();
        for (widget, regions) in &invalidations {
            self.dirty_widget_ids.insert(widget.0);
            if let (Some(node), Some(path)) = (
                self.render_node.as_mut(),
                self.render_index.path_for(widget.0),
            ) {
                for region in regions {
                    node.mark_dirty_path(path, zui_render::DirtyFlags::PAINT, Some(*region));
                }
            }
        }
        if result == EventResult::RequestRedraw || !invalidations.is_empty() {
            if ctx.requires_full_redraw() {
                self.request_paint(None);
            } else {
                self.paint_dirty = true;
                self.dirty_regions.extend(dirty_regions);
            }
        }
        (result, actions)
    }
    pub fn render_node_cached(&mut self) -> &zui_render::RenderNode {
        if self.render_node.is_none() {
            let mut context = RenderBuildContext::new(None, &[], true, &self.theme);
            let mut node = self.root.build_render_node_incremental(&mut context);
            node.normalize_local_coordinates();
            node.mark_dirty(zui_render::DirtyFlags::PAINT);
            self.render_index = node.build_index();
            self.render_node = Some(node);
        } else if self.paint_dirty {
            let previous = self.render_node.take();
            let dirty_paths = self
                .dirty_widget_ids
                .iter()
                .filter_map(|source_id| self.render_index.path_for(*source_id))
                .map(|path| path.to_vec())
                .collect::<Vec<_>>();
            let mut context = RenderBuildContext::new(
                previous.as_ref(),
                &dirty_paths,
                self.full_rebuild,
                &self.theme,
            );
            let mut node = self.root.build_render_node_incremental(&mut context);
            node.normalize_local_coordinates();
            if self.full_rebuild {
                node.mark_dirty(zui_render::DirtyFlags::PAINT);
            } else {
                for path in &dirty_paths {
                    node.mark_dirty_path(path, zui_render::DirtyFlags::PAINT, None);
                }
            }
            self.render_index = node.build_index();
            self.render_node = Some(node);
        }
        self.render_node
            .as_ref()
            .expect("render node was just built")
    }
    pub fn build_render_node(&self) -> zui_render::RenderNode {
        let mut node = self.root.build_render_node(&self.theme);
        node.normalize_local_coordinates();
        node
    }
    pub fn theme(&self) -> &Theme {
        &self.theme
    }
    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
        self.root.set_theme(&self.theme);
        self.request_paint(None);
    }
    pub fn request_layout(&mut self) {
        self.layout_dirty = true;
        self.paint_dirty = true;
        self.render_node = None;
        self.render_index = RenderNodeIndex::default();
        self.dirty_regions.clear();
        self.dirty_widget_ids.clear();
        self.full_rebuild = true;
    }
    pub fn request_paint(&mut self, region: Option<Rect>) {
        self.paint_dirty = true;
        self.full_rebuild = true;
        if let Some(region) = region {
            self.dirty_regions.add(region);
        } else {
            self.dirty_regions.clear();
        }
    }
    pub fn request_paint_regions(&mut self, regions: impl IntoIterator<Item = Rect>) {
        self.paint_dirty = true;
        self.full_rebuild = true;
        self.dirty_regions.extend(regions);
    }
    pub fn needs_redraw(&self) -> bool {
        self.layout_dirty || self.paint_dirty
    }
    pub fn next_redraw(&self) -> Option<std::time::Instant> {
        self.root.next_redraw()
    }
    pub fn dirty_region(&self) -> Option<Rect> {
        self.dirty_regions.union()
    }
    pub fn dirty_regions(&self) -> &[Rect] {
        self.dirty_regions.as_slice()
    }
    pub fn mark_clean(&mut self) {
        self.layout_dirty = false;
        self.paint_dirty = false;
        self.dirty_regions.clear();
        self.dirty_widget_ids.clear();
        self.full_rebuild = false;
        if let Some(node) = self.render_node.as_mut() {
            node.clear_dirty();
        }
    }
    pub fn root(&self) -> &dyn Widget {
        &*self.root
    }
    pub fn root_mut(&mut self) -> &mut dyn Widget {
        &mut *self.root
    }

    pub fn render_index(&self) -> &RenderNodeIndex {
        &self.render_index
    }
}
