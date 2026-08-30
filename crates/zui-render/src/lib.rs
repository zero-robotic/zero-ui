//! GPU rendering owned by the application composition root.
//!
//! This crate deliberately exposes no `wgpu::Device` to widgets. Widgets produce
//! drawing data; `Renderer` owns the GPU and the per-window swap-chain surfaces.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::sync::{Arc, Mutex, OnceLock};

use lyon_path::{math::point as lyon_point, Path as LyonPath};
use lyon_tessellation::{
    BuffersBuilder, FillOptions, FillRule as LyonFillRule, FillTessellator, FillVertex,
    StrokeOptions, StrokeTessellator, StrokeVertex, VertexBuffers,
};
use wgpu::util::DeviceExt;
use zui_core::{Color, Dip, PhysicalSize, Point, Rect, ScaleFactor, Size, WindowId};
use zui_platform::spi::RawWindowHandleProvider;

pub use wgpu;

mod draw_batches;
mod error;
mod gpu_pipeline;
mod paint;

use draw_batches::*;
pub use error::RenderError;
use gpu_pipeline::*;
use paint::transform_clip_shape;
pub use paint::{ClipShape, FillRule, IconPath, PaintCommand, PathCommand};

static SYSTEM_FONTS: OnceLock<Vec<fontdue::Font>> = OnceLock::new();
static TEXT_MEASURE_CACHE: OnceLock<Mutex<TextMeasureCache>> = OnceLock::new();

/// Bounded LRU for CPU text metrics. Text input and log-style views can
/// produce unbounded distinct strings, so this cache must not grow with the
/// lifetime of the process.
struct TextMeasureCache {
    widths: HashMap<(String, u32), Dip>,
    last_used: HashMap<(String, u32), u64>,
    clock: u64,
    capacity: usize,
}

impl Default for TextMeasureCache {
    fn default() -> Self {
        Self {
            widths: HashMap::new(),
            last_used: HashMap::new(),
            clock: 0,
            capacity: 4096,
        }
    }
}

impl TextMeasureCache {
    fn get(&mut self, key: &(String, u32)) -> Option<Dip> {
        let width = self.widths.get(key).copied()?;
        self.touch(key);
        Some(width)
    }

    fn insert(&mut self, key: (String, u32), width: Dip) {
        self.widths.insert(key.clone(), width);
        self.touch(&key);
        while self.widths.len() > self.capacity {
            let Some(oldest) = self
                .last_used
                .iter()
                .min_by_key(|(_, stamp)| *stamp)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            self.widths.remove(&oldest);
            self.last_used.remove(&oldest);
        }
    }

    fn touch(&mut self, key: &(String, u32)) {
        self.clock = self.clock.wrapping_add(1);
        self.last_used.insert(key.clone(), self.clock);
    }
}

type GlyphKey = (usize, char, u32, u32);

/// Vertical metrics shared by text measurement and glyph rasterization.
/// All values are logical DIPs and use positive distances.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextMetrics {
    pub ascent: Dip,
    pub descent: Dip,
    pub line_gap: Dip,
    pub line_height: Dip,
}

/// Converts the toolkit's scale token to the point size passed to the font
/// rasterizer. Keeping this conversion here prevents widget-side constants
/// from drifting away from the renderer.
pub fn text_font_size(scale: u32) -> f32 {
    (scale.max(1) * 7) as f32
}

/// Returns line metrics from the same fallback font set used to rasterize
/// [`PaintCommand::Text`]. The envelope covers every loaded fallback font so
/// mixed Latin/CJK text has a line box large enough for all selected glyphs.
pub fn text_metrics(scale: u32) -> TextMetrics {
    let font_size = text_font_size(scale);
    let mut ascent = 0.0_f32;
    let mut descent = 0.0_f32;
    let mut line_gap = 0.0_f32;
    let mut line_height = 0.0_f32;
    for font in cached_system_fonts() {
        if let Some(metrics) = font.horizontal_line_metrics(font_size) {
            ascent = ascent.max(metrics.ascent);
            descent = descent.max((-metrics.descent).max(0.0));
            line_gap = line_gap.max(metrics.line_gap.max(0.0));
            line_height = line_height.max(metrics.new_line_size);
        }
    }
    if line_height <= 0.0 {
        ascent = font_size * 0.8;
        descent = font_size - ascent;
        line_height = font_size;
    }
    line_height = line_height.max(ascent + descent + line_gap);
    TextMetrics {
        ascent: Dip(ascent),
        descent: Dip(descent),
        line_gap: Dip(line_gap),
        line_height: Dip(line_height),
    }
}

/// Measures text using the same system font used by the renderer.
pub fn measure_text(text: &str, scale: u32) -> Dip {
    let scale = scale.max(1);
    let key = (text.to_owned(), scale);
    let cache = TEXT_MEASURE_CACHE.get_or_init(|| Mutex::new(TextMeasureCache::default()));
    if let Some(width) = cache.lock().expect("text measure cache poisoned").get(&key) {
        return width;
    }
    let fonts = cached_system_fonts();
    let width = if !fonts.is_empty() {
        let size = text_font_size(scale);
        Dip(text
            .chars()
            .map(|character| {
                fonts
                    .iter()
                    .find(|font| font.lookup_glyph_index(character) != 0)
                    .map(|font| font.metrics(character, size).advance_width)
                    .unwrap_or(size)
            })
            .sum())
    } else {
        Dip(text.chars().count() as f32 * 6.0 * scale as f32)
    };
    cache
        .lock()
        .expect("text measure cache poisoned")
        .insert(key, width);
    width
}

// Paint commands and vector path types live in `paint.rs`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImageId(pub u64);

/// Stable renderer-owned reference used for resource lifetime accounting.
/// Widgets keep value IDs in paint commands; retained RenderNodes retain these
/// handles while their GPU records are resident.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResourceHandle {
    Image(ImageId),
    Icon(u64),
    Path(u64),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageResource {
    pub width: u32,
    pub height: u32,
    pub rgba8: Vec<u8>,
}

