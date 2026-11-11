#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DamageConfig {
    pub max_regions: usize,
    pub max_scene_bytes: usize,
    pub max_pixel_bytes: usize,
}

impl Default for DamageConfig {
    fn default() -> Self {
        Self {
            max_regions: 24,
            max_scene_bytes: 16 * 1024 * 1024,
            max_pixel_bytes: 128 * 1024 * 1024,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DamageStats {
    pub regions: usize,
    pub rasterized_pixels: u64,
    pub total_pixels: u64,
    pub reused_frame: bool,
    pub retained_bytes: usize,
}
