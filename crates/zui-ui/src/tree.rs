use crate::{
    event::{Action, EventContext, EventResult, SceneUpdate, UiEvent},
    layout::Constraints,
    theme::Theme,
    widget::{RenderBuildContext, Widget},
};
use zui_core::Rect;
use zui_render::RenderNodeIndex;

pub struct WidgetTree {
    root: Box<dyn Widget>,
    theme: Theme,
    render_node: Option<zui_render::RenderNode>,
    render_index: RenderNodeIndex,
    layout_dirty: bool,
    paint_dirty: bool,
    scene_update: SceneUpdate,
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
            scene_update: SceneUpdate::default(),
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
        self.scene_update.clear();
        self.scene_update.request_full_rebuild();
        size
    }
    pub fn event(&mut self, event: &UiEvent) -> (EventResult, Vec<Action>) {
        let mut ctx = EventContext::new();
        let result = self.root.event(event, &mut ctx);
        let actions = ctx.take_actions();
        let mut update = ctx.take_scene_update();
        if result == EventResult::RequestRedraw && update.is_empty() {
            update.request_full_rebuild();
        }
        for (widget, regions) in update.node_regions() {
            if let (Some(node), Some(path)) = (
                self.render_node.as_mut(),
                self.render_index.path_for(widget),
            ) {
                for region in regions {
                    node.mark_dirty_path(path, zui_render::DirtyFlags::PAINT, Some(*region));
                }
            }
        }
        if !update.is_empty() {
            self.paint_dirty = true;
            self.scene_update.merge(update);
        }
        (result, actions)
    }
    pub fn render_node_cached(&mut self) -> &zui_render::RenderNode {
        if self.render_node.is_none() {
            let mut context = RenderBuildContext::new(None, &[], true, &self.theme);
            let mut node = self.root.build_render_node_incremental(&mut context);
            node.mark_dirty(zui_render::DirtyFlags::PAINT);
            self.render_index = node.build_index();
            self.render_node = Some(node);
        } else if self.paint_dirty {
            let previous = self.render_node.take();
            let dirty_paths = self
                .scene_update
                .dirty_node_ids()
                .filter_map(|node_id| self.render_index.path_for(node_id))
                .map(|path| path.to_vec())
                .collect::<Vec<_>>();
            let mut context = RenderBuildContext::new(
                previous.as_ref(),
                &dirty_paths,
                self.scene_update.full_rebuild(),
                &self.theme,
            );
            let mut node = self.root.build_render_node_incremental(&mut context);
            if self.scene_update.full_rebuild() {
                node.mark_dirty(zui_render::DirtyFlags::PAINT);
            } else {
                for path in &dirty_paths {
                    node.mark_dirty_path(path, zui_render::DirtyFlags::PAINT, None);
                }
            }
            if self.scene_update.full_rebuild() {
                self.render_index = node.build_index();
            } else {
                self.render_index.update_subtrees(&node, &dirty_paths);
            }
            self.render_node = Some(node);
        }
        self.render_node
            .as_ref()
            .expect("render node was just built")
    }
    pub fn build_render_node(&self) -> zui_render::RenderNode {
        let mut context = RenderBuildContext::new(None, &[], true, &self.theme);
        self.root.build_render_node_incremental(&mut context)
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
        self.scene_update.clear();
        self.scene_update.request_full_rebuild();
    }
    pub fn request_paint(&mut self, region: Option<Rect>) {
        self.paint_dirty = true;
        self.scene_update.request_full_rebuild();
        if let Some(region) = region {
            self.scene_update.add_damage(region);
        } else {
            self.scene_update.clear();
            self.scene_update.request_full_rebuild();
        }
    }
    pub fn request_paint_regions(&mut self, regions: impl IntoIterator<Item = Rect>) {
        self.paint_dirty = true;
        self.scene_update.request_full_rebuild();
        self.scene_update.extend_damage(regions);
    }
    pub fn needs_redraw(&self) -> bool {
        self.layout_dirty || self.paint_dirty
    }
    pub fn next_redraw(&self) -> Option<std::time::Instant> {
        self.root.next_redraw()
    }
    pub fn dirty_region(&self) -> Option<Rect> {
        self.scene_update
            .damage_regions()
            .iter()
            .copied()
            .reduce(union_rect)
    }
    pub fn dirty_regions(&self) -> &[Rect] {
        self.scene_update.damage_regions()
    }
    pub fn scene_update(&self) -> &SceneUpdate {
        &self.scene_update
    }
    pub fn mark_clean(&mut self) {
        self.layout_dirty = false;
        self.paint_dirty = false;
        self.scene_update.clear();
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

    /// Widget identities whose RenderNode subtrees changed since the last
    /// completed frame. The renderer resolves these through `render_index`.
    pub fn dirty_widget_ids(&self) -> Vec<u64> {
        self.scene_update.dirty_node_ids().collect()
    }
}

fn union_rect(a: Rect, b: Rect) -> Rect {
    let left = a.origin.x.0.min(b.origin.x.0);
    let top = a.origin.y.0.min(b.origin.y.0);
    let right = (a.origin.x.0 + a.size.width.0).max(b.origin.x.0 + b.size.width.0);
    let bottom = (a.origin.y.0 + a.size.height.0).max(b.origin.y.0 + b.size.height.0);
    Rect {
        origin: zui_core::Point {
            x: zui_core::Dip(left),
            y: zui_core::Dip(top),
        },
        size: zui_core::Size {
            width: zui_core::Dip(right - left),
            height: zui_core::Dip(bottom - top),
        },
    }
}