impl ImageResource {
    pub fn new(width: u32, height: u32, rgba8: Vec<u8>) -> Result<Self, RenderError> {
        let expected = width as usize * height as usize * 4;
        if width == 0 || height == 0 || rgba8.len() != expected {
            return Err(RenderError::InvalidCommand(
                "image dimensions do not match RGBA8 data".into(),
            ));
        }
        Ok(Self {
            width,
            height,
            rgba8,
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct ResourceCache {
    images: HashMap<ImageId, ImageResource>,
}

impl ResourceCache {
    pub fn register_image(&mut self, id: ImageId, image: ImageResource) {
        self.images.insert(id, image);
    }

    pub fn remove_image(&mut self, id: ImageId) -> Option<ImageResource> {
        self.images.remove(&id)
    }

    pub fn image(&self, id: ImageId) -> Option<&ImageResource> {
        self.images.get(&id)
    }

    pub fn contains_image(&self, id: ImageId) -> bool {
        self.images.contains_key(&id)
    }
}

pub struct ResourceManager {
    cpu: ResourceCache,
    gpu_images: HashMap<ImageId, GpuImage>,
    image_atlases: Vec<ImageAtlas>,
    image_bind_groups: HashMap<(WindowId, usize), wgpu::BindGroup>,
    last_used: HashMap<ImageId, u64>,
    clock: u64,
    max_gpu_images: usize,
    image_sampler: wgpu::Sampler,
    icons: HashMap<u64, IconPath>,
    budget: ResourceBudget,
    fonts: &'static [fontdue::Font],
    glyph_cache: HashMap<GlyphKey, CachedGlyph>,
    glyph_last_used: HashMap<GlyphKey, u64>,
    font_cache: HashMap<char, Option<usize>>,
    path_cache: HashMap<u64, PathMesh>,
    references: HashMap<ResourceHandle, usize>,
    auxiliary_last_used: HashMap<ResourceHandle, u64>,
    next_internal_image_id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceBudget {
    pub max_gpu_images: usize,
    pub max_gpu_image_bytes: usize,
    /// Total bytes reserved by atlas pages. This is separate from image
    /// payload bytes because an atlas has unavoidable unused space.
    pub max_gpu_atlas_bytes: usize,
}

impl Default for ResourceBudget {
    fn default() -> Self {
        Self {
            max_gpu_images: 256,
            max_gpu_image_bytes: 256 * 1024 * 1024,
            max_gpu_atlas_bytes: 256 * 1024 * 1024,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ResourceUsage {
    pub cpu_image_bytes: usize,
    pub gpu_image_bytes: usize,
    pub gpu_atlas_bytes: usize,
    pub gpu_atlas_page_count: usize,
    pub gpu_image_count: usize,
    pub icon_count: usize,
    pub cached_glyph_count: usize,
    pub cached_path_count: usize,
}

/// CPU-side submission metrics for the most recently presented frame.
/// They make retained/tile optimizations observable without exposing backend
/// internals to widgets.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameStats {
    pub dirty_tile_count: usize,
    pub retained_candidate_count: usize,
    pub replay_entry_count: usize,
    pub direct_draw_count: usize,
    pub indirect_submission_count: usize,
    pub indirect_draw_count: usize,
    pub pipeline_switch_count: usize,
    pub bind_group_switch_count: usize,
    pub scissor_clip_count: usize,
    pub stencil_clip_count: usize,
    pub full_frame: bool,
}

impl ResourceManager {
    fn new(device: &wgpu::Device, fonts: &'static [fontdue::Font]) -> Self {
        Self {
            cpu: ResourceCache::default(),
            gpu_images: HashMap::new(),
            image_atlases: Vec::new(),
            image_bind_groups: HashMap::new(),
            last_used: HashMap::new(),
            clock: 0,
            max_gpu_images: 256,
            image_sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("zui-render image sampler"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            }),
            icons: HashMap::new(),
            budget: ResourceBudget::default(),
            fonts,
            glyph_cache: HashMap::new(),
            glyph_last_used: HashMap::new(),
            font_cache: HashMap::new(),
            path_cache: HashMap::new(),
            references: HashMap::new(),
            auxiliary_last_used: HashMap::new(),
            next_internal_image_id: u64::MAX,
        }
    }

    pub fn set_gpu_image_capacity(&mut self, capacity: usize) {
        self.max_gpu_images = capacity.max(1);
        self.budget.max_gpu_images = self.max_gpu_images;
        self.evict_gpu_images();
    }

    pub fn set_budget(&mut self, budget: ResourceBudget) {
        self.budget = ResourceBudget {
            max_gpu_images: budget.max_gpu_images.max(1),
            max_gpu_image_bytes: budget.max_gpu_image_bytes.max(4),
            max_gpu_atlas_bytes: budget.max_gpu_atlas_bytes.max(4),
        };
        self.max_gpu_images = self.budget.max_gpu_images;
        self.evict_gpu_images();
    }

    pub fn budget(&self) -> ResourceBudget {
        self.budget
    }

    pub fn usage(&self) -> ResourceUsage {
        ResourceUsage {
            cpu_image_bytes: self
                .cpu
                .images
                .values()
                .map(|image| image.rgba8.len())
                .sum(),
            gpu_image_bytes: self.gpu_images.values().map(|image| image.bytes).sum(),
            gpu_atlas_bytes: self.atlas_bytes(),
            gpu_atlas_page_count: self.image_atlases.len(),
            gpu_image_count: self.gpu_images.len(),
            icon_count: self.icons.len(),
            cached_glyph_count: self.glyph_cache.len(),
            cached_path_count: self.path_cache.len(),
        }
    }

    pub fn retain(&mut self, handle: ResourceHandle) {
        *self.references.entry(handle).or_default() += 1;
        self.touch_handle(handle);
    }

    pub fn release(&mut self, handle: ResourceHandle) {
        let Some(references) = self.references.get_mut(&handle) else {
            return;
        };
        *references = references.saturating_sub(1);
        if *references == 0 {
            self.references.remove(&handle);
        }
        self.evict_auxiliary_resources();
    }

    pub fn reference_count(&self, handle: ResourceHandle) -> usize {
        self.references.get(&handle).copied().unwrap_or(0)
    }

    fn retain_all(&mut self, handles: &[ResourceHandle]) {
        for handle in handles {
            self.retain(*handle);
        }
    }

    fn release_all(&mut self, handles: &[ResourceHandle]) {
        for handle in handles {
            self.release(*handle);
        }
    }

    pub fn register_icon(&mut self, id: u64, path: IconPath) {
        self.icons.insert(id, path);
        self.touch_handle(ResourceHandle::Icon(id));
    }
    pub fn icon(&self, id: u64) -> Option<&IconPath> {
        self.icons.get(&id)
    }
    pub fn remove_icon(&mut self, id: u64) -> Option<IconPath> {
        if self.reference_count(ResourceHandle::Icon(id)) != 0 {
            return None;
        }
        self.auxiliary_last_used.remove(&ResourceHandle::Icon(id));
        self.icons.remove(&id)
    }

    fn register_image(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        id: ImageId,
        image: ImageResource,
    ) {
        self.cpu.register_image(id, image.clone());
        self.upload_gpu_image(device, queue, id, &image);
    }

    fn upload_gpu_image(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        id: ImageId,
        image: &ImageResource,
    ) {
        let Some(slot) = self.allocate_atlas_slot(device, image.width, image.height) else {
            // Keep the CPU copy registered. A later eviction or a larger
            // resource budget may make this image resident again.
            return;
        };
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.image_atlases[slot.page].texture,
                mip_level: 0,
                origin: slot.origin,
                aspect: wgpu::TextureAspect::All,
            },
            &image.rgba8,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(image.width * 4),
                rows_per_image: Some(image.height),
            },
            wgpu::Extent3d {
                width: image.width,
                height: image.height,
                depth_or_array_layers: 1,
            },
        );
        self.gpu_images.insert(
            id,
            GpuImage {
                bytes: image.rgba8.len(),
                slot,
            },
        );
        self.touch(id);
        self.evict_gpu_images();
    }

    fn allocate_atlas_slot(
        &mut self,
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> Option<AtlasSlot> {
        const ATLAS_PAGE_SIZE: u32 = 2048;
        if width > ATLAS_PAGE_SIZE || height > ATLAS_PAGE_SIZE {
            return None;
        }
        if let Some(slot) = self.allocate_from_existing_pages(width, height) {
            return Some(slot);
        }

        let page_bytes = ATLAS_PAGE_SIZE as usize * ATLAS_PAGE_SIZE as usize * 4;
        if self.atlas_bytes().saturating_add(page_bytes) > self.budget.max_gpu_atlas_bytes {
            while self.evict_one_gpu_image() {
                if let Some(slot) = self.allocate_from_existing_pages(width, height) {
                    return Some(slot);
                }
            }
            return None;
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("zui-render image atlas page"),
            size: wgpu::Extent3d {
                width: ATLAS_PAGE_SIZE,
                height: ATLAS_PAGE_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let page = self.image_atlases.len();
        self.image_atlases.push(ImageAtlas::new(texture));
        self.image_atlases[page].allocate(page, width, height)
    }

    fn allocate_from_existing_pages(&mut self, width: u32, height: u32) -> Option<AtlasSlot> {
        self.image_atlases
            .iter_mut()
            .enumerate()
            .find_map(|(page, atlas)| atlas.allocate(page, width, height))
    }

    fn atlas_bytes(&self) -> usize {
        self.image_atlases.iter().map(ImageAtlas::bytes).sum()
    }

    fn touch(&mut self, id: ImageId) {
        self.clock = self.clock.wrapping_add(1);
        self.last_used.insert(id, self.clock);
    }

    fn touch_handle(&mut self, handle: ResourceHandle) {
        self.clock = self.clock.wrapping_add(1);
        self.auxiliary_last_used.insert(handle, self.clock);
    }

    fn evict_gpu_images(&mut self) {
        while self.gpu_images.len() > self.max_gpu_images
            || self
                .gpu_images
                .values()
                .map(|image| image.bytes)
                .sum::<usize>()
                > self.budget.max_gpu_image_bytes
        {
            if !self.evict_one_gpu_image() {
                break;
            }
        }
    }

    fn evict_one_gpu_image(&mut self) -> bool {
        let Some(oldest) = self
            .last_used
            .iter()
            .filter(|(id, _)| self.reference_count(ResourceHandle::Image(**id)) == 0)
            .min_by_key(|(_, stamp)| *stamp)
            .map(|(id, _)| *id)
        else {
            return false;
        };
        if let Some(image) = self.gpu_images.remove(&oldest) {
            if let Some(atlas) = self.image_atlases.get_mut(image.slot.page) {
                atlas.release(image.slot);
            }
        }
        self.last_used.remove(&oldest);
        self.reclaim_empty_atlas_tail();
        true
    }

    fn remove_image(&mut self, id: ImageId) -> Option<ImageResource> {
        if self.reference_count(ResourceHandle::Image(id)) != 0 {
            return None;
        }
        if let Some(image) = self.gpu_images.remove(&id) {
            if let Some(atlas) = self.image_atlases.get_mut(image.slot.page) {
                atlas.release(image.slot);
            }
        }
        self.last_used.remove(&id);
        self.reclaim_empty_atlas_tail();
        self.cpu.remove_image(id)
    }

    /// Atlas page indices are stored in GPU image slots, so only trailing
    /// empty pages may be reclaimed without rebasing live resources.
    fn reclaim_empty_atlas_tail(&mut self) {
        while self.image_atlases.len() > 1 {
            let page = self.image_atlases.len() - 1;
            if self
                .gpu_images
                .values()
                .any(|image| image.slot.page == page)
            {
                break;
            }
            self.image_atlases.pop();
        }
        let page_count = self.image_atlases.len();
        self.image_bind_groups
            .retain(|(_, page), _| *page < page_count);
    }

    fn register_glyph(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        alpha: &[u8],
    ) -> ImageId {
        let id = ImageId(self.next_internal_image_id);
        self.next_internal_image_id = self.next_internal_image_id.wrapping_sub(1);
        let mut rgba8 = Vec::with_capacity(alpha.len() * 4);
        for alpha in alpha {
            rgba8.extend_from_slice(&[255, 255, 255, *alpha]);
        }
        // Rasterizers may report an empty bitmap for whitespace. Keep a
        // transparent texel so the atlas contract always has valid geometry.
        let image = ImageResource::new(
            width.max(1),
            height.max(1),
            if rgba8.is_empty() { vec![0; 4] } else { rgba8 },
        )
        .expect("glyph bitmap dimensions are valid");
        self.register_image(device, queue, id, image);
        id
    }

    fn touch_glyph(&mut self, key: GlyphKey) {
        self.clock = self.clock.wrapping_add(1);
        self.glyph_last_used.insert(key, self.clock);
    }

    fn evict_glyphs(&mut self) {
        let capacity = self.max_gpu_images.saturating_mul(8).max(256);
        while self.glyph_cache.len() > capacity {
            let Some(key) = self
                .glyph_last_used
                .iter()
                .filter(|(key, _)| {
                    self.glyph_cache.get(*key).is_some_and(|glyph| {
                        self.reference_count(ResourceHandle::Image(glyph.image)) == 0
                    })
                })
                .min_by_key(|(_, stamp)| *stamp)
                .map(|(key, _)| *key)
            else {
                break;
            };
            let Some(glyph) = self.glyph_cache.remove(&key) else {
                continue;
            };
            self.glyph_last_used.remove(&key);
            let _ = self.remove_image(glyph.image);
        }
    }

    fn tessellate_path(&mut self, path: &IconPath) -> PathMesh {
        let key = path_cache_key(path);
        self.touch_handle(ResourceHandle::Path(key));
        self.path_cache
            .entry(key)
            .or_insert_with(|| path_fill_mesh(path, Color::WHITE))
            .clone()
    }

    fn evict_auxiliary_resources(&mut self) {
        let capacity = self.max_gpu_images;
        while self.path_cache.len() > capacity {
            let Some(handle) = self
                .auxiliary_last_used
                .iter()
                .filter(|(handle, _)| {
                    matches!(handle, ResourceHandle::Path(_)) && self.reference_count(**handle) == 0
                })
                .min_by_key(|(_, stamp)| *stamp)
                .map(|(handle, _)| *handle)
            else {
                break;
            };
            if let ResourceHandle::Path(key) = handle {
                self.path_cache.remove(&key);
            }
            self.auxiliary_last_used.remove(&handle);
        }
        while self.icons.len() > capacity {
            let Some(handle) = self
                .auxiliary_last_used
                .iter()
                .filter(|(handle, _)| {
                    matches!(handle, ResourceHandle::Icon(_)) && self.reference_count(**handle) == 0
                })
                .min_by_key(|(_, stamp)| *stamp)
                .map(|(handle, _)| *handle)
            else {
                break;
            };
            if let ResourceHandle::Icon(id) = handle {
                self.icons.remove(&id);
            }
            self.auxiliary_last_used.remove(&handle);
        }
    }

    pub fn image(&self, id: ImageId) -> Option<&ImageResource> {
        self.cpu.image(id)
    }

    pub fn contains_image(&self, id: ImageId) -> bool {
        self.cpu.contains_image(id)
    }

    pub fn gpu_image_count(&self) -> usize {
        self.gpu_images.len()
    }

    /// Makes an image resident and returns its atlas page plus normalized UV
    /// rectangle. Image vertices own their final UVs, allowing a single
    /// texture binding to serve every image on the page.
    fn ensure_image_atlas_slot(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        id: ImageId,
    ) -> Option<(usize, [f32; 4])> {
        if !self.gpu_images.contains_key(&id) {
            let image = self.cpu.image(id)?.clone();
            self.upload_gpu_image(device, queue, id, &image);
        }
        self.touch(id);
        let slot = self.gpu_images.get(&id)?.slot;
        Some((
            slot.page,
            [
                slot.origin.x as f32 / slot.atlas_width as f32,
                slot.origin.y as f32 / slot.atlas_height as f32,
                (slot.origin.x + slot.width) as f32 / slot.atlas_width as f32,
                (slot.origin.y + slot.height) as f32 / slot.atlas_height as f32,
            ],
        ))
    }

    fn bind_group(
        &mut self,
        window: WindowId,
        page: usize,
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
    ) -> Option<wgpu::BindGroup> {
        if !self.image_bind_groups.contains_key(&(window, page)) {
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("zui-render image atlas page bind group"),
                layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(
                            &self.image_atlases.get(page)?.view,
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.image_sampler),
                    },
                ],
            });
            self.image_bind_groups.insert((window, page), bind_group);
        }
        self.image_bind_groups.get(&(window, page)).cloned()
    }
}

fn texture_view(texture: &wgpu::Texture) -> wgpu::TextureView {
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

/// A compact affine transform used by retained render nodes.
///
/// The renderer currently consumes axis-aligned primitives, so transformed
/// rectangles are represented by their axis-aligned bounds. Keeping the
/// transform here still gives containers a stable API for translation,
/// scaling and rotation without leaking GPU details into widgets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub matrix: [f32; 6],
}

impl Transform {
    pub const IDENTITY: Self = Self {
        matrix: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
    };

    pub const fn identity() -> Self {
        Self::IDENTITY
    }

    pub const fn translate(x: Dip, y: Dip) -> Self {
        Self {
            matrix: [1.0, 0.0, 0.0, 1.0, x.0, y.0],
        }
    }

    pub const fn scale(x: f32, y: f32) -> Self {
        Self {
            matrix: [x, 0.0, 0.0, y, 0.0, 0.0],
        }
    }

    fn point(self, point: Point) -> Point {
        let [a, b, c, d, tx, ty] = self.matrix;
        Point {
            x: Dip(a * point.x.0 + c * point.y.0 + tx),
            y: Dip(b * point.x.0 + d * point.y.0 + ty),
        }
    }

    fn rect(self, rect: Rect) -> Rect {
        let corners = [
            rect.origin,
            Point {
                x: Dip(rect.origin.x.0 + rect.size.width.0),
                y: rect.origin.y,
            },
            Point {
                x: rect.origin.x,
                y: Dip(rect.origin.y.0 + rect.size.height.0),
            },
            Point {
                x: Dip(rect.origin.x.0 + rect.size.width.0),
                y: Dip(rect.origin.y.0 + rect.size.height.0),
            },
        ];
        let transformed = corners.map(|corner| self.point(corner));
        let left = transformed
            .iter()
            .map(|point| point.x.0)
            .fold(f32::INFINITY, f32::min);
        let top = transformed
            .iter()
            .map(|point| point.y.0)
            .fold(f32::INFINITY, f32::min);
        let right = transformed
            .iter()
            .map(|point| point.x.0)
            .fold(f32::NEG_INFINITY, f32::max);
        let bottom = transformed
            .iter()
            .map(|point| point.y.0)
            .fold(f32::NEG_INFINITY, f32::max);
        Rect {
            origin: Point {
                x: Dip(left),
                y: Dip(top),
            },
            size: zui_core::Size {
                width: Dip(right - left),
                height: Dip(bottom - top),
            },
        }
    }

    fn inverse(self) -> Option<Self> {
        let [a, b, c, d, tx, ty] = self.matrix;
        let determinant = a * d - b * c;
        (determinant.abs() > f32::EPSILON).then(|| Self {
            matrix: [
                d / determinant,
                -b / determinant,
                -c / determinant,
                a / determinant,
                (c * ty - d * tx) / determinant,
                (b * tx - a * ty) / determinant,
            ],
        })
    }

    /// Expresses this world-space transform in `parent_world` local space.
    /// Singular parents retain the original transform; they cannot define a
    /// meaningful local coordinate system.
    pub fn relative_to(self, parent_world: Self) -> Self {
        parent_world
            .inverse()
            .map(|inverse| compose_transform(inverse, self))
            .unwrap_or(self)
    }
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DirtyFlags(u8);

impl DirtyFlags {
    pub const LAYOUT: Self = Self(1 << 0);
    pub const PAINT: Self = Self(1 << 1);
    pub const CHILDREN: Self = Self(1 << 2);
    pub const RESOURCES: Self = Self(1 << 3);

    pub const fn empty() -> Self {
        Self(0)
    }
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// A small normalized set of invalidated rectangles. Overlapping rectangles
/// are coalesced, while distant regions remain independent so callers can
/// submit precise damage to the GPU.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DirtyRegionSet {
    regions: Vec<Rect>,
}

impl DirtyRegionSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, region: Rect) {
        let mut merged = region;
        let mut index = 0;
        while index < self.regions.len() {
            if rects_touch_or_overlap(self.regions[index], merged) {
                merged = union_rect(self.regions[index], merged);
                self.regions.remove(index);
            } else {
                index += 1;
            }
        }
        self.regions.push(merged);
    }

    pub fn extend(&mut self, regions: impl IntoIterator<Item = Rect>) {
        for region in regions {
            self.add(region);
        }
    }

    pub fn clear(&mut self) {
        self.regions.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.regions.is_empty()
    }

    pub fn as_slice(&self) -> &[Rect] {
        &self.regions
    }

    pub fn union(&self) -> Option<Rect> {
        self.regions.iter().copied().reduce(union_rect)
    }
}

/// Backend-neutral change set for a retained scene. UI frameworks map their
/// local widget identity to this stable numeric node identity before submit.
#[derive(Clone, Default)]
pub struct SceneUpdate {
    dirty_nodes: BTreeMap<RenderNodeId, DirtyRegionSet>,
    damage: DirtyRegionSet,
    full_rebuild: bool,
    /// Monotonically increasing version assigned by the UI scene owner.
    /// Zero denotes an unversioned submission for backwards-compatible
    /// renderer integrations.
    revision: u64,
}

impl SceneUpdate {
    pub fn invalidate_node(&mut self, node: RenderNodeId, region: Rect) {
        self.dirty_nodes.entry(node).or_default().add(region);
        self.damage.add(region);
    }
    pub fn request_full_rebuild(&mut self) {
        self.full_rebuild = true;
    }
    pub fn full_rebuild(&self) -> bool {
        self.full_rebuild
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn set_revision(&mut self, revision: u64) {
        self.revision = revision;
    }
    pub fn is_empty(&self) -> bool {
        !self.full_rebuild && self.dirty_nodes.is_empty() && self.damage.is_empty()
    }
    pub fn dirty_node_ids(&self) -> impl Iterator<Item = RenderNodeId> + '_ {
        self.dirty_nodes.keys().copied()
    }
    pub fn node_regions(&self) -> impl Iterator<Item = (RenderNodeId, &[Rect])> {
        self.dirty_nodes
            .iter()
            .map(|(id, regions)| (*id, regions.as_slice()))
    }
    pub fn damage_regions(&self) -> &[Rect] {
        self.damage.as_slice()
    }
    pub fn add_damage(&mut self, region: Rect) {
        self.damage.add(region);
    }
    pub fn extend_damage(&mut self, regions: impl IntoIterator<Item = Rect>) {
        self.damage.extend(regions);
    }
    pub fn merge(&mut self, mut update: Self) {
        self.full_rebuild |= update.full_rebuild;
        self.revision = self.revision.max(update.revision);
        self.damage.extend(
            std::mem::take(&mut update.damage)
                .as_slice()
                .iter()
                .copied(),
        );
        for (node, regions) in std::mem::take(&mut update.dirty_nodes) {
            self.dirty_nodes
                .entry(node)
                .or_default()
                .extend(regions.as_slice().iter().copied());
        }
    }
    pub fn clear(&mut self) {
        self.dirty_nodes.clear();
        self.damage.clear();
        self.full_rebuild = false;
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DirtyState {
    pub flags: DirtyFlags,
    pub regions: DirtyRegionSet,
}

impl DirtyState {
    pub fn is_dirty(&self) -> bool {
        !self.flags.is_empty() || !self.regions.is_empty()
    }

    pub fn clear(&mut self) {
        self.flags = DirtyFlags::empty();
        self.regions.clear();
    }
}

/// Retained intermediate representation between widgets and the renderer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RenderNode {
    /// Bounds in the node's local coordinate system.
    pub local_bounds: Rect,
    /// Stable identity of the originating widget or retained scene node.
    pub id: Option<RenderNodeId>,
    pub transform: Transform,
    /// Node-owned clip scopes. They compose with inherited clips in order.
    pub clips: Vec<ClipShape>,
    pub opacity: f32,
    /// Copy-on-write command storage. A clean retained node shares its
    /// immutable command list with the previous tree.
    pub commands: Arc<Vec<PaintCommand>>,
    /// Copy-on-write child storage. Reusing a clean subtree is therefore an
    /// Arc clone; only the dirty path detaches its child vector.
    pub children: Arc<Vec<Self>>,
    pub dirty: DirtyState,
}

#[derive(Clone, Debug)]
struct RenderNodeItem {
    node_id: Option<RenderNodeId>,
    bounds: Rect,
    transform: Transform,
    opacity: f32,
    clips: Vec<RenderClip>,
    /// Shares the retained node's immutable command list. GPU materialization
    /// consumes it by slice, so copying PaintCommands here is unnecessary.
    commands: Arc<Vec<PaintCommand>>,
}

#[derive(Clone, Debug)]
struct RenderClip {
    shape: ClipShape,
    transform: Transform,
}

/// Uniform-grid spatial index for retained render items. It is rebuilt only
/// when the retained scene changes and lets damage replay visit intersecting
/// nodes without scanning every item.
#[derive(Clone, Debug, Default)]
struct SpatialIndex {
    cell_size: f32,
    cells: HashMap<(i32, i32), Vec<Vec<usize>>>,
    bounds: BTreeMap<Vec<usize>, Rect>,
}

impl SpatialIndex {
    fn build(items: &BTreeMap<Vec<usize>, RenderNodeItem>) -> Self {
        let cell_size = 128.0_f32;
        let mut index = Self {
            cell_size,
            cells: HashMap::new(),
            bounds: BTreeMap::new(),
        };
        for (path, item) in items {
            index.insert(path.clone(), item.bounds);
        }
        index
    }

    fn insert(&mut self, path: Vec<usize>, bounds: Rect) {
        let (min_x, min_y, max_x, max_y) = self.cell_range(bounds);
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                self.cells.entry((x, y)).or_default().push(path.clone());
            }
        }
        self.bounds.insert(path, bounds);
    }

    fn update_subtrees(
        &mut self,
        items: &BTreeMap<Vec<usize>, RenderNodeItem>,
        paths: &[Vec<usize>],
    ) {
        self.bounds
            .retain(|path, _| !paths.iter().any(|prefix| path_starts_with(path, prefix)));
        self.cells.retain(|_, entries| {
            entries.retain(|path| !paths.iter().any(|prefix| path_starts_with(path, prefix)));
            !entries.is_empty()
        });
        for (path, item) in items {
            if paths.iter().any(|prefix| path_starts_with(path, prefix)) {
                self.insert(path.clone(), item.bounds);
            }
        }
    }

    /// Fast existential query used by damage coalescing. Unlike `query`, this
    /// keeps paths borrowed, does no sorting/deduplication, and stops as soon
    /// as one retained item overlaps both regions.
    fn any_intersects(&self, first: Rect, second: Rect) -> bool {
        let (min_x, min_y, max_x, max_y) = self.cell_range(first);
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                if self.cells.get(&(x, y)).is_some_and(|paths| {
                    paths.iter().any(|path| {
                        self.bounds.get(path).is_some_and(|bounds| {
                            rect_intersects(*bounds, first) && rect_intersects(*bounds, second)
                        })
                    })
                }) {
                    return true;
                }
            }
        }
        false
    }

    fn cell_range(&self, rect: Rect) -> (i32, i32, i32, i32) {
        let min_x = (rect.origin.x.0 / self.cell_size).floor() as i32;
        let min_y = (rect.origin.y.0 / self.cell_size).floor() as i32;
        let max_x = ((rect.origin.x.0 + rect.size.width.0) / self.cell_size).floor() as i32;
        let max_y = ((rect.origin.y.0 + rect.size.height.0) / self.cell_size).floor() as i32;
        (min_x, min_y, max_x, max_y)
    }
}

/// Stable identity of a retained node. Widget-backed nodes use their WidgetId;
/// paths are only an internal tree-location cache and must not be used as the
/// node's identity by renderer caches or resources.
pub type RenderNodeId = u64;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RenderNodeIndex {
    paths: HashMap<RenderNodeId, Vec<usize>>,
    ids_by_path: BTreeMap<Vec<usize>, RenderNodeId>,
}

impl RenderNodeIndex {
    pub fn path_for(&self, node_id: RenderNodeId) -> Option<&[usize]> {
        self.paths.get(&node_id).map(Vec::as_slice)
    }

    pub fn node<'a>(&self, root: &'a RenderNode, node_id: RenderNodeId) -> Option<&'a RenderNode> {
        let mut node = root;
        for index in self.path_for(node_id)? {
            node = node.children.get(*index)?;
        }
        Some(node)
    }

    /// Updates only changed subtrees after a retained-tree rebuild. Structural
    /// changes use an empty path (the root) and therefore intentionally
    /// rebuild the complete index.
    pub fn update_subtrees(&mut self, root: &RenderNode, paths: &[Vec<usize>]) {
        for prefix in paths {
            loop {
                let path = self
                    .ids_by_path
                    .range(prefix.clone()..)
                    .next()
                    .map(|(path, _)| path.clone());
                let Some(path) = path.filter(|path| path_starts_with(path, prefix)) else {
                    break;
                };
                if let Some(id) = self.ids_by_path.remove(&path) {
                    self.paths.remove(&id);
                }
            }
            let Some(node) = node_at_path(root, prefix) else {
                continue;
            };
            let mut path = prefix.clone();
            node.index_into(self, &mut path);
        }
    }
}

fn node_at_path<'a>(mut node: &'a RenderNode, path: &[usize]) -> Option<&'a RenderNode> {
    for index in path {
        node = node.children.get(*index)?;
    }
    Some(node)
}

impl RenderNode {
    pub fn new(bounds: Rect) -> Self {
        Self {
            local_bounds: bounds,
            id: None,
            transform: Transform::IDENTITY,
            clips: Vec::new(),
            opacity: 1.0,
            commands: Arc::new(Vec::new()),
            children: Arc::new(Vec::new()),
            dirty: DirtyState {
                flags: DirtyFlags::LAYOUT.union(DirtyFlags::PAINT),
                regions: DirtyRegionSet::new(),
            },
        }
    }

