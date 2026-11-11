use alloc::collections::BTreeMap;
use zui_paint::Scene;
use zui_types::NodeId;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PaintCacheStats {
    pub reused_subtrees: usize,
    pub painted_nodes: usize,
    pub retained_bytes: usize,
}

pub(crate) struct PaintCache {
    scenes: BTreeMap<NodeId, (Scene, usize)>,
    pub budget: usize,
    pub stats: PaintCacheStats,
}

impl Default for PaintCache {
    fn default() -> Self {
        Self {
            scenes: BTreeMap::new(),
            budget: 16 * 1024 * 1024,
            stats: Default::default(),
        }
    }
}

impl PaintCache {
    pub fn clear(&mut self) {
        self.scenes.clear();
        self.stats.retained_bytes = 0;
    }
    pub fn remove(&mut self, id: NodeId) {
        if let Some((_, bytes)) = self.scenes.remove(&id) {
            self.stats.retained_bytes -= bytes;
        }
    }
    pub fn get(&mut self, id: NodeId) -> Option<Scene> {
        let scene = self.scenes.get(&id)?.0.clone();
        self.stats.reused_subtrees += 1;
        Some(scene)
    }
    pub fn insert(&mut self, id: NodeId, scene: &Scene) {
        self.remove(id);
        let bytes =
            scene
                .storage_bytes()
                .saturating_add(core::mem::size_of::<(NodeId, Scene, usize)>());
        if bytes <= self.budget.saturating_sub(self.stats.retained_bytes) {
            self.scenes.insert(id, (scene.clone(), bytes));
            self.stats.retained_bytes += bytes;
        }
    }
}