    pub fn for_widget(bounds: Rect) -> Self {
        let mut node = Self::new(Rect {
            origin: Point::default(),
            size: bounds.size,
        });
        node.transform = Transform::translate(bounds.origin.x, bounds.origin.y);
        node
    }

    pub fn add_child(&mut self, child: Self) {
        Arc::make_mut(&mut self.children).push(child);
        self.mark_dirty(DirtyFlags::CHILDREN);
    }

    pub fn build_index(&self) -> RenderNodeIndex {
        let mut index = RenderNodeIndex::default();
        let mut path = Vec::new();
        self.index_into(&mut index, &mut path);
        index
    }

    /// Reuses unchanged subtrees from a previous retained tree. Widgets do
    /// not participate in cache decisions; the tree builder supplies the
    /// current node and this method compares source ids to retain clean
    /// RenderNode subtrees.
    pub fn reuse_clean_subtrees(
        mut self,
        previous: Option<&Self>,
        dirty_paths: &[Vec<usize>],
        force_rebuild: bool,
    ) -> Self {
        let can_match = previous
            .map(|previous| previous.id == self.id)
            .unwrap_or(false);
        if can_match && !force_rebuild && dirty_paths.is_empty() {
            return previous.expect("previous node exists when matched").clone();
        }

        if let Some(previous) = previous.filter(|previous| previous.id == self.id) {
            let old_children = &previous.children;
            let children = std::mem::take(Arc::make_mut(&mut self.children));
            self.children = Arc::new(
                children
                    .into_iter()
                    .enumerate()
                    .map(|(index, child)| {
                        let child_paths = dirty_paths
                            .iter()
                            .filter_map(|path| {
                                (path.first().copied() == Some(index)).then(|| path[1..].to_vec())
                            })
                            .collect::<Vec<_>>();
                        child.reuse_clean_subtrees(
                            old_children.get(index),
                            &child_paths,
                            force_rebuild,
                        )
                    })
                    .collect(),
            );
        }
        self
    }

    fn index_into(&self, index: &mut RenderNodeIndex, path: &mut Vec<usize>) {
        if let Some(id) = self.id {
            index.paths.insert(id, path.clone());
            index.ids_by_path.insert(path.clone(), id);
        }
        for (child_index, child) in self.children.iter().enumerate() {
            path.push(child_index);
            child.index_into(index, path);
            path.pop();
        }
    }

    pub fn child_mut(&mut self, index: usize) -> Option<&mut Self> {
        Arc::make_mut(&mut self.children).get_mut(index)
    }

    /// Mutates this node's local paint commands, detaching only this command
    /// vector when it is shared with a clean retained subtree.
    pub fn commands_mut(&mut self) -> &mut Vec<PaintCommand> {
        Arc::make_mut(&mut self.commands)
    }

    /// Marks a descendant as dirty and records the corresponding parent
    /// invalidation. This is the explicit propagation point for retained
    /// trees whose children are stored by value.
    pub fn mark_child_dirty(&mut self, index: usize, flags: DirtyFlags, region: Option<Rect>) {
        if let Some(child) = Arc::make_mut(&mut self.children).get_mut(index) {
            match region {
                Some(region) => child.mark_dirty_region(region),
                None => child.mark_dirty(flags),
            }
            self.mark_dirty(DirtyFlags::CHILDREN);
            if let Some(region) = region {
                self.mark_dirty_region(region);
            }
        }
    }

    /// Propagates an invalidation from a Widget path to the corresponding
    /// retained node and all of its ancestors. The path is relative to this
    /// node and follows child indices in the RenderNode tree.
    pub fn mark_dirty_path(&mut self, path: &[usize], flags: DirtyFlags, region: Option<Rect>) {
        if path.is_empty() {
            self.mark_dirty(flags);
            if let Some(region) = region {
                self.mark_dirty_region(region);
            }
            return;
        }

        self.mark_dirty(DirtyFlags::CHILDREN);
        if let Some(region) = region {
            self.mark_dirty_region(region);
        }
        if let Some(child) = Arc::make_mut(&mut self.children).get_mut(path[0]) {
            child.mark_dirty_path(&path[1..], flags, region);
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty.is_dirty() || self.children.iter().any(Self::is_dirty)
    }

    pub fn accumulated_dirty_region(&self) -> Option<Rect> {
        let mut region = self.dirty.regions.union();
        for child in self.children.iter() {
            if let Some(child_region) = child.accumulated_dirty_region() {
                region = Some(match region {
                    Some(region) => union_rect(region, child_region),
                    None => child_region,
                });
            }
        }
        region
    }

    pub fn set_transform(&mut self, transform: Transform) {
        self.transform = transform;
        self.mark_dirty(DirtyFlags::PAINT);
    }

    pub fn set_id(&mut self, id: RenderNodeId) {
        self.id = Some(id);
    }

    /// Converts a world-space placement emitted by layout into a transform
    /// relative to `parent_world`. Rendering stores only the resulting local
    /// transform; this conversion is performed exactly once while building a
    /// node, never as a post-build tree walk.
    pub fn localize_to_parent(&mut self, parent_world: Transform) {
        self.transform = self.transform.relative_to(parent_world);
    }

    pub fn world_bounds(&self) -> Rect {
        self.transform.rect(self.local_bounds)
    }

    pub fn set_clip(&mut self, clip: Option<ClipShape>) {
        self.clips = clip.into_iter().collect();
        self.mark_dirty(DirtyFlags::PAINT);
    }

    pub fn push_clip(&mut self, clip: ClipShape) {
        self.clips.push(clip);
        self.mark_dirty(DirtyFlags::PAINT);
    }

    pub fn clear_clips(&mut self) {
        self.clips.clear();
        self.mark_dirty(DirtyFlags::PAINT);
    }

    pub fn set_opacity(&mut self, opacity: f32) {
        self.opacity = opacity.clamp(0.0, 1.0);
        self.mark_dirty(DirtyFlags::PAINT);
    }

    pub fn mark_dirty(&mut self, flags: DirtyFlags) {
        self.dirty.flags = self.dirty.flags.union(flags);
    }

    pub fn mark_dirty_region(&mut self, region: Rect) {
        self.mark_dirty(DirtyFlags::PAINT);
        self.dirty.regions.add(region);
    }

    pub fn clear_dirty(&mut self) {
        self.dirty.clear();
        for child in Arc::make_mut(&mut self.children) {
            child.clear_dirty();
        }
    }
}

fn compose_transform(parent: Transform, child: Transform) -> Transform {
    // General affine multiplication: parent * child.
    let [a, b, c, d, tx, ty] = parent.matrix;
    let [e, f, g, h, ux, uy] = child.matrix;
    Transform {
        matrix: [
            a * e + c * f,
            b * e + d * f,
            a * g + c * h,
            b * g + d * h,
            a * ux + c * uy + tx,
            b * ux + d * uy + ty,
        ],
    }
}

fn intersect_clip(parent: Option<Rect>, child: Option<Rect>) -> Option<Rect> {
    match (parent, child) {
        (Some(a), Some(b)) => {
            let left = a.origin.x.0.max(b.origin.x.0);
            let top = a.origin.y.0.max(b.origin.y.0);
            let right = (a.origin.x.0 + a.size.width.0).min(b.origin.x.0 + b.size.width.0);
            let bottom = (a.origin.y.0 + a.size.height.0).min(b.origin.y.0 + b.size.height.0);
            Some(Rect {
                origin: Point {
                    x: Dip(left),
                    y: Dip(top),
                },
                size: zui_core::Size {
                    width: Dip((right - left).max(0.0)),
                    height: Dip((bottom - top).max(0.0)),
                },
            })
        }
        (clip, None) | (None, clip) => clip,
    }
}

fn intersect_rect(a: Rect, b: Rect) -> Rect {
    let left = a.origin.x.0.max(b.origin.x.0);
    let top = a.origin.y.0.max(b.origin.y.0);
    let right = (a.origin.x.0 + a.size.width.0).min(b.origin.x.0 + b.size.width.0);
    let bottom = (a.origin.y.0 + a.size.height.0).min(b.origin.y.0 + b.size.height.0);
    Rect {
        origin: Point {
            x: Dip(left),
            y: Dip(top),
        },
        size: zui_core::Size {
            width: Dip((right - left).max(0.0)),
            height: Dip((bottom - top).max(0.0)),
        },
    }
}

fn apply_opacity(mut color: Color, opacity: f32) -> Color {
    color.a *= opacity;
    color
}

fn rect_intersects(a: Rect, b: Rect) -> bool {
    a.origin.x.0 < b.origin.x.0 + b.size.width.0
        && a.origin.x.0 + a.size.width.0 > b.origin.x.0
        && a.origin.y.0 < b.origin.y.0 + b.size.height.0
        && a.origin.y.0 + a.size.height.0 > b.origin.y.0
}

fn rects_touch_or_overlap(a: Rect, b: Rect) -> bool {
    let a_right = a.origin.x.0 + a.size.width.0;
    let a_bottom = a.origin.y.0 + a.size.height.0;
    let b_right = b.origin.x.0 + b.size.width.0;
    let b_bottom = b.origin.y.0 + b.size.height.0;
    a.origin.x.0 <= b_right
        && b.origin.x.0 <= a_right
        && a.origin.y.0 <= b_bottom
        && b.origin.y.0 <= a_bottom
}

fn build_render_item(
    node: &RenderNode,
    parent_transform: Transform,
    parent_clip: Option<Rect>,
    parent_opacity: f32,
    parent_clips: &[RenderClip],
) -> (
    RenderNodeItem,
    Transform,
    Option<Rect>,
    f32,
    Vec<RenderClip>,
) {
    let transform = compose_transform(parent_transform, node.transform);
    let mut clip = parent_clip;
    let opacity = parent_opacity * node.opacity;
    let mut clips = parent_clips.to_vec();
    for shape in &node.clips {
        clip = intersect_clip(clip, Some(transform_clip_shape(shape, transform).bounds()));
        clips.push(RenderClip {
            shape: shape.clone(),
            transform,
        });
    }
    let inherited_clips = clips.clone();
    let item = RenderNodeItem {
        node_id: node.id,
        bounds: clip
            .map(|clip| intersect_rect(transform.rect(node.local_bounds), clip))
            .unwrap_or_else(|| transform.rect(node.local_bounds)),
        transform,
        opacity,
        clips,
        // Commands remain in the node's local coordinate system. The
        // renderer resolves them only while producing GPU vertices.
        commands: Arc::clone(&node.commands),
    };
    (item, transform, clip, opacity, inherited_clips)
}

fn path_starts_with(path: &[usize], prefix: &[usize]) -> bool {
    path.len() >= prefix.len() && path[..prefix.len()] == *prefix
}

fn remove_cached_subtree(cache: &mut BTreeMap<Vec<usize>, RenderNodeItem>, path: &[usize]) {
    cache.retain(|cached_path, _| !path_starts_with(cached_path, path));
}

fn update_retained_subtree(
    node: &RenderNode,
    path: &mut Vec<usize>,
    cache: &mut BTreeMap<Vec<usize>, RenderNodeItem>,
    parent_transform: Transform,
    parent_clip: Option<Rect>,
    parent_opacity: f32,
    parent_clips: &[RenderClip],
    force_rebuild: bool,
    changed_paths: &mut Vec<Vec<usize>>,
) {
    let state_change = node.dirty.flags.contains(DirtyFlags::PAINT)
        || node.dirty.flags.contains(DirtyFlags::LAYOUT)
        || node.dirty.flags.contains(DirtyFlags::RESOURCES);
    let rebuild_subtree = force_rebuild || state_change || !cache.contains_key(path);
    if rebuild_subtree {
        remove_cached_subtree(cache, path);
    }

    let (item, transform, clip, opacity, clips) = build_render_item(
        node,
        parent_transform,
        parent_clip,
        parent_opacity,
        parent_clips,
    );
    if rebuild_subtree {
        cache.insert(path.clone(), item);
        changed_paths.push(path.clone());
    }

    // Remove cached children which no longer exist after a structural update.
    cache.retain(|cached_path, _| {
        !(cached_path.len() > path.len()
            && path_starts_with(cached_path, path)
            && cached_path[path.len()] >= node.children.len())
    });

    for (index, child) in node.children.iter().enumerate() {
        path.push(index);
        if rebuild_subtree || child.dirty.is_dirty() || !cache.contains_key(path) {
            update_retained_subtree(
                child,
                path,
                cache,
                transform,
                clip,
                opacity,
                &clips,
                rebuild_subtree,
                changed_paths,
            );
        }
        path.pop();
    }
}

/// Rebuilds exactly one dirty RenderNode subtree. Parent state is accumulated
/// only along the supplied path, so a WidgetId invalidation does not require a
/// walk over unrelated retained siblings.
fn update_retained_dirty_path(
    root: &RenderNode,
    dirty_path: &[usize],
    cache: &mut BTreeMap<Vec<usize>, RenderNodeItem>,
    changed_paths: &mut Vec<Vec<usize>>,
) {
    let mut node = root;
    let mut path = Vec::new();
    let mut transform = Transform::IDENTITY;
    let mut clip = None;
    let mut opacity = 1.0;
    let mut clips = Vec::new();
    for child_index in dirty_path {
        let (_, next_transform, next_clip, next_opacity, next_clips) =
            build_render_item(node, transform, clip, opacity, &clips);
        let Some(child) = node.children.get(*child_index) else {
            return;
        };
        path.push(*child_index);
        node = child;
        transform = next_transform;
        clip = next_clip;
        opacity = next_opacity;
        clips = next_clips;
    }
    update_retained_subtree(
        node,
        &mut path,
        cache,
        transform,
        clip,
        opacity,
        &clips,
        false,
        changed_paths,
    );
}

fn coalesce_damage_for_spatial_index(regions: &[Rect], index: Option<&SpatialIndex>) -> Vec<Rect> {
    let mut result = regions.to_vec();
    let mut changed = true;
    while changed {
        changed = false;
        'outer: for left in 0..result.len() {
            for right in (left + 1)..result.len() {
                if index
                    .map(|index| index.any_intersects(result[left], result[right]))
                    .unwrap_or(false)
                {
                    let merged = union_rect(result[left], result[right]);
                    result[left] = merged;
                    result.remove(right);
                    changed = true;
                    break 'outer;
                }
            }
        }
    }
    result
}

fn union_rect(a: Rect, b: Rect) -> Rect {
    let left = a.origin.x.0.min(b.origin.x.0);
    let top = a.origin.y.0.min(b.origin.y.0);
    let right = (a.origin.x.0 + a.size.width.0).max(b.origin.x.0 + b.size.width.0);
    let bottom = (a.origin.y.0 + a.size.height.0).max(b.origin.y.0 + b.size.height.0);
    Rect {
        origin: Point {
            x: Dip(left),
            y: Dip(top),
        },
        size: zui_core::Size {
            width: Dip(right - left),
            height: Dip(bottom - top),
        },
    }
}

pub struct RenderNodeBuilder {
    node: RenderNode,
}

impl RenderNodeBuilder {
    pub fn new(bounds: Rect) -> Self {
        Self {
            node: RenderNode::new(bounds),
        }
    }

    pub fn for_widget(bounds: Rect) -> Self {
        let mut builder = Self::new(Rect {
            origin: Point::default(),
            size: bounds.size,
        });
        builder.transform(Transform::translate(bounds.origin.x, bounds.origin.y));
        builder
    }

    pub fn commands_mut(&mut self) -> &mut Vec<PaintCommand> {
        Arc::make_mut(&mut self.node.commands)
    }

    pub fn add_child(&mut self, child: RenderNode) {
        self.node.add_child(child);
    }

    pub fn transform(&mut self, transform: Transform) -> &mut Self {
        self.node.transform = transform;
        self
    }

    pub fn id(&mut self, id: RenderNodeId) -> &mut Self {
        self.node.set_id(id);
        self
    }

    pub fn clip(&mut self, clip: Option<ClipShape>) -> &mut Self {
        self.node.clips = clip.into_iter().collect();
        self
    }

    pub fn opacity(&mut self, opacity: f32) -> &mut Self {
        self.node.opacity = opacity.clamp(0.0, 1.0);
        self
    }

    pub fn finish(self) -> RenderNode {
        let mut node = self.node;
        node.clear_dirty();
        node
    }
}

struct SurfaceState {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    size: PhysicalSize,
    pipeline: wgpu::RenderPipeline,
    rounded_pipeline: wgpu::RenderPipeline,
    stencil_rounded_pipeline: wgpu::RenderPipeline,
    line_pipeline: wgpu::RenderPipeline,
    image_pipeline: wgpu::RenderPipeline,
    /// Immutable atlas-page material layout. Keeping it on the surface avoids
    /// querying the pipeline for every image draw during replay.
    image_bind_group_layout: wgpu::BindGroupLayout,
    stencil_pipeline: wgpu::RenderPipeline,
    stencil_mask_pipeline: wgpu::RenderPipeline,
    /// Per-transform uniform resources. A draw must never share a mutable
    /// uniform with a later draw in the same command buffer.
    transform_bind_group_layout: wgpu::BindGroupLayout,
    transform_bindings: HashMap<[u32; 6], TransformBinding>,
    transform_binding_clock: u64,
    max_transform_bindings: usize,
    canvas: wgpu::Texture,
    canvas_view: wgpu::TextureView,
    stencil: wgpu::Texture,
    stencil_view: wgpu::TextureView,
    stencil_reset: wgpu::Buffer,
    /// An overwrite-only pipeline and material used to clear one scissored
    /// persistent-canvas tile before retained batches are replayed.
    damage_clear_pipeline: wgpu::RenderPipeline,
    damage_clear_color: wgpu::Buffer,
    damage_clear_bind_group: wgpu::BindGroup,
    blit_pipeline: wgpu::RenderPipeline,
    blit_bind_group: wgpu::BindGroup,
    /// Whether the swap-chain texture can be the destination of a GPU copy.
    /// When available, presentation uses a texture copy instead of a
    /// full-screen fragment composite.
    direct_copy_present: bool,
    scale_factor: ScaleFactor,
    has_contents: bool,
    /// Retained renderer items. A clean RenderNode reuses this ordered list
    /// without walking the widget/render tree again.
    retained_items: Option<BTreeMap<Vec<usize>, RenderNodeItem>>,
    /// GPU draw data keyed by retained-node path. Only nodes whose render key
    /// changes rebuild these batches; partial replay reads this table directly.
    retained_gpu_items: Option<BTreeMap<Vec<usize>, RetainedGpuItem>>,
    /// Ordered GPU submission segments keyed by RenderNode path. Replacing a
    /// dirty subtree updates only the affected path range; clean and partial
    /// frames borrow these segments directly instead of rebuilding a global
    /// submission vector.
    retained_submission_batches: Option<RetainedSubmissionTable>,
    /// Long-lived vertex storage for retained draw batches. Dirty nodes replace
    /// only their allocated ranges instead of allocating a buffer per batch.
    vertex_arenas: VertexArenas,
    index_arena: IndexArena,
    indirect_arena: IndirectArena,
    composition_tiles: CompositionTiles,
    spatial_index: Option<SpatialIndex>,
    tile_submission_index: Option<TileSubmissionIndex>,
    /// The last version of the UI-owned scene successfully materialized for
    /// this surface. A missing or non-sequential version means incremental
    /// caches cannot be trusted and must be rebuilt from the submitted root.
    submitted_scene_revision: Option<u64>,
    last_frame_stats: FrameStats,
}

/// The persistent canvas is composed in fixed physical tiles. A dirty widget
/// therefore invalidates a stable GPU layer unit rather than an arbitrary
/// floating-point rectangle; adjacent damage naturally coalesces before replay.
struct CompositionTiles {
    size: PhysicalSize,
    tile_size: u32,
}

impl CompositionTiles {
    const TILE_SIZE: u32 = 128;

    fn new(size: PhysicalSize) -> Self {
        Self {
            size,
            tile_size: Self::TILE_SIZE,
        }
    }

    fn regions(&self, damage: &[Rect], scale_factor: ScaleFactor, full: bool) -> Vec<Rect> {
        let scale = scale_factor.0.max(1.0) as f32;
        if full {
            return vec![Rect {
                origin: Point::default(),
                size: Size {
                    width: Dip(self.size.width.max(1) as f32 / scale),
                    height: Dip(self.size.height.max(1) as f32 / scale),
                },
            }];
        }
        let mut tiles = BTreeSet::new();
        for rect in damage {
            let left = (rect.origin.x.0 * scale).floor().max(0.0) as u32;
            let top = (rect.origin.y.0 * scale).floor().max(0.0) as u32;
            let right = ((rect.origin.x.0 + rect.size.width.0) * scale)
                .ceil()
                .max(left as f32) as u32;
            let bottom = ((rect.origin.y.0 + rect.size.height.0) * scale)
                .ceil()
                .max(top as f32) as u32;
            let x_start = left / self.tile_size;
            let x_end = right.min(self.size.width).div_ceil(self.tile_size);
            let y_start = top / self.tile_size;
            let y_end = bottom.min(self.size.height).div_ceil(self.tile_size);
            for y in y_start..y_end {
                for x in x_start..x_end {
                    tiles.insert((x, y));
                }
            }
        }
        tiles
            .into_iter()
            .map(|(x, y)| {
                let left = x * self.tile_size;
                let top = y * self.tile_size;
                Rect {
                    origin: Point {
                        x: Dip(left as f32 / scale),
                        y: Dip(top as f32 / scale),
                    },
                    size: Size {
                        width: Dip((self.size.width.saturating_sub(left).min(self.tile_size))
                            as f32
                            / scale),
                        height: Dip((self.size.height.saturating_sub(top).min(self.tile_size))
                            as f32
                            / scale),
                    },
                }
            })
            .collect()
    }
}

/// Persistent tile-to-retained-node lookup. Unlike the generic spatial index,
/// this index uses the exact composition tile grid and is updated by changed
/// RenderNode subtrees, so partial replay starts from submission candidates
/// without rebuilding a query result from every overlapping spatial cell.
#[derive(Clone, Debug, Default)]
struct TileSubmissionIndex {
    tile_size: f32,
    tiles: HashMap<(i32, i32), Vec<Vec<usize>>>,
    path_tiles: BTreeMap<Vec<usize>, Vec<(i32, i32)>>,
}

impl TileSubmissionIndex {
    fn build(items: &BTreeMap<Vec<usize>, RenderNodeItem>, scale_factor: ScaleFactor) -> Self {
        let mut index = Self {
            tile_size: CompositionTiles::TILE_SIZE as f32 / scale_factor.0.max(1.0) as f32,
            ..Self::default()
        };
        for (path, item) in items {
            index.insert(path.clone(), item.bounds);
        }
        index
    }

    fn insert(&mut self, path: Vec<usize>, bounds: Rect) {
        let tiles = self.tiles_for(bounds);
        for tile in &tiles {
            self.tiles.entry(*tile).or_default().push(path.clone());
        }
        self.path_tiles.insert(path, tiles);
    }

    fn update_subtrees(
        &mut self,
        items: &BTreeMap<Vec<usize>, RenderNodeItem>,
        paths: &[Vec<usize>],
    ) {
        // Reinserted paths land at the end of each tile vector. Track only
        // affected tiles and restore RenderNode-path order before replay:
        // retained tree order is the paint order.
        let mut touched_tiles = BTreeSet::new();
        for prefix in paths {
            loop {
                let path = self
                    .path_tiles
                    .range(prefix.clone()..)
                    .next()
                    .map(|(path, _)| path.clone());
                let Some(path) = path.filter(|path| path_starts_with(path, prefix)) else {
                    break;
                };
                if let Some(tiles) = self.path_tiles.remove(&path) {
                    for tile in tiles {
                        touched_tiles.insert(tile);
                        if let Some(entries) = self.tiles.get_mut(&tile) {
                            entries.retain(|entry| entry != &path);
                            if entries.is_empty() {
                                self.tiles.remove(&tile);
                            }
                        }
                    }
                }
            }
            for (path, item) in items.range(prefix.clone()..) {
                if !path_starts_with(path, prefix) {
                    break;
                }
                let tiles = self.tiles_for(item.bounds);
                for tile in &tiles {
                    touched_tiles.insert(*tile);
                    self.tiles.entry(*tile).or_default().push(path.clone());
                }
                self.path_tiles.insert(path.clone(), tiles);
            }
        }
        for tile in touched_tiles {
            if let Some(paths) = self.tiles.get_mut(&tile) {
                paths.sort_unstable();
                paths.dedup();
            }
        }
    }

    #[cfg(test)]
    fn query(&self, region: Rect) -> Vec<Vec<usize>> {
        let mut result = Vec::new();
        self.for_each_borrowed(region, |path| result.push(path.clone()));
        result
    }

    /// Visits canonical retained paths without copying path vectors. A damage
    /// region normally maps to exactly one composition tile; that common path
    /// traverses the persistent tile slice directly and performs no temporary
    /// allocation. Multi-tile regions retain ordered de-duplication.
    fn for_each_borrowed(&self, region: Rect, mut visit: impl FnMut(&Vec<usize>)) -> usize {
        let tiles = self.tiles_for(region);
        if let [tile] = tiles.as_slice() {
            if let Some(paths) = self.tiles.get(tile) {
                for path in paths {
                    visit(path);
                }
                return paths.len();
            }
            return 0;
        }
        let mut seen = BTreeSet::new();
        for tile in tiles {
            if let Some(paths) = self.tiles.get(&tile) {
                seen.extend(paths.iter());
            }
        }
        let count = seen.len();
        for path in seen {
            visit(path);
        }
        count
    }

    fn tiles_for(&self, rect: Rect) -> Vec<(i32, i32)> {
        let tile_size = self.tile_size.max(1.0);
        let min_x = (rect.origin.x.0 / tile_size).floor() as i32;
        let min_y = (rect.origin.y.0 / tile_size).floor() as i32;
        let max_x = ((rect.origin.x.0 + rect.size.width.0) / tile_size).floor() as i32;
        let max_y = ((rect.origin.y.0 + rect.size.height.0) / tile_size).floor() as i32;
        let mut tiles = Vec::new();
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                tiles.push((x, y));
            }
        }
        tiles
    }
}

#[derive(Clone)]
struct RetainedGpuItem {
    /// Stable retained-node identity. The path is only used to preserve tree
    /// paint order and to locate a subtree during a structural edit.
    node_id: Option<RenderNodeId>,
    bounds: Rect,
    batches: Vec<GpuBatch>,
    /// Allocations are owned by this retained node and released when its
    /// subtree is invalidated.
    vertex_allocations: Vec<VertexArenaAllocation>,
    index_allocations: Vec<IndexArenaAllocation>,
    indirect_allocations: Vec<IndirectArenaAllocation>,
    resources: Vec<ResourceHandle>,
}

/// Persistent, path-ordered GPU submission table.
///
/// The table deliberately owns no `GpuBatch`. GPU allocations belong solely
/// to `RetainedGpuItem`, so replacing a dirty subtree cannot leave a second
/// cloned submission list keeping old arena ranges alive. Paths provide paint
/// order; `node_id` is the stable lookup key exposed to the retained system.
#[derive(Default)]
struct RetainedSubmissionTable {
    segments: BTreeMap<Vec<usize>, RetainedSubmissionEntry>,
}

#[derive(Clone, Copy)]
struct RetainedSubmissionEntry {
    node_id: Option<RenderNodeId>,
    has_clip: bool,
}

impl RetainedSubmissionTable {
    fn rebuild(items: &BTreeMap<Vec<usize>, RetainedGpuItem>) -> Self {
        let mut table = Self::default();
        table.replace_subtrees(items, &[Vec::new()]);
        table
    }

    fn replace_subtrees(
        &mut self,
        items: &BTreeMap<Vec<usize>, RetainedGpuItem>,
        paths: &[Vec<usize>],
    ) {
        for prefix in paths {
            loop {
                let path = self
                    .segments
                    .range(prefix.clone()..)
                    .next()
                    .map(|(path, _)| path.clone());
                let Some(path) = path.filter(|path| path_starts_with(path, prefix)) else {
                    break;
                };
                self.segments.remove(&path);
            }
            for (path, item) in items.range(prefix.clone()..) {
                if !path_starts_with(path, prefix) {
                    break;
                }
                self.segments.insert(
                    path.clone(),
                    RetainedSubmissionEntry {
                        node_id: item.node_id,
                        has_clip: item
                            .batches
                            .iter()
                            .any(|batch| matches!(batch, GpuBatch::Clip(_, _, _))),
                    },
                );
            }
        }
    }

    fn for_each_entry(&self, mut visit: impl FnMut(&[usize], RetainedSubmissionEntry)) {
        for (path, entry) in &self.segments {
            visit(path, *entry);
        }
    }
}

/// Hashes path structure without formatting or allocation. The key is shared
/// by path tessellation and retained resource accounting, so equivalent paths
/// always address the same cache entry.
fn path_cache_key(path: &IconPath) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.fill_rule.hash(&mut hasher);
    path.commands.len().hash(&mut hasher);
    for command in &path.commands {
        match command {
            PathCommand::MoveTo(point) => {
                0_u8.hash(&mut hasher);
                hash_path_point(&mut hasher, *point);
            }
            PathCommand::LineTo(point) => {
                1_u8.hash(&mut hasher);
                hash_path_point(&mut hasher, *point);
            }
            PathCommand::QuadTo { control, to } => {
                2_u8.hash(&mut hasher);
                hash_path_point(&mut hasher, *control);
                hash_path_point(&mut hasher, *to);
            }
            PathCommand::CubicTo {
                control1,
                control2,
                to,
            } => {
                3_u8.hash(&mut hasher);
                hash_path_point(&mut hasher, *control1);
                hash_path_point(&mut hasher, *control2);
                hash_path_point(&mut hasher, *to);
            }
            PathCommand::Close => 4_u8.hash(&mut hasher),
        }
    }
    hasher.finish()
}

fn hash_path_point(hasher: &mut impl Hasher, point: Point) {
    point.x.0.to_bits().hash(hasher);
    point.y.0.to_bits().hash(hasher);
}

fn path_resource_handle(path: &IconPath) -> ResourceHandle {
    ResourceHandle::Path(path_cache_key(path))
}

fn item_resource_handles(item: &RenderNodeItem) -> Vec<ResourceHandle> {
    let mut handles = Vec::new();
    let mut add = |handle| {
        if !handles.contains(&handle) {
            handles.push(handle);
        }
    };
    for command in item.commands.iter() {
        match command {
            PaintCommand::Image { image, .. } => add(ResourceHandle::Image(*image)),
            PaintCommand::Icon { path, .. }
            | PaintCommand::PathFill { path, .. }
            | PaintCommand::PathStroke { path, .. } => add(path_resource_handle(path)),
            _ => {}
        }
    }
    for clip in &item.clips {
        if let ClipShape::Path { path } = &clip.shape {
            add(path_resource_handle(path));
        }
    }
    handles
}

fn batch_resource_handles(batches: &[GpuBatch]) -> Vec<ResourceHandle> {
    let mut handles = Vec::new();
    let mut add = |handle| {
        if !handles.contains(&handle) {
            handles.push(handle);
        }
    };
    for batch in batches {
        match batch {
            GpuBatch::Draw(BatchKind::Image { images, .. }, _, _, _)
            | GpuBatch::DrawGroup(BatchKind::Image { images, .. }, _, _, _) => {
                for image in images.iter().copied() {
                    add(ResourceHandle::Image(image));
                }
            }
            _ => {}
        }
    }
    handles
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VertexArenaKind {
    Rect,
    Rounded,
    Line,
    Image,
}

#[derive(Clone, Debug)]
struct VertexArenaAllocation {
    kind: VertexArenaKind,
    page: usize,
    range: Range<u64>,
    vertex_count: u32,
}

#[derive(Clone, Debug)]
struct IndexArenaAllocation {
    page: usize,
    range: Range<u64>,
    index_count: u32,
}

struct VertexArenaPage {
    buffer: wgpu::Buffer,
    capacity: u64,
    used: u64,
    free: Vec<Range<u64>>,
}

struct VertexArena {
    label: &'static str,
    pages: Vec<VertexArenaPage>,
}

struct IndexArenaPage {
    buffer: wgpu::Buffer,
    capacity: u64,
    used: u64,
    free: Vec<Range<u64>>,
}

struct IndexArena {
    pages: Vec<IndexArenaPage>,
}

/// A reusable command buffer for ordered indexed indirect draws. Unlike the
/// vertex/index arenas, commands are frame-local, but its GPU allocation is
/// intentionally persistent so a dirty frame never creates per-batch buffers.
struct IndirectArena {
    buffer: wgpu::Buffer,
    capacity: u64,
    used: u64,
    free: Vec<Range<u64>>,
    needs_reupload: bool,
}

#[derive(Clone, Debug)]
struct IndirectArenaAllocation {
    range: Range<u64>,
}

/// Inserts a released GPU range and coalesces adjacent blocks immediately.
/// Arena allocations are naturally aligned, so this keeps long-running UI
/// sessions from accumulating unusable small holes after widget rebuilds.
fn release_arena_range(free: &mut Vec<Range<u64>>, range: Range<u64>) {
    if range.is_empty() {
        return;
    }
    let insertion = free.partition_point(|entry| entry.start < range.start);
    free.insert(insertion, range);
    let mut merged: Vec<Range<u64>> = Vec::with_capacity(free.len());
    for range in free.drain(..) {
        if let Some(previous) = merged.last_mut() {
            if range.start <= previous.end {
                previous.end = previous.end.max(range.end);
                continue;
            }
        }
        merged.push(range);
    }
    *free = merged;
}

impl IndirectArena {
    const INITIAL_CAPACITY: u64 = 16 * 1024;

    fn new(device: &wgpu::Device) -> Self {
        Self {
            buffer: Self::create_buffer(device, Self::INITIAL_CAPACITY),
            capacity: Self::INITIAL_CAPACITY,
            used: 0,
            free: Vec::new(),
            needs_reupload: false,
        }
    }

    fn create_buffer(device: &wgpu::Device, capacity: u64) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("zui-render indexed indirect arena"),
            size: capacity,
            usage: wgpu::BufferUsages::INDIRECT | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    fn allocate(&mut self, device: &wgpu::Device, bytes: u64) -> IndirectArenaAllocation {
        let bytes = bytes.max(4).next_multiple_of(4);
        if let Some(index) = self
            .free
            .iter()
            .position(|range| range.end.saturating_sub(range.start) >= bytes)
        {
            let range = self.free[index].start..self.free[index].start + bytes;
            self.free[index].start += bytes;
            if self.free[index].is_empty() {
                self.free.swap_remove(index);
            }
            return IndirectArenaAllocation { range };
        }
        if self.capacity.saturating_sub(self.used) >= bytes {
            let range = self.used..self.used + bytes;
            self.used += bytes;
            return IndirectArenaAllocation { range };
        }
        self.capacity = (self.used + bytes)
            .next_power_of_two()
            .max(Self::INITIAL_CAPACITY);
        self.buffer = Self::create_buffer(device, self.capacity);
        // Reallocating the backing buffer invalidates prior commands. This
        // only happens when the retained command arena grows; callers rebuild
        // all currently-live ranges before submitting the next frame.
        self.needs_reupload = true;
        let range = self.used..self.used + bytes;
        self.used += bytes;
        IndirectArenaAllocation { range }
    }

    fn release(&mut self, allocation: IndirectArenaAllocation) {
        release_arena_range(&mut self.free, allocation.range);
    }
}

struct VertexArenas {
    rect: VertexArena,
    rounded: VertexArena,
    line: VertexArena,
    image: VertexArena,
}

impl VertexArenas {
    fn new(device: &wgpu::Device) -> Self {
        Self {
            rect: VertexArena::new(device, "zui-render retained rect arena"),
            rounded: VertexArena::new(device, "zui-render retained rounded arena"),
            line: VertexArena::new(device, "zui-render retained line arena"),
            image: VertexArena::new(device, "zui-render retained image arena"),
        }
    }

    fn arena_mut(&mut self, kind: VertexArenaKind) -> &mut VertexArena {
        match kind {
            VertexArenaKind::Rect => &mut self.rect,
            VertexArenaKind::Rounded => &mut self.rounded,
            VertexArenaKind::Line => &mut self.line,
            VertexArenaKind::Image => &mut self.image,
        }
    }

    fn buffer(&self, allocation: &VertexArenaAllocation) -> &wgpu::Buffer {
        let arena = match allocation.kind {
            VertexArenaKind::Rect => &self.rect,
            VertexArenaKind::Rounded => &self.rounded,
            VertexArenaKind::Line => &self.line,
            VertexArenaKind::Image => &self.image,
        };
        &arena.pages[allocation.page].buffer
    }

    fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        kind: VertexArenaKind,
        bytes: &[u8],
        vertex_count: u32,
    ) -> VertexArenaAllocation {
        let allocation = self
            .arena_mut(kind)
            .allocate(device, kind, bytes.len() as u64);
        queue.write_buffer(self.buffer(&allocation), allocation.range.start, bytes);
        VertexArenaAllocation {
            vertex_count,
            ..allocation
        }
    }

    fn release(&mut self, allocation: VertexArenaAllocation) {
        self.arena_mut(allocation.kind).release(allocation);
    }
}

impl VertexArena {
    const INITIAL_PAGE_SIZE: u64 = 256 * 1024;

    fn new(device: &wgpu::Device, label: &'static str) -> Self {
        Self {
            label,
            pages: vec![Self::page(device, label, Self::INITIAL_PAGE_SIZE)],
        }
    }

    fn page(device: &wgpu::Device, label: &'static str, capacity: u64) -> VertexArenaPage {
        VertexArenaPage {
            buffer: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: capacity,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            capacity,
            used: 0,
            free: Vec::new(),
        }
    }

    fn allocate(
        &mut self,
        device: &wgpu::Device,
        kind: VertexArenaKind,
        bytes: u64,
    ) -> VertexArenaAllocation {
        let bytes = bytes.max(4).next_multiple_of(4);
        for (page, entry) in self.pages.iter_mut().enumerate() {
            if let Some(index) = entry
                .free
                .iter()
                .position(|range| range.end - range.start >= bytes)
            {
                let range = entry.free[index].start..entry.free[index].start + bytes;
                entry.free[index].start += bytes;
                if entry.free[index].is_empty() {
                    entry.free.swap_remove(index);
                }
                return VertexArenaAllocation {
                    kind,
                    page,
                    range,
                    vertex_count: 0,
                };
            }
            if entry.capacity - entry.used >= bytes {
                let range = entry.used..entry.used + bytes;
                entry.used += bytes;
                return VertexArenaAllocation {
                    kind,
                    page,
                    range,
                    vertex_count: 0,
                };
            }
        }
        let capacity = self
            .pages
            .last()
            .map(|page| page.capacity.saturating_mul(2).max(bytes))
            .unwrap_or(Self::INITIAL_PAGE_SIZE)
            .max(Self::INITIAL_PAGE_SIZE);
        self.pages.push(Self::page(device, self.label, capacity));
        let page = self.pages.len() - 1;
        self.pages[page].used = bytes;
        VertexArenaAllocation {
            kind,
            page,
            range: 0..bytes,
            vertex_count: 0,
        }
    }

    fn release(&mut self, allocation: VertexArenaAllocation) {
        let page = &mut self.pages[allocation.page];
        release_arena_range(&mut page.free, allocation.range);
    }
}

impl IndexArena {
    const INITIAL_PAGE_SIZE: u64 = 256 * 1024;

    fn new(device: &wgpu::Device) -> Self {
        Self {
            pages: vec![Self::page(device, Self::INITIAL_PAGE_SIZE)],
        }
    }

    fn page(device: &wgpu::Device, capacity: u64) -> IndexArenaPage {
        IndexArenaPage {
            buffer: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("zui-render retained index arena"),
                size: capacity,
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            capacity,
            used: 0,
            free: Vec::new(),
        }
    }

    fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        indices: &[u32],
    ) -> IndexArenaAllocation {
        let bytes = bytemuck::cast_slice(indices);
        let allocation = self.allocate(device, bytes.len() as u64);
        queue.write_buffer(
            &self.pages[allocation.page].buffer,
            allocation.range.start,
            bytes,
        );
        IndexArenaAllocation {
            index_count: indices.len() as u32,
            ..allocation
        }
    }

    fn buffer(&self, allocation: &IndexArenaAllocation) -> &wgpu::Buffer {
        &self.pages[allocation.page].buffer
    }

    fn allocate(&mut self, device: &wgpu::Device, bytes: u64) -> IndexArenaAllocation {
        let bytes = bytes.max(4).next_multiple_of(4);
        for (page, entry) in self.pages.iter_mut().enumerate() {
            if let Some(index) = entry
                .free
                .iter()
                .position(|range| range.end - range.start >= bytes)
            {
                let range = entry.free[index].start..entry.free[index].start + bytes;
                entry.free[index].start += bytes;
                if entry.free[index].is_empty() {
                    entry.free.swap_remove(index);
                }
                return IndexArenaAllocation {
                    page,
                    range,
                    index_count: 0,
                };
            }
            if entry.capacity - entry.used >= bytes {
                let range = entry.used..entry.used + bytes;
                entry.used += bytes;
                return IndexArenaAllocation {
                    page,
                    range,
                    index_count: 0,
                };
            }
        }
        let capacity = self
            .pages
            .last()
            .map(|page| page.capacity.saturating_mul(2).max(bytes))
            .unwrap_or(Self::INITIAL_PAGE_SIZE)
            .max(Self::INITIAL_PAGE_SIZE);
        self.pages.push(Self::page(device, capacity));
        let page = self.pages.len() - 1;
        self.pages[page].used = bytes;
        IndexArenaAllocation {
            page,
            range: 0..bytes,
            index_count: 0,
        }
    }

    fn release(&mut self, allocation: IndexArenaAllocation) {
        release_arena_range(&mut self.pages[allocation.page].free, allocation.range);
    }
}

struct TransformBinding {
    _buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    last_used: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct RectVertex {
    position: [f32; 2],
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct RoundedRectVertex {
    position: [f32; 2],
    local: [f32; 2],
    size: [f32; 2],
    radius: f32,
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct LineVertex {
    position: [f32; 2],
    point: [f32; 2],
    start: [f32; 2],
    end: [f32; 2],
    width: f32,
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ImageVertex {
    position: [f32; 2],
    uv: [f32; 2],
    opacity: f32,
    color: [f32; 4],
}

enum RenderBatch {
    Rect(Vec<RectVertex>, Vec<u32>, Transform),
    IndexedRect(Vec<RectVertex>, Vec<u32>, Transform),
    Rounded(Vec<RoundedRectVertex>, Vec<u32>, Transform),
    Line(Vec<LineVertex>, Vec<u32>, Transform),
    Image {
        page: usize,
        images: Vec<ImageId>,
        vertices: Vec<ImageVertex>,
        indices: Vec<u32>,
        transform: Transform,
    },
    Clip(ClipGeometry, Transform),
}

#[derive(Clone, Debug)]
enum ClipGeometry {
    Reset,
    Rect(Rect),
    Rounded {
        rect: Rect,
        radius: Dip,
    },
    Path {
        bounds: Rect,
        vertices: Vec<RectVertex>,
        indices: Vec<u32>,
    },
}

#[derive(Clone, Debug, Default)]
struct PathMesh {
    vertices: Vec<RectVertex>,
    indices: Vec<u32>,
}

#[derive(Clone, PartialEq, Eq)]
enum BatchKind {
    Rect,
    Rounded,
    Line,
    Image {
        page: usize,
        /// All source images represented by this material run. This keeps
        /// resource lifetime accounting correct after same-page merging.
        images: Arc<Vec<ImageId>>,
    },
}

#[derive(Clone)]
enum GpuBatch {
    Draw(
        BatchKind,
        VertexArenaAllocation,
        IndexArenaAllocation,
        Transform,
    ),
    /// Adjacent compatible draws retain their command order but share one
    /// pipeline/material binding during replay.
    DrawGroup(
        BatchKind,
        Arc<Vec<(VertexArenaAllocation, IndexArenaAllocation)>>,
        Transform,
        Option<IndirectDrawRange>,
    ),
    Clip(ClipGeometry, Option<ClipGpuGeometry>, Transform),
}

fn gpu_batch_transform(batch: &GpuBatch) -> Option<Transform> {
    match batch {
        GpuBatch::Draw(_, _, _, transform)
        | GpuBatch::DrawGroup(_, _, transform, _)
        | GpuBatch::Clip(_, _, transform) => Some(*transform),
    }
}

/// A replay entry borrows immutable retained batches. Dirty-tile replay never
/// clones a node's draw lists merely to encode a frame.
enum ReplayBatch<'a> {
    Borrowed(&'a GpuBatch),
    /// A submission-time merge of adjacent retained indirect groups. The
    /// first batch supplies the material and arena pages; the range spans
    /// compatible commands from one or more retained nodes.
    BorrowedIndirectRun(&'a GpuBatch, IndirectDrawRange),
    Scissor(Rect),
    ResetClip,
}

impl ReplayBatch<'_> {
    fn batch(&self) -> Option<&GpuBatch> {
        match self {
            Self::Borrowed(batch) => Some(*batch),
            Self::BorrowedIndirectRun(batch, _) => Some(*batch),
            Self::Scissor(_) | Self::ResetClip => None,
        }
    }

    fn indirect_override(&self) -> Option<IndirectDrawRange> {
        match self {
            Self::BorrowedIndirectRun(_, range) => Some(*range),
            Self::Borrowed(_) | Self::Scissor(_) | Self::ResetClip => None,
        }
    }
}

#[derive(Clone, Copy)]
struct IndirectDrawRange {
    offset: u64,
    count: u32,
}

fn indirect_draw_range(batch: &GpuBatch) -> Option<IndirectDrawRange> {
    match batch {
        GpuBatch::DrawGroup(_, _, _, range) => *range,
        GpuBatch::Draw(..) | GpuBatch::Clip(..) => None,
    }
}

/// Returns a single indirect range only when one multi-draw call can use the
/// previous batch's pipeline, transform, vertex page and index page for both
/// command ranges. Clip and scissor commands are never candidates.
fn merge_indirect_draw_ranges(
    previous: &GpuBatch,
    previous_range: IndirectDrawRange,
    next: &GpuBatch,
    next_range: IndirectDrawRange,
) -> Option<IndirectDrawRange> {
    let (
        GpuBatch::DrawGroup(previous_kind, previous_draws, previous_transform, _),
        GpuBatch::DrawGroup(next_kind, next_draws, next_transform, _),
    ) = (previous, next)
    else {
        return None;
    };
    let ((previous_vertices, previous_indices), (next_vertices, next_indices)) =
        previous_draws.first().zip(next_draws.first())?;
    let stride = std::mem::size_of::<wgpu::util::DrawIndexedIndirectArgs>() as u64;
    (same_batch_material(previous_kind, next_kind)
        && previous_transform == next_transform
        && previous_vertices.page == next_vertices.page
        && previous_indices.page == next_indices.page
        && previous_range.offset + previous_range.count as u64 * stride == next_range.offset)
        .then_some(IndirectDrawRange {
            offset: previous_range.offset,
            count: previous_range.count + next_range.count,
        })
}

#[derive(Clone)]
struct ClipGpuGeometry {
    vertices: VertexArenaAllocation,
    indices: Option<IndexArenaAllocation>,
}

fn group_ordered_draws(batches: Vec<GpuBatch>) -> Vec<GpuBatch> {
    let mut grouped = Vec::with_capacity(batches.len());
    for batch in batches {
        let (kind, draws, transform, indirect) = match batch {
            GpuBatch::Draw(kind, vertices, indices, transform) => {
                (kind, Arc::new(vec![(vertices, indices)]), transform, None)
            }
            GpuBatch::DrawGroup(kind, draws, transform, indirect) => {
                (kind, draws, transform, indirect)
            }
            batch => {
                grouped.push(batch);
                continue;
            }
        };
        if let Some(GpuBatch::DrawGroup(
            previous_kind,
            previous_draws,
            previous_transform,
            previous_indirect,
        )) = grouped.last_mut()
        {
            let same_page = previous_draws.first().zip(draws.first()).is_some_and(
                |((previous_vertices, previous_indices), (vertices, indices))| {
                    previous_vertices.page == vertices.page && previous_indices.page == indices.page
                },
            );
            let contiguous_indirect = match (*previous_indirect, indirect) {
                (Some(previous), Some(next)) => {
                    previous.offset
                        + previous.count as u64
                            * std::mem::size_of::<wgpu::util::DrawIndexedIndirectArgs>() as u64
                        == next.offset
                }
                (None, None) => true,
                _ => false,
            };
            if same_batch_material(previous_kind, &kind)
                && *previous_transform == transform
                && same_page
                && contiguous_indirect
            {
                Arc::make_mut(previous_draws).extend(draws.iter().cloned());
                merge_batch_resources(previous_kind, &kind);
                *previous_indirect = match (*previous_indirect, indirect) {
                    (Some(previous), Some(next)) => Some(IndirectDrawRange {
                        offset: previous.offset,
                        count: previous.count + next.count,
                    }),
                    _ => None,
                };
                continue;
            }
        }
        grouped.push(GpuBatch::DrawGroup(kind, draws, transform, indirect));
    }
    grouped
}

fn same_batch_material(left: &BatchKind, right: &BatchKind) -> bool {
    match (left, right) {
        (BatchKind::Image { page: left, .. }, BatchKind::Image { page: right, .. }) => {
            left == right
        }
        _ => left == right,
    }
}

fn merge_batch_resources(into: &mut BatchKind, from: &BatchKind) {
    let (BatchKind::Image { images: into, .. }, BatchKind::Image { images: from, .. }) =
        (into, from)
    else {
        return;
    };
    let into = Arc::make_mut(into);
    for image in from.iter().copied() {
        if !into.contains(&image) {
            into.push(image);
        }
    }
}

fn vertex_stride(kind: &BatchKind) -> u64 {
    match kind {
        BatchKind::Rect => std::mem::size_of::<RectVertex>() as u64,
        BatchKind::Rounded => std::mem::size_of::<RoundedRectVertex>() as u64,
        BatchKind::Line => std::mem::size_of::<LineVertex>() as u64,
        BatchKind::Image { .. } => std::mem::size_of::<ImageVertex>() as u64,
    }
}

/// Upload indirect arguments for each page-local ordered draw group. Index
/// data in the arena is local to each mesh, therefore `base_vertex` moves the
/// index stream to the matching vertex allocation while `first_index` selects
/// the allocation inside the shared index page.
fn prepare_indirect_draws(
    batches: &mut [GpuBatch],
    arena: &mut IndirectArena,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    replaying_full_table: bool,
) -> Vec<IndirectArenaAllocation> {
    let force_reupload = replaying_full_table && std::mem::take(&mut arena.needs_reupload);
    let mut allocations = Vec::new();
    for batch in batches {
        let GpuBatch::DrawGroup(kind, draws, _, indirect) = batch else {
            continue;
        };
        if indirect.is_some() && !force_reupload {
            continue;
        }
        let bytes =
            (draws.len() * std::mem::size_of::<wgpu::util::DrawIndexedIndirectArgs>()) as u64;
        let start = indirect.map(|range| range.offset).unwrap_or_else(|| {
            let allocation = arena.allocate(device, bytes);
            let offset = allocation.range.start;
            allocations.push(allocation);
            offset
        });
        let stride = vertex_stride(kind);
        let mut commands = Vec::with_capacity(draws.len());
        for (vertices, indices) in draws.iter() {
            commands.push(wgpu::util::DrawIndexedIndirectArgs {
                index_count: indices.index_count,
                instance_count: 1,
                first_index: (indices.range.start / std::mem::size_of::<u32>() as u64) as u32,
                base_vertex: (vertices.range.start / stride) as i32,
                first_instance: 0,
            });
        }
        *indirect = Some(IndirectDrawRange {
            offset: start,
            count: draws.len() as u32,
        });
        queue.write_buffer(&arena.buffer, start, bytemuck::cast_slice(&commands));
    }
    allocations
}

fn batch_vertex_allocations(batches: &[GpuBatch]) -> Vec<VertexArenaAllocation> {
    batches
        .iter()
        .flat_map(|batch| match batch {
            GpuBatch::Draw(_, allocation, _, _)
            | GpuBatch::Clip(
                _,
                Some(ClipGpuGeometry {
                    vertices: allocation,
                    ..
                }),
                _,
            ) => vec![allocation.clone()],
            GpuBatch::DrawGroup(_, draws, _, _) => {
                draws.iter().map(|(vertices, _)| vertices.clone()).collect()
            }
            GpuBatch::Clip(_, None, _) => Vec::new(),
        })
        .collect()
}

fn batch_index_allocations(batches: &[GpuBatch]) -> Vec<IndexArenaAllocation> {
    batches
        .iter()
        .flat_map(|batch| match batch {
            GpuBatch::Draw(_, _, allocation, _) => vec![allocation.clone()],
            GpuBatch::Clip(
                _,
                Some(ClipGpuGeometry {
                    indices: Some(allocation),
                    ..
                }),
                _,
            ) => vec![allocation.clone()],
            GpuBatch::DrawGroup(_, draws, _, _) => {
                draws.iter().map(|(_, indices)| indices.clone()).collect()
            }
            GpuBatch::Clip(_, _, _) => Vec::new(),
        })
        .collect()
}

fn rect_batch(
    batches: &mut Vec<RenderBatch>,
    transform: Transform,
) -> (&mut Vec<RectVertex>, &mut Vec<u32>) {
    if !matches!(batches.last(), Some(RenderBatch::Rect(_, _, current)) if *current == transform) {
        batches.push(RenderBatch::Rect(Vec::new(), Vec::new(), transform));
    }
    match batches.last_mut().expect("rect batch was just added") {
        RenderBatch::Rect(vertices, indices, _) => (vertices, indices),
        _ => unreachable!(),
    }
}

fn rounded_batch(
    batches: &mut Vec<RenderBatch>,
    transform: Transform,
) -> (&mut Vec<RoundedRectVertex>, &mut Vec<u32>) {
    if !matches!(batches.last(), Some(RenderBatch::Rounded(_, _, current)) if *current == transform)
    {
        batches.push(RenderBatch::Rounded(Vec::new(), Vec::new(), transform));
    }
    match batches.last_mut().expect("rounded batch was just added") {
        RenderBatch::Rounded(vertices, indices, _) => (vertices, indices),
        _ => unreachable!(),
    }
}

fn line_batch(
    batches: &mut Vec<RenderBatch>,
    transform: Transform,
) -> (&mut Vec<LineVertex>, &mut Vec<u32>) {
    if !matches!(batches.last(), Some(RenderBatch::Line(_, _, current)) if *current == transform) {
        batches.push(RenderBatch::Line(Vec::new(), Vec::new(), transform));
    }
    match batches.last_mut().expect("line batch was just added") {
        RenderBatch::Line(vertices, indices, _) => (vertices, indices),
        _ => unreachable!(),
    }
}

fn image_batch(
    batches: &mut Vec<RenderBatch>,
    page: usize,
    image: ImageId,
    transform: Transform,
) -> (&mut Vec<ImageVertex>, &mut Vec<u32>) {
    if !matches!(batches.last(), Some(RenderBatch::Image { page: current, transform: current_transform, .. }) if *current == page && *current_transform == transform)
    {
        batches.push(RenderBatch::Image {
            page,
            images: vec![image],
            vertices: Vec::new(),
            indices: Vec::new(),
            transform,
        });
    }
    match batches.last_mut().expect("image batch was just added") {
        RenderBatch::Image {
            images,
            vertices,
            indices,
            ..
        } => {
            if !images.contains(&image) {
                images.push(image);
            }
            (vertices, indices)
        }
        _ => unreachable!(),
    }
}

pub struct Renderer {
    pub(crate) instance: wgpu::Instance,
    pub(crate) adapter: wgpu::Adapter,
    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
    surfaces: HashMap<WindowId, SurfaceState>,
    resources: ResourceManager,
    indirect_execution: bool,
}

struct GpuImage {
    bytes: usize,
    slot: AtlasSlot,
}

struct ImageAtlas {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    width: u32,
    height: u32,
    x: u32,
    y: u32,
    row_height: u32,
    free: Vec<AtlasSlot>,
}

#[derive(Clone, Copy)]
struct AtlasSlot {
    page: usize,
    origin: wgpu::Origin3d,
    width: u32,
    height: u32,
    atlas_width: u32,
    atlas_height: u32,
}

impl ImageAtlas {
    fn new(texture: wgpu::Texture) -> Self {
        let width = texture.width();
        let height = texture.height();
        Self {
            view: texture_view(&texture),
            texture,
            width,
            height,
            x: 0,
            y: 0,
            row_height: 0,
            free: Vec::new(),
        }
    }

    fn bytes(&self) -> usize {
        self.width as usize * self.height as usize * 4
    }

    fn allocate(&mut self, page: usize, width: u32, height: u32) -> Option<AtlasSlot> {
        if let Some(index) = self
            .free
            .iter()
            .position(|slot| slot.width >= width && slot.height >= height)
        {
            let slot = self.free.swap_remove(index);
            // Keep the unused part available for later allocations.
            if slot.width > width {
                self.free.push(AtlasSlot {
                    origin: wgpu::Origin3d {
                        x: slot.origin.x + width,
                        ..slot.origin
                    },
                    width: slot.width - width,
                    height,
                    ..slot
                });
            }
            if slot.height > height {
                self.free.push(AtlasSlot {
                    origin: wgpu::Origin3d {
                        y: slot.origin.y + height,
                        ..slot.origin
                    },
                    width: slot.width,
                    height: slot.height - height,
                    ..slot
                });
            }
            return Some(AtlasSlot {
                page,
                width,
                height,
                ..slot
            });
        }
        if self.x + width > self.width {
            self.x = 0;
            self.y += self.row_height;
            self.row_height = 0;
        }
        if self.y + height > self.height {
            return None;
        }
        let slot = AtlasSlot {
            page,
            origin: wgpu::Origin3d {
                x: self.x,
                y: self.y,
                z: 0,
            },
            width,
            height,
            atlas_width: self.width,
            atlas_height: self.height,
        };
        self.x += width;
        self.row_height = self.row_height.max(height);
        Some(slot)
    }

    fn release(&mut self, slot: AtlasSlot) {
        self.free.push(slot);
        coalesce_atlas_slots(&mut self.free);
    }
}

fn coalesce_atlas_slots(free: &mut Vec<AtlasSlot>) {
    let mut changed = true;
    while changed {
        changed = false;
        'pairs: for left in 0..free.len() {
            for right in (left + 1)..free.len() {
                let a = free[left];
                let b = free[right];
                if a.page != b.page
                    || a.atlas_width != b.atlas_width
                    || a.atlas_height != b.atlas_height
                {
                    continue;
                }
                let horizontal = a.origin.y == b.origin.y
                    && a.height == b.height
                    && (a.origin.x + a.width == b.origin.x || b.origin.x + b.width == a.origin.x);
                let vertical = a.origin.x == b.origin.x
                    && a.width == b.width
                    && (a.origin.y + a.height == b.origin.y || b.origin.y + b.height == a.origin.y);
                if horizontal || vertical {
                    let origin = wgpu::Origin3d {
                        x: a.origin.x.min(b.origin.x),
                        y: a.origin.y.min(b.origin.y),
                        z: 0,
                    };
                    free[left] = AtlasSlot {
                        page: a.page,
                        origin,
                        width: if horizontal {
                            a.width + b.width
                        } else {
                            a.width
                        },
                        height: if vertical {
                            a.height + b.height
                        } else {
                            a.height
                        },
                        atlas_width: a.atlas_width,
                        atlas_height: a.atlas_height,
                    };
                    free.swap_remove(right);
                    changed = true;
                    break 'pairs;
                }
            }
        }
    }
}

struct CachedGlyph {
    metrics: fontdue::Metrics,
    image: ImageId,
}

impl Renderer {
    pub async fn new() -> Result<Self, RenderError> {
        let instance = wgpu::Instance::default();
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .map_err(|_| RenderError::AdapterUnavailable)?;
        let indirect_execution = adapter
            .get_downlevel_capabilities()
            .flags
            .contains(wgpu::DownlevelFlags::INDIRECT_EXECUTION);
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("zui-render device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_defaults(),
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                memory_hints: wgpu::MemoryHints::Performance,
                trace: wgpu::Trace::Off,
            })
            .await
            .map_err(|error| RenderError::Device(error.to_string()))?;
        let resources = ResourceManager::new(&device, cached_system_fonts());
        Ok(Self {
            instance,
            adapter,
            device,
            queue,
            surfaces: HashMap::new(),
            resources,
            indirect_execution,
        })
    }

    pub fn new_blocking() -> Result<Self, RenderError> {
        pollster::block_on(Self::new())
    }

    pub fn register_image(&mut self, id: ImageId, image: ImageResource) {
        self.resources
            .register_image(&self.device, &self.queue, id, image);
    }

    pub fn remove_image(&mut self, id: ImageId) -> Option<ImageResource> {
        self.resources.remove_image(id)
    }

    pub fn retain_resource(&mut self, handle: ResourceHandle) {
        self.resources.retain(handle);
    }

    pub fn release_resource(&mut self, handle: ResourceHandle) {
        self.resources.release(handle);
    }

    pub fn resource_reference_count(&self, handle: ResourceHandle) -> usize {
        self.resources.reference_count(handle)
    }

    pub fn resource_manager(&mut self) -> &mut ResourceManager {
        &mut self.resources
    }

    /// Returns submission metrics for the last frame rendered to `window`.
    pub fn frame_stats(&self, window: WindowId) -> Option<FrameStats> {
        self.surfaces
            .get(&window)
            .map(|state| state.last_frame_stats)
    }

    /// Attach a platform host's native handles to a render surface.
    ///
    /// The host must outlive the returned surface and the renderer. Backend
    /// implementations are responsible for keeping that lifetime valid.
    pub fn attach_surface<H: RawWindowHandleProvider>(
        &mut self,
        window: WindowId,
        host: &H,
        size: PhysicalSize,
        scale_factor: ScaleFactor,
    ) -> Result<(), RenderError> {
        let raw_window_handle = host
            .raw_window_handle()
            .map_err(|error| RenderError::Surface(error.to_string()))?;
        let raw_display_handle = host
            .raw_display_handle()
            .map_err(|error| RenderError::Surface(error.to_string()))?;
        let target = wgpu::SurfaceTargetUnsafe::RawHandle {
            raw_display_handle: Some(raw_display_handle),
            raw_window_handle,
        };
        let surface = unsafe { self.instance.create_surface_unsafe(target) }
            .map_err(|error| RenderError::Surface(error.to_string()))?;
        let mut config = surface
            .get_default_config(&self.adapter, size.width.max(1), size.height.max(1))
            .ok_or_else(|| RenderError::Surface("adapter cannot present to this surface".into()))?;
        let direct_copy_present = surface
            .get_capabilities(&self.adapter)
            .usages
            .contains(wgpu::TextureUsages::COPY_DST);
        if direct_copy_present {
            config.usage |= wgpu::TextureUsages::COPY_DST;
        }
        surface.configure(&self.device, &config);
        let transform_layout = create_transform_bind_group_layout(&self.device);
        let pipeline = create_rect_pipeline(&self.device, config.format, &transform_layout);
        let rounded_pipeline =
            create_rounded_rect_pipeline(&self.device, config.format, &transform_layout);
        let stencil_rounded_pipeline =
            create_stencil_rounded_pipeline(&self.device, config.format, &transform_layout);
        let line_pipeline = create_line_pipeline(&self.device, config.format, &transform_layout);
        let image_pipeline = create_image_pipeline(&self.device, config.format, &transform_layout);
        let image_bind_group_layout = image_pipeline.get_bind_group_layout(1);
        let stencil_pipeline = create_stencil_pipeline(&self.device, config.format);
        let stencil_mask_pipeline =
            create_stencil_mask_pipeline(&self.device, config.format, &transform_layout);
        let (canvas, canvas_view) = create_canvas(&self.device, size, config.format);
        let (stencil, stencil_view) = create_stencil(&self.device, size);
        let stencil_reset = create_stencil_reset_buffer(&self.device);
        let damage_clear_pipeline = create_damage_clear_pipeline(&self.device, config.format);
        let damage_clear_color =
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("zui-render damage clear color"),
                    contents: bytemuck::cast_slice(&[[0.0_f32; 4]]),
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                });
        let damage_clear_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("zui-render damage clear bind group"),
            layout: &damage_clear_pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: damage_clear_color.as_entire_binding(),
            }],
        });
        let blit_pipeline = create_blit_pipeline(&self.device, config.format);
        let blit_bind_group = create_blit_bind_group(&self.device, &blit_pipeline, &canvas_view);
        self.surfaces.insert(
            window,
            SurfaceState {
                surface,
                config,
                size,
                pipeline,
                rounded_pipeline,
                stencil_rounded_pipeline,
                line_pipeline,
                image_pipeline,
                image_bind_group_layout,
                stencil_pipeline,
                stencil_mask_pipeline,
                transform_bind_group_layout: transform_layout,
                transform_bindings: HashMap::new(),
                transform_binding_clock: 0,
                max_transform_bindings: 1024,
                canvas,
                canvas_view,
                stencil,
                stencil_view,
                stencil_reset,
                damage_clear_pipeline,
                damage_clear_color,
                damage_clear_bind_group,
                blit_pipeline,
                blit_bind_group,
                direct_copy_present,
                scale_factor,
                has_contents: false,
                retained_items: None,
                retained_gpu_items: None,
                retained_submission_batches: None,
                vertex_arenas: VertexArenas::new(&self.device),
                index_arena: IndexArena::new(&self.device),
                indirect_arena: IndirectArena::new(&self.device),
                composition_tiles: CompositionTiles::new(size),
                spatial_index: None,
                tile_submission_index: None,
                submitted_scene_revision: None,
                last_frame_stats: FrameStats::default(),
            },
        );
        Ok(())
    }

    /// Attach a native window directly. This is the preferred path for the
    /// initial winit backend and avoids leaking wgpu handles into platform API.
    pub fn attach_native_surface<W: wgpu::rwh::HasDisplayHandle + wgpu::rwh::HasWindowHandle>(
        &mut self,
        window: WindowId,
        host: &W,
        size: PhysicalSize,
        scale_factor: ScaleFactor,
    ) -> Result<(), RenderError> {
        let target = unsafe {
            wgpu::SurfaceTargetUnsafe::from_display_and_window(host, host)
                .map_err(|error| RenderError::Surface(error.to_string()))?
        };
        let surface = unsafe { self.instance.create_surface_unsafe(target) }
            .map_err(|error| RenderError::Surface(error.to_string()))?;
        let mut config = surface
            .get_default_config(&self.adapter, size.width.max(1), size.height.max(1))
            .ok_or_else(|| RenderError::Surface("adapter cannot present to this surface".into()))?;
        let direct_copy_present = surface
            .get_capabilities(&self.adapter)
            .usages
            .contains(wgpu::TextureUsages::COPY_DST);
        if direct_copy_present {
            config.usage |= wgpu::TextureUsages::COPY_DST;
        }
        surface.configure(&self.device, &config);
        let transform_layout = create_transform_bind_group_layout(&self.device);
        let pipeline = create_rect_pipeline(&self.device, config.format, &transform_layout);
        let rounded_pipeline =
            create_rounded_rect_pipeline(&self.device, config.format, &transform_layout);
        let stencil_rounded_pipeline =
            create_stencil_rounded_pipeline(&self.device, config.format, &transform_layout);
        let line_pipeline = create_line_pipeline(&self.device, config.format, &transform_layout);
        let image_pipeline = create_image_pipeline(&self.device, config.format, &transform_layout);
        let image_bind_group_layout = image_pipeline.get_bind_group_layout(1);
        let stencil_pipeline = create_stencil_pipeline(&self.device, config.format);
        let stencil_mask_pipeline =
            create_stencil_mask_pipeline(&self.device, config.format, &transform_layout);
        let (canvas, canvas_view) = create_canvas(&self.device, size, config.format);
        let (stencil, stencil_view) = create_stencil(&self.device, size);
        let stencil_reset = create_stencil_reset_buffer(&self.device);
        let damage_clear_pipeline = create_damage_clear_pipeline(&self.device, config.format);
        let damage_clear_color =
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("zui-render damage clear color"),
                    contents: bytemuck::cast_slice(&[[0.0_f32; 4]]),
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                });
        let damage_clear_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("zui-render damage clear bind group"),
            layout: &damage_clear_pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: damage_clear_color.as_entire_binding(),
            }],
        });
        let blit_pipeline = create_blit_pipeline(&self.device, config.format);
        let blit_bind_group = create_blit_bind_group(&self.device, &blit_pipeline, &canvas_view);
        self.surfaces.insert(
            window,
            SurfaceState {
                surface,
                config,
                size,
                pipeline,
                rounded_pipeline,
                stencil_rounded_pipeline,
                line_pipeline,
                image_pipeline,
                image_bind_group_layout,
                stencil_pipeline,
                stencil_mask_pipeline,
                transform_bind_group_layout: transform_layout,
                transform_bindings: HashMap::new(),
                transform_binding_clock: 0,
                max_transform_bindings: 1024,
                canvas,
                canvas_view,
                stencil,
                stencil_view,
                stencil_reset,
                damage_clear_pipeline,
                damage_clear_color,
                damage_clear_bind_group,
                blit_pipeline,
                blit_bind_group,
                direct_copy_present,
                scale_factor,
                has_contents: false,
                retained_items: None,
                retained_gpu_items: None,
                retained_submission_batches: None,
                vertex_arenas: VertexArenas::new(&self.device),
                index_arena: IndexArena::new(&self.device),
                indirect_arena: IndirectArena::new(&self.device),
                composition_tiles: CompositionTiles::new(size),
                spatial_index: None,
                tile_submission_index: None,
                submitted_scene_revision: None,
                last_frame_stats: FrameStats::default(),
            },
        );
        Ok(())
    }

    pub fn resize(
        &mut self,
        window: WindowId,
        size: PhysicalSize,
        _scale_factor: ScaleFactor,
    ) -> Result<(), RenderError> {
        let state = self
            .surfaces
            .get_mut(&window)
            .ok_or(RenderError::SurfaceNotAttached(window))?;
        if size.width == 0 || size.height == 0 {
            state.size = size;
            return Ok(());
        }
        state.size = size;
        state.config.width = size.width;
        state.config.height = size.height;
        state.surface.configure(&self.device, &state.config);
        let (canvas, canvas_view) = create_canvas(&self.device, size, state.config.format);
        state.canvas = canvas;
        state.canvas_view = canvas_view;
        let (stencil, stencil_view) = create_stencil(&self.device, size);
        state.stencil = stencil;
        state.stencil_view = stencil_view;
        state.blit_bind_group =
            create_blit_bind_group(&self.device, &state.blit_pipeline, &state.canvas_view);
        state.has_contents = false;
        state.transform_bindings.clear();
        state.transform_binding_clock = 0;
        state.retained_items = None;
        state.retained_submission_batches = None;
        let released_resources = state
            .retained_gpu_items
            .take()
            .into_iter()
            .flat_map(|items| items.into_values())
            .flat_map(|item| item.resources)
            .collect::<Vec<_>>();
        self.resources.release_all(&released_resources);
        state.vertex_arenas = VertexArenas::new(&self.device);
        state.index_arena = IndexArena::new(&self.device);
        state.indirect_arena = IndirectArena::new(&self.device);
        state.composition_tiles = CompositionTiles::new(size);
        state.spatial_index = None;
        state.tile_submission_index = None;
        state.submitted_scene_revision = None;
        state.last_frame_stats = FrameStats::default();
        Ok(())
    }

    pub fn detach_surface(&mut self, window: WindowId) {
        if let Some(state) = self.surfaces.remove(&window) {
            let resources = state
                .retained_gpu_items
                .into_iter()
                .flat_map(|items| items.into_values())
                .flat_map(|item| item.resources)
                .collect::<Vec<_>>();
            self.resources.release_all(&resources);
        }
    }

    fn build_render_batches(
        &mut self,
        commands: &[PaintCommand],
        _render_size: PhysicalSize,
        scale_factor: f32,
        clip_transform: Transform,
        initial_opacity: f32,
    ) -> Vec<RenderBatch> {
        let mut batches = Vec::new();
        let mut command_transform = Transform::IDENTITY;
        let mut opacity = initial_opacity;
        let mut clip_stack: Vec<(ClipGeometry, Transform)> = Vec::new();
        let mut transform_stack = Vec::new();
        let mut opacity_stack = Vec::new();
        for command in commands {
            match command {
                PaintCommand::PushTransform(next) => {
                    transform_stack.push(command_transform);
                    command_transform = compose_transform(command_transform, *next);
                    continue;
                }
                PaintCommand::PopTransform => {
                    command_transform = transform_stack.pop().expect("validated transform stack");
                    continue;
                }
                PaintCommand::PushOpacity(value) => {
                    opacity_stack.push(opacity);
                    opacity *= value.clamp(0.0, 1.0);
                    continue;
                }
                PaintCommand::PopOpacity => {
                    opacity = opacity_stack.pop().expect("validated opacity stack");
                    continue;
                }
                PaintCommand::PushClip(shape) => {
                    let geometry = match shape {
                        ClipShape::Rect(rect) => ClipGeometry::Rect(*rect),
                        ClipShape::RoundedRect { rect, radius } => ClipGeometry::Rounded {
                            rect: *rect,
                            radius: *radius,
                        },
                        ClipShape::Path { path } => {
                            let mesh = self.resources.tessellate_path(path);
                            ClipGeometry::Path {
                                bounds: path_bounds(path),
                                vertices: mesh.vertices,
                                indices: mesh.indices,
                            }
                        }
                    };
                    let transform = compose_transform(clip_transform, command_transform);
                    batches.push(RenderBatch::Clip(geometry.clone(), transform));
                    clip_stack.push((geometry, transform));
                    continue;
                }
                PaintCommand::PopClip => {
                    batches.push(RenderBatch::Clip(ClipGeometry::Reset, clip_transform));
                    clip_stack.pop();
                    for (geometry, transform) in &clip_stack {
                        batches.push(RenderBatch::Clip(geometry.clone(), *transform));
                    }
                    continue;
                }
                _ => {}
            }
            let transform = compose_transform(clip_transform, command_transform);
            match command {
                PaintCommand::Rect { rect, color } => {
                    let (vertices, indices) = rect_batch(&mut batches, transform);
                    append_rect(vertices, indices, *rect, apply_opacity(*color, opacity));
                }
                PaintCommand::RoundedRect {
                    rect,
                    radius,
                    color,
                } => {
                    let (vertices, indices) = rounded_batch(&mut batches, transform);
                    append_rounded_rect(
                        vertices,
                        indices,
                        *rect,
                        *radius,
                        apply_opacity(*color, opacity),
                    )
                }
                PaintCommand::Line {
                    start,
                    end,
                    width,
                    color,
                } => {
                    let (vertices, indices) = line_batch(&mut batches, transform);
                    append_line(
                        vertices,
                        indices,
                        *start,
                        *end,
                        *width,
                        apply_opacity(*color, opacity),
                    )
                }
                PaintCommand::Text {
                    text,
                    origin,
                    color,
                    scale,
                } => append_text(
                    &mut batches,
                    &mut self.resources,
                    &self.device,
                    &self.queue,
                    text,
                    *origin,
                    apply_opacity(*color, opacity),
                    *scale,
                    scale_factor,
                    transform,
                ),
                PaintCommand::Icon {
                    path,
                    color,
                    stroke,
                    ..
                } => {
                    let mesh = path_stroke_mesh(path, *stroke, apply_opacity(*color, opacity));
                    if !mesh.vertices.is_empty() {
                        batches.push(RenderBatch::Rect(mesh.vertices, mesh.indices, transform));
                    }
                }
                PaintCommand::PathFill { path, color } => {
                    let mesh = path_fill_mesh(path, apply_opacity(*color, opacity));
                    if !mesh.vertices.is_empty() && !mesh.indices.is_empty() {
                        batches.push(RenderBatch::IndexedRect(
                            mesh.vertices,
                            mesh.indices,
                            transform,
                        ));
                    }
                }
                PaintCommand::PathStroke { path, width, color } => {
                    let mesh = path_stroke_mesh(path, *width, apply_opacity(*color, opacity));
                    if !mesh.vertices.is_empty() && !mesh.indices.is_empty() {
                        batches.push(RenderBatch::IndexedRect(
                            mesh.vertices,
                            mesh.indices,
                            transform,
                        ));
                    }
                }
                PaintCommand::Image {
                    rect,
                    image,
                    opacity: image_opacity,
                } => {
                    if let Some((page, uv)) =
                        self.resources
                            .ensure_image_atlas_slot(&self.device, &self.queue, *image)
                    {
                        let (vertices, indices) =
                            image_batch(&mut batches, page, *image, transform);
                        append_image(
                            vertices,
                            indices,
                            *rect,
                            *image_opacity * opacity,
                            Color::WHITE,
                            uv,
                        )
                    }
                }
                PaintCommand::PushTransform(_)
                | PaintCommand::PopTransform
                | PaintCommand::PushOpacity(_)
                | PaintCommand::PopOpacity
                | PaintCommand::Clear(_)
                | PaintCommand::PopClip => {}
                PaintCommand::PushClip(_) => unreachable!(),
            }
        }
        batches
    }

    fn build_clip_batch(&mut self, shape: &ClipShape, transform: Transform) -> RenderBatch {
        let geometry = match shape {
            ClipShape::Rect(rect) => ClipGeometry::Rect(*rect),
            ClipShape::RoundedRect { rect, radius } => ClipGeometry::Rounded {
                rect: *rect,
                radius: *radius,
            },
            ClipShape::Path { path } => {
                let mesh = self.resources.tessellate_path(path);
                ClipGeometry::Path {
                    bounds: path_bounds(path),
                    vertices: mesh.vertices,
                    indices: mesh.indices,
                }
            }
        };
        RenderBatch::Clip(geometry, transform)
    }

    /// Replays the retained GPU scene into the persistent canvas and presents
    /// it. Immediate command-list rendering is intentionally absent: all
    /// widgets enter through `RenderNode` and therefore share one resource,
    /// dirty-state and allocation lifecycle.
    fn render_retained_scene_with_damage_regions(
        &mut self,
        window: WindowId,
        damage_regions: &[Rect],
        clear: Color,
    ) -> Result<(), RenderError> {
        let state = self
            .surfaces
            .get_mut(&window)
            .ok_or(RenderError::SurfaceNotAttached(window))?;
        // Temporarily move retained GPU items out of SurfaceState. This keeps
        // the replay table stable while the pass mutates other per-surface
        // caches (transform bindings) without cloning every node batch.
        let mut retained_gpu_items = state
            .retained_gpu_items
            .take()
            .expect("retained scene is prepared before replay");
        // An empty damage list means full invalidation at the UI layer. On a
        // newly attached (or resized) surface, make that explicit so the
        // first frame cannot take a partial replay/composite path.
        let full_frame = damage_regions.is_empty();
        let tiled_damage_regions =
            state
                .composition_tiles
                .regions(damage_regions, state.scale_factor, full_frame);
        let damage_regions = tiled_damage_regions.as_slice();
        let mut frame_stats = FrameStats {
            dirty_tile_count: damage_regions.len(),
            full_frame,
            ..FrameStats::default()
        };
        let frame = match state.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
                return Err(RenderError::SurfaceLost)
            }
            wgpu::CurrentSurfaceTexture::Timeout => return Ok(()),
            wgpu::CurrentSurfaceTexture::Occluded => return Ok(()),
            wgpu::CurrentSurfaceTexture::Validation => {
                return Err(RenderError::Surface("surface validation failed".into()))
            }
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let clear_rgba = [clear.r, clear.g, clear.b, clear.a];
        let clear = wgpu::Color {
            r: clear.r as f64,
            g: clear.g as f64,
            b: clear.b as f64,
            a: clear.a as f64,
        };
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("zui-render frame"),
            });
        let queue = &self.queue;
        queue.write_buffer(
            &state.damage_clear_color,
            0,
            bytemuck::cast_slice(&[clear_rgba]),
        );
        {
            // An arena growth replaces its backing buffer. Re-upload the
            // persistent full submission table once before borrowing partial
            // node spans from it again.
            if self.indirect_execution && state.indirect_arena.needs_reupload {
                if let Some(submissions) = state.retained_submission_batches.as_ref() {
                    let items = &mut retained_gpu_items;
                    // `prepare_indirect_draws` consumes this flag. Segments
                    // are visited independently, so restore it for every
                    // segment batch that must rewrite its existing command
                    // range after an arena buffer growth.
                    submissions.for_each_entry(|path, _| {
                        let Some(item) = items.get_mut(path) else {
                            return;
                        };
                        state.indirect_arena.needs_reupload = true;
                        let _ = prepare_indirect_draws(
                            &mut item.batches,
                            &mut state.indirect_arena,
                            &self.device,
                            &self.queue,
                            true,
                        );
                    });
                }
            }
            frame_stats.replay_entry_count = 0;
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("zui-render clear pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &state.canvas_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: if !full_frame && !damage_regions.is_empty() && state.has_contents {
                            wgpu::LoadOp::Load
                        } else {
                            wgpu::LoadOp::Clear(clear)
                        },
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &state.stencil_view,
                    depth_ops: None,
                    stencil_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0),
                        store: wgpu::StoreOp::Store,
                    }),
                }),
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });
            let mut active_clip = None;
            let mut active_damage = None;
            let mut active_clip_depth = 0_u32;
            let mut active_pipeline = None;
            let mut active_transform = None;
            let mut active_image = None;
            let mut encode_replay = |replay: &ReplayBatch<'_>| {
                frame_stats.replay_entry_count += 1;
                let scissor = match replay {
                    ReplayBatch::Scissor(region) => Some(*region),
                    _ => None,
                };
                if let Some(region) = scissor {
                    active_clip = None;
                    active_damage = Some(region);
                    active_clip_depth = 0;
                    set_scissor(&mut pass, None, state.size, state.scale_factor);
                    pass.set_pipeline(&state.stencil_pipeline);
                    frame_stats.pipeline_switch_count += 1;
                    pass.set_stencil_reference(0);
                    pass.set_vertex_buffer(0, state.stencil_reset.slice(..));
                    pass.draw(0..6, 0..1);
                    // A render-pass LoadOp preserves the persistent canvas;
                    // clear this tile explicitly before replaying the items
                    // that intersect it. Without this draw, changed text and
                    // translucent controls accumulate over prior pixels.
                    set_scissor(&mut pass, Some(region), state.size, state.scale_factor);
                    pass.set_pipeline(&state.damage_clear_pipeline);
                    pass.set_bind_group(0, &state.damage_clear_bind_group, &[]);
                    pass.set_vertex_buffer(0, state.stencil_reset.slice(..));
                    pass.draw(0..6, 0..1);
                    active_pipeline = None;
                    active_transform = None;
                    active_image = None;
                    return;
                }
                if matches!(replay, ReplayBatch::ResetClip) {
                    set_scissor(&mut pass, None, state.size, state.scale_factor);
                    pass.set_pipeline(&state.stencil_pipeline);
                    frame_stats.pipeline_switch_count += 1;
                    pass.set_stencil_reference(0);
                    pass.set_vertex_buffer(0, state.stencil_reset.slice(..));
                    pass.draw(0..6, 0..1);
                    active_clip = None;
                    active_clip_depth = 0;
                    active_pipeline = None;
                    active_transform = None;
                    active_image = None;
                    return;
                }
                let batch = replay
                    .batch()
                    .expect("non-control replay entry contains a GPU batch");
                let indirect_override = replay.indirect_override();
                if let GpuBatch::Clip(geometry, mask, transform) = batch {
                    // Axis-aligned rectangular clips are represented exactly
                    // by the pass scissor. Avoiding a stencil mask here saves
                    // a draw, a pipeline change, and a stencil write for the
                    // overwhelmingly common container clip.
                    if let ClipGeometry::Rect(rect) = geometry {
                        let rect = transform.rect(*rect);
                        active_clip = Some(
                            active_clip
                                .map(|previous| intersect_rect(previous, rect))
                                .unwrap_or(rect),
                        );
                        frame_stats.scissor_clip_count += 1;
                        return;
                    }
                    if let Some(mask) = mask {
                        let pipeline_key = match geometry {
                            ClipGeometry::Rounded { .. } => 4,
                            _ => 5,
                        };
                        if active_pipeline != Some(pipeline_key) {
                            pass.set_pipeline(match geometry {
                                ClipGeometry::Rounded { .. } => &state.stencil_rounded_pipeline,
                                _ => &state.stencil_mask_pipeline,
                            });
                            active_pipeline = Some(pipeline_key);
                            frame_stats.pipeline_switch_count += 1;
                        }
                        pass.set_stencil_reference(active_clip_depth);
                        let key = transform_key(*transform);
                        if active_transform != Some(key) {
                            pass.set_bind_group(
                                0,
                                &state
                                    .transform_bindings
                                    .get(&key)
                                    .expect("clip transform binding is prepared")
                                    .bind_group,
                                &[],
                            );
                            active_transform = Some(key);
                            frame_stats.bind_group_switch_count += 1;
                        }
                        active_image = None;
                        pass.set_vertex_buffer(
                            0,
                            state
                                .vertex_arenas
                                .buffer(&mask.vertices)
                                .slice(mask.vertices.range.clone()),
                        );
                        if let Some(indices) = &mask.indices {
                            pass.set_index_buffer(
                                state
                                    .index_arena
                                    .buffer(indices)
                                    .slice(indices.range.clone()),
                                wgpu::IndexFormat::Uint32,
                            );
                            pass.draw_indexed(0..indices.index_count, 0, 0..1);
                        } else {
                            pass.draw(0..mask.vertices.vertex_count, 0..1);
                        }
                        active_clip_depth = active_clip_depth.saturating_add(1);
                        frame_stats.stencil_clip_count += 1;
                    } else {
                        set_scissor(&mut pass, None, state.size, state.scale_factor);
                        pass.set_pipeline(&state.stencil_pipeline);
                        frame_stats.pipeline_switch_count += 1;
                        pass.set_stencil_reference(0);
                        pass.set_vertex_buffer(0, state.stencil_reset.slice(..));
                        pass.draw(0..6, 0..1);
                        active_clip_depth = 0;
                        active_pipeline = None;
                        active_transform = None;
                        active_image = None;
                    }
                    if matches!(geometry, ClipGeometry::Reset) {
                        active_clip = None;
                        return;
                    }
                    let next_clip = match geometry {
                        ClipGeometry::Rounded { rect, .. } => transform.rect(*rect),
                        ClipGeometry::Path { bounds, .. } => transform.rect(*bounds),
                        ClipGeometry::Rect(_) | ClipGeometry::Reset => unreachable!(),
                    };
                    active_clip = Some(
                        active_clip
                            .map(|previous| intersect_rect(previous, next_clip))
                            .unwrap_or(next_clip),
                    );
                    return;
                }
                let (kind, draws, transform, indirect): (
                    &BatchKind,
                    &[(VertexArenaAllocation, IndexArenaAllocation)],
                    Transform,
                    Option<IndirectDrawRange>,
                ) = match batch {
                    GpuBatch::DrawGroup(kind, draws, transform, indirect) => {
                        (kind, draws.as_slice(), *transform, *indirect)
                    }
                    // Every retained CPU batch is normalized by
                    // `group_ordered_draws` before replay.
                    GpuBatch::Draw(..) => unreachable!("draw batches are grouped before replay"),
                    _ => return,
                };
                let indirect = indirect_override.or(indirect);
                let scissor = intersect_clip(active_clip, active_damage);
                set_scissor(&mut pass, scissor, state.size, state.scale_factor);
                pass.set_stencil_reference(active_clip_depth);
                let pipeline_key = match kind {
                    BatchKind::Rect => 0,
                    BatchKind::Rounded => 1,
                    BatchKind::Line => 2,
                    BatchKind::Image { .. } => 3,
                };
                if active_pipeline != Some(pipeline_key) {
                    pass.set_pipeline(match kind {
                        BatchKind::Rect => &state.pipeline,
                        BatchKind::Rounded => &state.rounded_pipeline,
                        BatchKind::Line => &state.line_pipeline,
                        BatchKind::Image { .. } => &state.image_pipeline,
                    });
                    active_pipeline = Some(pipeline_key);
                    frame_stats.pipeline_switch_count += 1;
                }
                let key = transform_key(transform);
                if active_transform != Some(key) {
                    pass.set_bind_group(
                        0,
                        &state
                            .transform_bindings
                            .get(&key)
                            .expect("draw transform binding is prepared")
                            .bind_group,
                        &[],
                    );
                    active_transform = Some(key);
                    frame_stats.bind_group_switch_count += 1;
                }
                if let BatchKind::Image { page, .. } = kind {
                    if let Some(bind_group) = self.resources.bind_group(
                        window,
                        *page,
                        &self.device,
                        &state.image_bind_group_layout,
                    ) {
                        if active_image != Some(*page) {
                            pass.set_bind_group(1, &bind_group, &[]);
                            active_image = Some(*page);
                            frame_stats.bind_group_switch_count += 1;
                        }
                        if let (Some(indirect), Some((vertices, indices))) =
                            (indirect, draws.first())
                        {
                            pass.set_vertex_buffer(
                                0,
                                state.vertex_arenas.buffer(vertices).slice(..),
                            );
                            pass.set_index_buffer(
                                state.index_arena.buffer(indices).slice(..),
                                wgpu::IndexFormat::Uint32,
                            );
                            pass.multi_draw_indexed_indirect(
                                &state.indirect_arena.buffer,
                                indirect.offset,
                                indirect.count,
                            );
                            frame_stats.indirect_submission_count += 1;
                            frame_stats.indirect_draw_count += indirect.count as usize;
                        } else {
                            for (vertices, indices) in draws {
                                pass.set_vertex_buffer(
                                    0,
                                    state
                                        .vertex_arenas
                                        .buffer(vertices)
                                        .slice(vertices.range.clone()),
                                );
                                pass.set_index_buffer(
                                    state
                                        .index_arena
                                        .buffer(indices)
                                        .slice(indices.range.clone()),
                                    wgpu::IndexFormat::Uint32,
                                );
                                pass.draw_indexed(0..indices.index_count, 0, 0..1);
                                frame_stats.direct_draw_count += 1;
                            }
                        }
                    }
                } else {
                    active_image = None;
                    if let (Some(indirect), Some((vertices, indices))) = (indirect, draws.first()) {
                        pass.set_vertex_buffer(0, state.vertex_arenas.buffer(vertices).slice(..));
                        pass.set_index_buffer(
                            state.index_arena.buffer(indices).slice(..),
                            wgpu::IndexFormat::Uint32,
                        );
                        pass.multi_draw_indexed_indirect(
                            &state.indirect_arena.buffer,
                            indirect.offset,
                            indirect.count,
                        );
                        frame_stats.indirect_submission_count += 1;
                        frame_stats.indirect_draw_count += indirect.count as usize;
                    } else {
                        for (vertices, indices) in draws {
                            pass.set_vertex_buffer(
                                0,
                                state
                                    .vertex_arenas
                                    .buffer(vertices)
                                    .slice(vertices.range.clone()),
                            );
                            pass.set_index_buffer(
                                state
                                    .index_arena
                                    .buffer(indices)
                                    .slice(indices.range.clone()),
                                wgpu::IndexFormat::Uint32,
                            );
                            pass.draw_indexed(0..indices.index_count, 0, 0..1);
                            frame_stats.direct_draw_count += 1;
                        }
                    }
                }
            };
            let node_batches = &retained_gpu_items;
            if state.has_contents && !damage_regions.is_empty() {
                let index = state
                    .tile_submission_index
                    .as_ref()
                    .expect("retained GPU items always have a tile submission index");
                for region in damage_regions {
                    encode_replay(&ReplayBatch::Scissor(*region));
                    // A scissor starts an independent replay scope. Within
                    // that scope retain the same cross-node multi-draw
                    // coalescing used by a full scene replay.
                    let mut pending_indirect: Option<(&GpuBatch, IndirectDrawRange)> = None;
                    macro_rules! flush_tile_indirect {
                        () => {
                            if let Some((batch, range)) = pending_indirect.take() {
                                encode_replay(&ReplayBatch::BorrowedIndirectRun(batch, range));
                            }
                        };
                    }
                    frame_stats.retained_candidate_count +=
                        index.for_each_borrowed(*region, |path| {
                            let Some(item) = node_batches.get(path) else {
                                return;
                            };
                            if !rect_intersects(item.bounds, *region) {
                                return;
                            }
                            let has_clip = item
                                .batches
                                .iter()
                                .any(|batch| matches!(batch, GpuBatch::Clip(_, _, _)));
                            if has_clip {
                                flush_tile_indirect!();
                                encode_replay(&ReplayBatch::ResetClip);
                            }
                            for batch in &item.batches {
                                let Some(range) = indirect_draw_range(batch) else {
                                    flush_tile_indirect!();
                                    encode_replay(&ReplayBatch::Borrowed(batch));
                                    continue;
                                };
                                if let Some((previous, previous_range)) = pending_indirect {
                                    if let Some(merged) = merge_indirect_draw_ranges(
                                        previous,
                                        previous_range,
                                        batch,
                                        range,
                                    ) {
                                        pending_indirect = Some((previous, merged));
                                        continue;
                                    }
                                    flush_tile_indirect!();
                                }
                                pending_indirect = Some((batch, range));
                            }
                            if has_clip {
                                flush_tile_indirect!();
                                encode_replay(&ReplayBatch::ResetClip);
                            }
                        });
                    flush_tile_indirect!();
                }
            } else {
                let table = state
                    .retained_submission_batches
                    .as_ref()
                    .expect("retained GPU batches have a submission table");
                let mut pending_indirect: Option<(&GpuBatch, IndirectDrawRange)> = None;
                macro_rules! flush_pending_indirect {
                    () => {
                        if let Some((batch, range)) = pending_indirect.take() {
                            encode_replay(&ReplayBatch::BorrowedIndirectRun(batch, range));
                        }
                    };
                }
                table.for_each_entry(|path, entry| {
                    let Some(item) = node_batches.get(path) else {
                        return;
                    };
                    debug_assert_eq!(entry.node_id, item.node_id);
                    if entry.has_clip {
                        flush_pending_indirect!();
                        encode_replay(&ReplayBatch::ResetClip);
                    }
                    for batch in &item.batches {
                        let Some(range) = indirect_draw_range(batch) else {
                            flush_pending_indirect!();
                            encode_replay(&ReplayBatch::Borrowed(batch));
                            continue;
                        };
                        if let Some((previous, previous_range)) = pending_indirect {
                            if let Some(merged) =
                                merge_indirect_draw_ranges(previous, previous_range, batch, range)
                            {
                                pending_indirect = Some((previous, merged));
                                continue;
                            }
                            flush_pending_indirect!();
                        }
                        pending_indirect = Some((batch, range));
                    }
                    if entry.has_clip {
                        flush_pending_indirect!();
                        encode_replay(&ReplayBatch::ResetClip);
                    }
                });
                flush_pending_indirect!();
            }
            drop(pass);
            prune_transform_bindings(state);
        }
        {
            // The swap-chain texture is not a persistent render target, so it
            // cannot safely be incrementally loaded on Metal. When the
            // backend exposes COPY_DST, compose the retained GPU canvas into
            // the acquired frame with a texture copy. This keeps composition
            // entirely on the GPU and avoids the full-screen fragment pass.
            // Some backends do not expose COPY_DST for presentation textures;
            // retain the shader fallback for those platforms.
            if state.direct_copy_present {
                encoder.copy_texture_to_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &state.canvas,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::TexelCopyTextureInfo {
                        texture: &frame.texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::Extent3d {
                        width: state.size.width.max(1),
                        height: state.size.height.max(1),
                        depth_or_array_layers: 1,
                    },
                );
            } else {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("zui-render canvas composite pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    occlusion_query_set: None,
                    timestamp_writes: None,
                    multiview_mask: None,
                });
                pass.set_pipeline(&state.blit_pipeline);
                pass.set_bind_group(0, &state.blit_bind_group, &[]);
                set_scissor(&mut pass, None, state.size, state.scale_factor);
                pass.draw(0..3, 0..1);
            }
        }
        state.has_contents = true;
        state.last_frame_stats = frame_stats;
        state.retained_gpu_items = Some(retained_gpu_items);
        self.queue.submit(std::iter::once(encoder.finish()));
        // Glyph cache eviction is deliberately deferred until the complete
        // frame has been submitted. Text construction may touch hundreds of
        // glyphs; scanning the LRU per character is both expensive and can
        // recycle a glyph before its just-built batch reaches the GPU.
        self.resources.evict_glyphs();
        frame.present();
        Ok(())
    }

    /// Internal retained rendering path. Widget ids are resolved to
    /// RenderNode paths before this point, so updates rebuild only affected
    /// retained subtrees unless the scene explicitly requests a full rebuild.
    fn render_node_with_damage_regions_indexed(
        &mut self,
        window: WindowId,
        node: &RenderNode,
        damage_regions: &[Rect],
        clear: Color,
        dirty_paths: &[Vec<usize>],
        force_scene_rebuild: bool,
    ) -> Result<(), RenderError> {
        let mut retained = self
            .surfaces
            .get_mut(&window)
            .and_then(|state| state.retained_items.take())
            .unwrap_or_default();
        let initial_scene = retained.is_empty();
        let scene_changed = force_scene_rebuild || node.dirty.is_dirty() || initial_scene;
        let mut changed_paths = Vec::new();
        if scene_changed {
            if force_scene_rebuild || initial_scene || dirty_paths.is_empty() {
                let mut path = Vec::new();
                update_retained_subtree(
                    node,
                    &mut path,
                    &mut retained,
                    Transform::IDENTITY,
                    None,
                    1.0,
                    &[],
                    force_scene_rebuild || initial_scene,
                    &mut changed_paths,
                );
            } else {
                let mut paths = dirty_paths.to_vec();
                paths.sort_by_key(Vec::len);
                paths.dedup();
                for path in paths {
                    if !changed_paths
                        .iter()
                        .any(|ancestor| path_starts_with(&path, ancestor))
                    {
                        update_retained_dirty_path(node, &path, &mut retained, &mut changed_paths);
                    }
                }
            }
        }
        if scene_changed {
            if let Some(state) = self.surfaces.get_mut(&window) {
                if initial_scene || state.spatial_index.is_none() {
                    state.spatial_index = Some(SpatialIndex::build(&retained));
                } else if let Some(index) = state.spatial_index.as_mut() {
                    index.update_subtrees(&retained, &changed_paths);
                }
                if initial_scene || state.tile_submission_index.is_none() {
                    state.tile_submission_index =
                        Some(TileSubmissionIndex::build(&retained, state.scale_factor));
                } else if let Some(index) = state.tile_submission_index.as_mut() {
                    index.update_subtrees(&retained, &changed_paths);
                }
            }
        }
        let gpu_scene_changed = scene_changed
            || self
                .surfaces
                .get(&window)
                .and_then(|state| state.retained_gpu_items.as_ref())
                .is_none();
        if gpu_scene_changed {
            // Materialize GPU data only for new or changed retained nodes. The
            // replay function consumes this table directly, so a clean partial
            // redraw no longer walks every RenderNode to assemble batches.
            let old_gpu_items = self
                .surfaces
                .get_mut(&window)
                .and_then(|state| state.retained_gpu_items.take())
                .unwrap_or_default();
            let mut retained_gpu_items = old_gpu_items;
            let (render_size, scale_factor) = {
                let state = self
                    .surfaces
                    .get(&window)
                    .ok_or(RenderError::SurfaceNotAttached(window))?;
                let scale = state.scale_factor.0.max(1.0);
                (
                    PhysicalSize {
                        width: (state.size.width as f64 / scale).round().max(1.0) as u32,
                        height: (state.size.height as f64 / scale).round().max(1.0) as u32,
                    },
                    state.scale_factor.0 as f32,
                )
            };
            if changed_paths.is_empty() {
                changed_paths.push(Vec::new());
            }
            changed_paths.sort_by_key(Vec::len);
            changed_paths.dedup();
            let mut minimal_paths: Vec<Vec<usize>> = Vec::new();
            for path in changed_paths {
                if !minimal_paths
                    .iter()
                    .any(|ancestor| path_starts_with(&path, ancestor))
                {
                    minimal_paths.push(path);
                }
            }
            for changed_path in &minimal_paths {
                // BTreeMap keeps descendants contiguous after their prefix.
                // Remove exactly that span instead of filtering every retained
                // item on each dirty subtree update.
                loop {
                    let next_path = retained_gpu_items
                        .range(changed_path.clone()..)
                        .next()
                        .map(|(path, _)| path.clone());
                    let Some(path) = next_path.filter(|path| path_starts_with(path, changed_path))
                    else {
                        break;
                    };
                    let item = retained_gpu_items
                        .remove(&path)
                        .expect("path was read from retained item table");
                    self.resources.release_all(&item.resources);
                    let state = self
                        .surfaces
                        .get_mut(&window)
                        .ok_or(RenderError::SurfaceNotAttached(window))?;
                    for allocation in item.vertex_allocations {
                        state.vertex_arenas.release(allocation);
                    }
                    for allocation in item.index_allocations {
                        state.index_arena.release(allocation);
                    }
                    for allocation in item.indirect_allocations {
                        state.indirect_arena.release(allocation);
                    }
                }
                for (path, item) in retained.range(changed_path.clone()..) {
                    if !path_starts_with(path, changed_path) {
                        break;
                    }
                    PaintCommand::validate_sequence(&item.commands)?;
                    for command in item.commands.iter() {
                        if let PaintCommand::Image { image, .. } = command {
                            if !self.resources.contains_image(*image) {
                                return Err(RenderError::MissingImage(*image));
                            }
                        }
                    }
                    let mut batches = Vec::new();
                    for clip in &item.clips {
                        batches.push(self.build_clip_batch(&clip.shape, clip.transform));
                    }
                    batches.extend(self.build_render_batches(
                        &item.commands,
                        render_size,
                        scale_factor,
                        item.transform,
                        item.opacity,
                    ));
                    let mut gpu_batches = {
                        let state = self
                            .surfaces
                            .get_mut(&window)
                            .ok_or(RenderError::SurfaceNotAttached(window))?;
                        build_gpu_batches(
                            &self.device,
                            &self.queue,
                            &mut state.vertex_arenas,
                            &mut state.index_arena,
                            batches,
                        )
                    };
                    // Keep every source image alive before compatible atlas
                    // page draws are merged. A grouped batch stores one page
                    // material, while it may contain glyphs from many image
                    // resources on that page.
                    let gpu_batch_resources = batch_resource_handles(&gpu_batches);
                    gpu_batches = group_ordered_draws(gpu_batches);
                    // Transform bindings belong to the retained node update,
                    // not to frame replay. Unchanged nodes keep their
                    // bindings, so a partial frame never pre-scans every
                    // candidate batch merely to prepare uniforms.
                    {
                        let state = self
                            .surfaces
                            .get_mut(&window)
                            .ok_or(RenderError::SurfaceNotAttached(window))?;
                        for batch in &gpu_batches {
                            if let Some(transform) = gpu_batch_transform(batch) {
                                transform_bind_group_for(
                                    state,
                                    &self.device,
                                    transform,
                                    render_size,
                                );
                            }
                        }
                    }
                    let indirect_allocations = if self.indirect_execution {
                        let state = self
                            .surfaces
                            .get_mut(&window)
                            .ok_or(RenderError::SurfaceNotAttached(window))?;
                        prepare_indirect_draws(
                            &mut gpu_batches,
                            &mut state.indirect_arena,
                            &self.device,
                            &self.queue,
                            false,
                        )
                    } else {
                        Vec::new()
                    };
                    let vertex_allocations = batch_vertex_allocations(&gpu_batches);
                    let index_allocations = batch_index_allocations(&gpu_batches);
                    let mut resources = item_resource_handles(item);
                    for handle in gpu_batch_resources {
                        if !resources.contains(&handle) {
                            resources.push(handle);
                        }
                    }
                    self.resources.retain_all(&resources);
                    retained_gpu_items.insert(
                        path.clone(),
                        RetainedGpuItem {
                            node_id: item.node_id,
                            bounds: item.bounds,
                            batches: gpu_batches,
                            vertex_allocations,
                            index_allocations,
                            indirect_allocations,
                            resources,
                        },
                    );
                }
            }
            if let Some(state) = self.surfaces.get_mut(&window) {
                let submissions = state
                    .retained_submission_batches
                    .get_or_insert_with(|| RetainedSubmissionTable::rebuild(&retained_gpu_items));
                submissions.replace_subtrees(&retained_gpu_items, &minimal_paths);
                state.retained_gpu_items = Some(retained_gpu_items);
            }
        }
        let damage_regions = coalesce_damage_for_spatial_index(
            damage_regions,
            self.surfaces
                .get(&window)
                .and_then(|state| state.spatial_index.as_ref()),
        );
        let result = self.render_retained_scene_with_damage_regions(window, &damage_regions, clear);
        if result.is_ok() {
            if let Some(state) = self.surfaces.get_mut(&window) {
                state.retained_items = Some(retained);
            }
        }
        result
    }

    /// Applies the single authoritative retained-scene update for this frame.
    pub fn render_scene(
        &mut self,
        window: WindowId,
        node: &RenderNode,
        clear: Color,
        index: &RenderNodeIndex,
        update: &SceneUpdate,
    ) -> Result<(), RenderError> {
        // An explicit full rebuild must reach the retained-scene cache as an
        // empty path. Previously this flag was only consumed by WidgetTree;
        // the renderer still received individual dirty paths and could keep
        // stale GPU batches for a rebuilt subtree.
        let versioned_submission = update.revision() != 0;
        let previous_revision = self
            .surfaces
            .get(&window)
            .ok_or(RenderError::SurfaceNotAttached(window))?
            .submitted_scene_revision;
        // Incremental replay is valid only if this surface consumed the
        // immediately preceding scene snapshot. A newly attached surface,
        // resize, dropped frame, or skipped submission automatically falls
        // back to rebuilding from the complete RenderNode root.
        let revision_gap =
            versioned_submission && previous_revision != Some(update.revision().saturating_sub(1));
        let full_rebuild = update.full_rebuild() || revision_gap;
        let dirty_paths = if full_rebuild {
            Vec::new()
        } else {
            update
                .dirty_node_ids()
                .filter_map(|id| index.path_for(id))
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        };
        let damage_regions = if full_rebuild {
            // A retained-scene rebuild must clear the complete persistent
            // canvas before replay. Treating the root bounds as a partial
            // damage region keeps LoadOp::Load active and leaves pixels from
            // removed or changed primitives behind.
            &[]
        } else {
            update.damage_regions()
        };
        let result = self.render_node_with_damage_regions_indexed(
            window,
            node,
            damage_regions,
            clear,
            &dirty_paths,
            full_rebuild,
        );
        if result.is_ok() && versioned_submission {
            if let Some(state) = self.surfaces.get_mut(&window) {
                state.submitted_scene_revision = Some(update.revision());
            }
        }
        result
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }
}

// CPU tessellation and GPU batch construction live in `draw_batches.rs`.

// GPU pipeline construction lives in `gpu_pipeline.rs`.
#[cfg(test)]
mod tests;
