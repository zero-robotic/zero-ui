//! GPU rendering owned by the application composition root.
//!
//! This crate deliberately exposes no `wgpu::Device` to widgets. Widgets produce
//! drawing data; `Renderer` owns the GPU and the per-window swap-chain surfaces.

use std::collections::{BTreeMap, HashMap};
use std::hash::{Hash, Hasher};
use std::sync::{Mutex, OnceLock};

use lyon_path::{math::point as lyon_point, Path as LyonPath};
use lyon_tessellation::{
    BuffersBuilder, FillOptions, FillRule as LyonFillRule, FillTessellator, FillVertex,
    StrokeOptions, StrokeTessellator, StrokeVertex, VertexBuffers,
};
use wgpu::util::DeviceExt;
use zui_core::{Color, Dip, PhysicalSize, Point, Rect, ScaleFactor, Size, WindowId};
use zui_platform::spi::RawWindowHandleProvider;

pub use wgpu;

mod error;
mod draw_batches;
mod gpu_pipeline;
mod paint;

pub use error::RenderError;
pub use paint::{ClipShape, FillRule, IconPath, LineSegment, PaintCommand, PathCommand};
use paint::transform_clip_shape;
use gpu_pipeline::*;
use draw_batches::*;

static SYSTEM_FONTS: OnceLock<Vec<fontdue::Font>> = OnceLock::new();
static TEXT_MEASURE_CACHE: OnceLock<Mutex<HashMap<(String, u32), Dip>>> = OnceLock::new();

/// Measures text using the same system font used by the renderer.
pub fn measure_text(text: &str, scale: u32) -> Dip {
    let scale = scale.max(1);
    let cache = TEXT_MEASURE_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(width) = cache
        .lock()
        .expect("text measure cache poisoned")
        .get(&(text.to_owned(), scale))
    {
        return *width;
    }
    let fonts = cached_system_fonts();
    let width = if !fonts.is_empty() {
        let size = (scale * 7) as f32;
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
        .insert((text.to_owned(), scale), width);
    width
}

// Paint commands and vector path types live in `paint.rs`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImageId(pub u64);

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
    image_bind_groups: HashMap<(WindowId, ImageId), wgpu::BindGroup>,
    last_used: HashMap<ImageId, u64>,
    clock: u64,
    max_gpu_images: usize,
    image_sampler: wgpu::Sampler,
    icons: HashMap<u64, IconPath>,
    budget: ResourceBudget,
    fonts: &'static [fontdue::Font],
    glyph_cache: HashMap<(usize, char, u32, u32), CachedGlyph>,
    font_cache: HashMap<char, Option<usize>>,
    path_cache: HashMap<u64, Vec<RectVertex>>,
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
            font_cache: HashMap::new(),
            path_cache: HashMap::new(),
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

    pub fn register_icon(&mut self, id: u64, path: IconPath) {
        self.icons.insert(id, path);
    }
    pub fn icon(&self, id: u64) -> Option<&IconPath> {
        self.icons.get(&id)
    }
    pub fn remove_icon(&mut self, id: u64) -> Option<IconPath> {
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
        let uv = [
            slot.origin.x as f32 / slot.atlas_width as f32,
            slot.origin.y as f32 / slot.atlas_height as f32,
            (slot.origin.x + image.width) as f32 / slot.atlas_width as f32,
            (slot.origin.y + image.height) as f32 / slot.atlas_height as f32,
        ];
        let uv_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("zui-render image atlas uv"),
            contents: bytemuck::cast_slice(&uv),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        self.gpu_images.insert(
            id,
            GpuImage {
                uv_buffer,
                bytes: image.rgba8.len(),
                slot,
            },
        );
        self.image_bind_groups
            .retain(|(_, image_id), _| *image_id != id);
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
        let Some(oldest) = self.last_used.iter().min_by_key(|(_, stamp)| *stamp).map(|(id, _)| *id) else {
            return false;
        };
        if let Some(image) = self.gpu_images.remove(&oldest) {
            if let Some(atlas) = self.image_atlases.get_mut(image.slot.page) {
                atlas.release(image.slot);
            }
        }
        self.last_used.remove(&oldest);
        self.image_bind_groups.retain(|(_, id), _| *id != oldest);
        self.reclaim_empty_atlas_tail();
        true
    }

    fn remove_image(&mut self, id: ImageId) -> Option<ImageResource> {
        if let Some(image) = self.gpu_images.remove(&id) {
            if let Some(atlas) = self.image_atlases.get_mut(image.slot.page) {
                atlas.release(image.slot);
            }
        }
        self.last_used.remove(&id);
        self.image_bind_groups
            .retain(|(_, image_id), _| *image_id != id);
        self.reclaim_empty_atlas_tail();
        self.cpu.remove_image(id)
    }

    /// Atlas page indices are stored in GPU image slots, so only trailing
    /// empty pages may be reclaimed without rebasing live resources.
    fn reclaim_empty_atlas_tail(&mut self) {
        while self.image_atlases.len() > 1 {
            let page = self.image_atlases.len() - 1;
            if self.gpu_images.values().any(|image| image.slot.page == page) {
                break;
            }
            self.image_atlases.pop();
        }
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
        let image = ImageResource::new(width.max(1), height.max(1), if rgba8.is_empty() {
            vec![0; 4]
        } else {
            rgba8
        })
        .expect("glyph bitmap dimensions are valid");
        self.register_image(device, queue, id, image);
        id
    }

    fn tessellate_path(&mut self, path: &IconPath) -> Vec<RectVertex> {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        format!("{path:?}").hash(&mut hasher);
        let key = hasher.finish();
        self.path_cache
            .entry(key)
            .or_insert_with(|| path_mask_vertices(path))
            .clone()
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

    fn bind_group(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        window: WindowId,
        id: ImageId,
        layout: &wgpu::BindGroupLayout,
    ) -> Option<wgpu::BindGroup> {
        if !self.gpu_images.contains_key(&id) {
            let image = self.cpu.image(id)?.clone();
            self.upload_gpu_image(device, queue, id, &image);
        }
        self.touch(id);
        if !self.image_bind_groups.contains_key(&(window, id)) {
            let image = self.gpu_images.get(&id)?;
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("zui-render image bind group"),
                layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(
                            &self.image_atlases.get(image.slot.page)?.view,
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.image_sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: image.uv_buffer.as_entire_binding(),
                    },
                ],
            });
            self.image_bind_groups.insert((window, id), bind_group);
        }
        self.image_bind_groups.get(&(window, id)).cloned()
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
    /// The originating widget identity, when this node was built by a Widget.
    pub source_id: Option<u64>,
    pub transform: Transform,
    pub clip: Option<ClipShape>,
    pub opacity: f32,
    pub commands: Vec<PaintCommand>,
    pub children: Vec<Self>,
    /// `false` while the node still stores an absolute placement transform.
    /// Cached nodes remain `true` when a parent subtree is rebuilt.
    pub coordinates_normalized: bool,
    pub dirty: DirtyState,
}

#[derive(Clone, Debug)]
struct RenderNodeItem {
    bounds: Rect,
    transform: Transform,
    opacity: f32,
    clips: Vec<RenderClip>,
    commands: Vec<PaintCommand>,
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

    fn update_subtrees(&mut self, items: &BTreeMap<Vec<usize>, RenderNodeItem>, paths: &[Vec<usize>]) {
        self.bounds.retain(|path, _| !paths.iter().any(|prefix| path_starts_with(path, prefix)));
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

    fn query(&self, region: Rect) -> Vec<Vec<usize>> {
        let (min_x, min_y, max_x, max_y) = self.cell_range(region);
        let mut result = Vec::new();
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                if let Some(paths) = self.cells.get(&(x, y)) {
                    result.extend(paths.iter().filter(|path| {
                        self.bounds
                            .get(*path)
                            .is_some_and(|bounds| rect_intersects(*bounds, region))
                    }).cloned());
                }
            }
        }
        result.sort();
        result.dedup();
        result
    }

    fn cell_range(&self, rect: Rect) -> (i32, i32, i32, i32) {
        let min_x = (rect.origin.x.0 / self.cell_size).floor() as i32;
        let min_y = (rect.origin.y.0 / self.cell_size).floor() as i32;
        let max_x = ((rect.origin.x.0 + rect.size.width.0) / self.cell_size).floor() as i32;
        let max_y = ((rect.origin.y.0 + rect.size.height.0) / self.cell_size).floor() as i32;
        (min_x, min_y, max_x, max_y)
    }

}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RenderNodeIndex {
    paths: HashMap<u64, Vec<usize>>,
}

impl RenderNodeIndex {
    pub fn path_for(&self, source_id: u64) -> Option<&[usize]> {
        self.paths.get(&source_id).map(Vec::as_slice)
    }

    pub fn node<'a>(&self, root: &'a RenderNode, source_id: u64) -> Option<&'a RenderNode> {
        let mut node = root;
        for index in self.path_for(source_id)? {
            node = node.children.get(*index)?;
        }
        Some(node)
    }
}

impl RenderNode {
    pub fn new(bounds: Rect) -> Self {
        Self {
            local_bounds: bounds,
            source_id: None,
            transform: Transform::IDENTITY,
            clip: None,
            opacity: 1.0,
            commands: Vec::new(),
            children: Vec::new(),
            coordinates_normalized: false,
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
        self.children.push(child);
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
            .map(|previous| previous.source_id == self.source_id)
            .unwrap_or(false);
        if can_match && !force_rebuild && dirty_paths.is_empty() {
            return previous.expect("previous node exists when matched").clone();
        }

        if let Some(previous) = previous.filter(|previous| previous.source_id == self.source_id) {
            let old_children = &previous.children;
            let children = std::mem::take(&mut self.children);
            self.children = children
                .into_iter()
                .enumerate()
                .map(|(index, child)| {
                    let child_paths = dirty_paths
                        .iter()
                        .filter_map(|path| {
                            (path.first().copied() == Some(index)).then(|| path[1..].to_vec())
                        })
                        .collect::<Vec<_>>();
                    child.reuse_clean_subtrees(old_children.get(index), &child_paths, force_rebuild)
                })
                .collect();
        }
        self
    }

    fn index_into(&self, index: &mut RenderNodeIndex, path: &mut Vec<usize>) {
        if let Some(source_id) = self.source_id {
            index.paths.insert(source_id, path.clone());
        }
        for (child_index, child) in self.children.iter().enumerate() {
            path.push(child_index);
            child.index_into(index, path);
            path.pop();
        }
    }

    pub fn child_mut(&mut self, index: usize) -> Option<&mut Self> {
        self.children.get_mut(index)
    }

    /// Marks a descendant as dirty and records the corresponding parent
    /// invalidation. This is the explicit propagation point for retained
    /// trees whose children are stored by value.
    pub fn mark_child_dirty(&mut self, index: usize, flags: DirtyFlags, region: Option<Rect>) {
        if let Some(child) = self.children.get_mut(index) {
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
        if let Some(child) = self.children.get_mut(path[0]) {
            child.mark_dirty_path(&path[1..], flags, region);
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty.is_dirty() || self.children.iter().any(Self::is_dirty)
    }

    pub fn accumulated_dirty_region(&self) -> Option<Rect> {
        let mut region = self.dirty.regions.union();
        for child in &self.children {
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

    pub fn set_source_id(&mut self, source_id: u64) {
        self.source_id = Some(source_id);
    }

    pub fn world_bounds(&self) -> Rect {
        self.transform.rect(self.local_bounds)
    }

    /// Stable fingerprint for retained GPU resources. It changes when this
    /// node's commands, state, placement, or any descendant changes.
    pub fn gpu_cache_key(&self) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.local_gpu_cache_key().hash(&mut hasher);
        for child in &self.children {
            child.gpu_cache_key().hash(&mut hasher);
        }
        hasher.finish()
    }

    fn local_gpu_cache_key(&self) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        format!(
            "{:?}{:?}{:?}{:?}{:?}{:?}",
            self.source_id,
            self.local_bounds,
            self.transform,
            self.clip,
            self.opacity,
            &self.commands
        )
        .hash(&mut hasher);
        hasher.finish()
    }

    /// Converts a tree whose nodes were arranged in window coordinates into
    /// local bounds plus transforms relative to the immediate parent.
    pub fn normalize_local_coordinates(&mut self) {
        self.normalize_from(Point::default());
    }

    fn normalize_from(&mut self, parent_world_origin: Point) {
        let world_origin = if self.coordinates_normalized {
            Point {
                x: Dip(parent_world_origin.x.0 + self.transform.matrix[4]),
                y: Dip(parent_world_origin.y.0 + self.transform.matrix[5]),
            }
        } else {
            let world_origin = self.transform.point(Point::default());
            self.transform.matrix[4] -= parent_world_origin.x.0;
            self.transform.matrix[5] -= parent_world_origin.y.0;
            self.coordinates_normalized = true;
            world_origin
        };
        for child in &mut self.children {
            child.normalize_from(world_origin);
        }
    }

    pub fn set_clip(&mut self, clip: Option<ClipShape>) {
        self.clip = clip;
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
        for child in &mut self.children {
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
    let node_clip_bounds = node
        .clip
        .as_ref()
        .map(|clip| transform_clip_shape(clip, transform).bounds());
    let clip = intersect_clip(parent_clip, node_clip_bounds);
    let opacity = parent_opacity * node.opacity;
    let mut clips = parent_clips.to_vec();
    if let Some(shape) = node.clip.clone() {
        clips.push(RenderClip { shape, transform });
    }
    let inherited_clips = clips.clone();
    let item = RenderNodeItem {
        bounds: clip
            .map(|clip| intersect_rect(transform.rect(node.local_bounds), clip))
            .unwrap_or_else(|| transform.rect(node.local_bounds)),
        transform,
        opacity,
        clips,
        // Commands remain in the node's local coordinate system. The
        // renderer resolves them only while producing GPU vertices.
        commands: node.commands.clone(),
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

fn coalesce_damage_for_spatial_index(regions: &[Rect], index: Option<&SpatialIndex>) -> Vec<Rect> {
    let mut result = regions.to_vec();
    let mut changed = true;
    while changed {
        changed = false;
        'outer: for left in 0..result.len() {
            for right in (left + 1)..result.len() {
                if index
                    .map(|index| {
                        index.query(result[left]).into_iter().any(|item| {
                            index
                                .bounds
                                .get(&item)
                                .is_some_and(|bounds| rect_intersects(*bounds, result[right]))
                        })
                    })
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
        &mut self.node.commands
    }

    pub fn add_child(&mut self, child: RenderNode) {
        self.node.add_child(child);
    }

    pub fn transform(&mut self, transform: Transform) -> &mut Self {
        self.node.transform = transform;
        self
    }

    pub fn source_id(&mut self, source_id: u64) -> &mut Self {
        self.node.set_source_id(source_id);
        self
    }

    pub fn clip(&mut self, clip: Option<ClipShape>) -> &mut Self {
        self.node.clip = clip;
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
    stencil_pipeline: wgpu::RenderPipeline,
    stencil_mask_pipeline: wgpu::RenderPipeline,
    /// Per-transform uniform resources. A draw must never share a mutable
    /// uniform with a later draw in the same command buffer.
    transform_bind_group_layout: wgpu::BindGroupLayout,
    transform_bindings: HashMap<[u32; 6], TransformBinding>,
    canvas: wgpu::Texture,
    canvas_view: wgpu::TextureView,
    stencil: wgpu::Texture,
    stencil_view: wgpu::TextureView,
    stencil_reset: wgpu::Buffer,
    blit_pipeline: wgpu::RenderPipeline,
    blit_bind_group: wgpu::BindGroup,
    /// Whether the swap-chain texture can be the destination of a GPU copy.
    /// When available, presentation uses a texture copy instead of a
    /// full-screen fragment composite.
    direct_copy_present: bool,
    scale_factor: ScaleFactor,
    has_contents: bool,
    /// GPU vertex buffers keyed by the retained render-node/display-list
    /// fingerprint. Unchanged nodes reuse their buffers across frames.
    node_gpu_cache: HashMap<u64, GpuBatchCacheEntry>,
    cache_clock: u64,
    max_gpu_cache_entries: usize,
    /// Retained renderer items. A clean RenderNode reuses this ordered list
    /// without walking the widget/render tree again.
    retained_items: Option<BTreeMap<Vec<usize>, RenderNodeItem>>,
    /// GPU draw data keyed by retained-node path. Only nodes whose render key
    /// changes rebuild these batches; partial replay reads this table directly.
    retained_gpu_items: Option<BTreeMap<Vec<usize>, RetainedGpuItem>>,
    spatial_index: Option<SpatialIndex>,
}

struct GpuBatchCacheEntry {
    batches: Vec<GpuBatch>,
    last_used: u64,
}

#[derive(Clone)]
struct RetainedGpuItem {
    bounds: Rect,
    batches: Vec<GpuBatch>,
}

struct TransformBinding {
    _buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

fn take_gpu_cache(state: &mut SurfaceState, key: u64) -> Option<Vec<GpuBatch>> {
    state.cache_clock = state.cache_clock.wrapping_add(1);
    state.node_gpu_cache.remove(&key).map(|entry| entry.batches)
}

fn insert_gpu_cache(state: &mut SurfaceState, key: u64, batches: Vec<GpuBatch>) {
    state.cache_clock = state.cache_clock.wrapping_add(1);
    let last_used = state.cache_clock;
    state
        .node_gpu_cache
        .insert(key, GpuBatchCacheEntry { batches, last_used });
    while state.node_gpu_cache.len() > state.max_gpu_cache_entries {
        let Some(oldest_key) = state
            .node_gpu_cache
            .iter()
            .min_by_key(|(_, entry)| entry.last_used)
            .map(|(key, _)| *key)
        else {
            break;
        };
        state.node_gpu_cache.remove(&oldest_key);
    }
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
    Rect(Vec<RectVertex>, Transform),
    Rounded(Vec<RoundedRectVertex>, Transform),
    Line(Vec<LineVertex>, Transform),
    Image {
        image: ImageId,
        vertices: Vec<ImageVertex>,
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
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BatchKind {
    Rect,
    Rounded,
    Line,
    Image(ImageId),
}

#[derive(Clone)]
enum GpuBatch {
    Draw(BatchKind, wgpu::Buffer, u32, Vec<u8>, Transform),
    Clip(ClipGeometry, Option<(wgpu::Buffer, u32)>, Transform),
    Scissor(Rect),
}

fn rect_batch(batches: &mut Vec<RenderBatch>, transform: Transform) -> &mut Vec<RectVertex> {
    if !matches!(batches.last(), Some(RenderBatch::Rect(_, current)) if *current == transform) {
        batches.push(RenderBatch::Rect(Vec::new(), transform));
    }
    match batches.last_mut().expect("rect batch was just added") {
        RenderBatch::Rect(vertices, _) => vertices,
        _ => unreachable!(),
    }
}

fn rounded_batch(
    batches: &mut Vec<RenderBatch>,
    transform: Transform,
) -> &mut Vec<RoundedRectVertex> {
    if !matches!(batches.last(), Some(RenderBatch::Rounded(_, current)) if *current == transform) {
        batches.push(RenderBatch::Rounded(Vec::new(), transform));
    }
    match batches.last_mut().expect("rounded batch was just added") {
        RenderBatch::Rounded(vertices, _) => vertices,
        _ => unreachable!(),
    }
}

fn line_batch(batches: &mut Vec<RenderBatch>, transform: Transform) -> &mut Vec<LineVertex> {
    if !matches!(batches.last(), Some(RenderBatch::Line(_, current)) if *current == transform) {
        batches.push(RenderBatch::Line(Vec::new(), transform));
    }
    match batches.last_mut().expect("line batch was just added") {
        RenderBatch::Line(vertices, _) => vertices,
        _ => unreachable!(),
    }
}

fn image_batch(
    batches: &mut Vec<RenderBatch>,
    image: ImageId,
    transform: Transform,
) -> &mut Vec<ImageVertex> {
    if !matches!(batches.last(), Some(RenderBatch::Image { image: current, transform: current_transform, .. }) if *current == image && *current_transform == transform)
    {
        batches.push(RenderBatch::Image {
            image,
            vertices: Vec::new(),
            transform,
        });
    }
    match batches.last_mut().expect("image batch was just added") {
        RenderBatch::Image { vertices, .. } => vertices,
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
}

struct GpuImage {
    uv_buffer: wgpu::Buffer,
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

    pub fn resource_manager(&mut self) -> &mut ResourceManager {
        &mut self.resources
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
        let stencil_pipeline = create_stencil_pipeline(&self.device, config.format);
        let stencil_mask_pipeline =
            create_stencil_mask_pipeline(&self.device, config.format, &transform_layout);
        let (canvas, canvas_view) = create_canvas(&self.device, size, config.format);
        let (stencil, stencil_view) = create_stencil(&self.device, size);
        let stencil_reset = create_stencil_reset_buffer(&self.device);
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
                stencil_pipeline,
                stencil_mask_pipeline,
                transform_bind_group_layout: transform_layout,
                transform_bindings: HashMap::new(),
                canvas,
                canvas_view,
                stencil,
                stencil_view,
                stencil_reset,
                blit_pipeline,
                blit_bind_group,
                direct_copy_present,
                scale_factor,
                has_contents: false,
                node_gpu_cache: HashMap::new(),
                cache_clock: 0,
                max_gpu_cache_entries: 256,
                retained_items: None,
                retained_gpu_items: None,
                spatial_index: None,
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
        let stencil_pipeline = create_stencil_pipeline(&self.device, config.format);
        let stencil_mask_pipeline =
            create_stencil_mask_pipeline(&self.device, config.format, &transform_layout);
        let (canvas, canvas_view) = create_canvas(&self.device, size, config.format);
        let (stencil, stencil_view) = create_stencil(&self.device, size);
        let stencil_reset = create_stencil_reset_buffer(&self.device);
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
                stencil_pipeline,
                stencil_mask_pipeline,
                transform_bind_group_layout: transform_layout,
                transform_bindings: HashMap::new(),
                canvas,
                canvas_view,
                stencil,
                stencil_view,
                stencil_reset,
                blit_pipeline,
                blit_bind_group,
                direct_copy_present,
                scale_factor,
                has_contents: false,
                node_gpu_cache: HashMap::new(),
                cache_clock: 0,
                max_gpu_cache_entries: 256,
                retained_items: None,
                retained_gpu_items: None,
                spatial_index: None,
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
        state.node_gpu_cache.clear();
        state.transform_bindings.clear();
        state.retained_items = None;
        state.retained_gpu_items = None;
        state.spatial_index = None;
        state.cache_clock = 0;
        Ok(())
    }

    pub fn detach_surface(&mut self, window: WindowId) {
        self.surfaces.remove(&window);
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
                PaintCommand::Transform(next) => {
                    command_transform = compose_transform(command_transform, *next);
                    continue;
                }
                PaintCommand::PushTransform(next) => {
                    transform_stack.push(command_transform);
                    command_transform = compose_transform(command_transform, *next);
                    continue;
                }
                PaintCommand::PopTransform => {
                    command_transform = transform_stack.pop().expect("validated transform stack");
                    continue;
                }
                PaintCommand::Opacity(value) => {
                    opacity *= value.clamp(0.0, 1.0);
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
                PaintCommand::Clip { shape } | PaintCommand::PushClip(shape) => {
                    let geometry = match shape {
                            ClipShape::Rect(rect) => ClipGeometry::Rect(*rect),
                            ClipShape::RoundedRect { rect, radius } => ClipGeometry::Rounded {
                                rect: *rect,
                                radius: *radius,
                            },
                            ClipShape::Path { path } => ClipGeometry::Path {
                                bounds: path_bounds(path),
                                vertices: self.resources.tessellate_path(path),
                            },
                        };
                    let transform = compose_transform(clip_transform, command_transform);
                    batches.push(RenderBatch::Clip(geometry.clone(), transform));
                    if matches!(command, PaintCommand::PushClip(_)) {
                        clip_stack.push((geometry, transform));
                    }
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
                    append_rect(
                        rect_batch(&mut batches, transform),
                        *rect,
                        apply_opacity(*color, opacity),
                    );
                }
                PaintCommand::RoundedRect {
                    rect,
                    radius,
                    color,
                } => append_rounded_rect(
                    rounded_batch(&mut batches, transform),
                    *rect,
                    *radius,
                    apply_opacity(*color, opacity),
                ),
                PaintCommand::Line {
                    start,
                    end,
                    width,
                    color,
                } => append_line(
                    line_batch(&mut batches, transform),
                    *start,
                    *end,
                    *width,
                    apply_opacity(*color, opacity),
                ),
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
                    for segment in &path.segments {
                        append_line(
                            line_batch(&mut batches, transform),
                            segment.start,
                            segment.end,
                            *stroke,
                            apply_opacity(*color, opacity),
                        );
                    }
                }
                PaintCommand::PathFill { path, color } => {
                    rect_batch(&mut batches, transform).extend(path_fill_vertices(
                        path,
                        apply_opacity(*color, opacity),
                    ));
                }
                PaintCommand::PathStroke { path, width, color } => {
                    rect_batch(&mut batches, transform).extend(path_stroke_vertices(
                        path,
                        *width,
                        apply_opacity(*color, opacity),
                    ));
                }
                PaintCommand::Image {
                    rect,
                    image,
                    opacity: image_opacity,
                } => append_image(
                    image_batch(&mut batches, *image, transform),
                    *rect,
                    *image_opacity * opacity,
                    Color::WHITE,
                ),
                PaintCommand::Transform(_)
                | PaintCommand::Opacity(_)
                | PaintCommand::PushTransform(_)
                | PaintCommand::PopTransform
                | PaintCommand::PushOpacity(_)
                | PaintCommand::PopOpacity
                | PaintCommand::Clear(_)
                | PaintCommand::PopClip => {}
                PaintCommand::Clip { .. } | PaintCommand::PushClip(_) => unreachable!(),
            }
        }
        batches
    }

    fn render_commands_with_damage_regions_key(
        &mut self,
        window: WindowId,
        commands: &[PaintCommand],
        damage_regions: &[Rect],
        cache_key: Option<u64>,
        segments: Option<&[RenderNodeItem]>,
    ) -> Result<(), RenderError> {
        PaintCommand::validate_sequence(commands)?;
        for command in commands {
            if let PaintCommand::Image { image, .. } = command {
                if !self.resources.contains_image(*image) {
                    return Err(RenderError::MissingImage(*image));
                }
            }
        }
        let (render_size, render_scale_factor) = {
            let surface = self
                .surfaces
                .get(&window)
                .ok_or(RenderError::SurfaceNotAttached(window))?;
            let scale = surface.scale_factor.0.max(1.0);
            (
                PhysicalSize {
                    width: (surface.size.width as f64 / scale).round().max(1.0) as u32,
                    height: (surface.size.height as f64 / scale).round().max(1.0) as u32,
                },
                surface.scale_factor.0 as f32,
            )
        };
        let batches = if segments.is_none() {
            Some(self.build_render_batches(
                commands,
                render_size,
                render_scale_factor,
                Transform::IDENTITY,
                1.0,
            ))
        } else {
            None
        };
        let state = self
            .surfaces
            .get_mut(&window)
            .ok_or(RenderError::SurfaceNotAttached(window))?;
        // Temporarily move retained GPU items out of SurfaceState. This keeps
        // the replay table stable while the pass mutates other per-surface
        // caches (transform bindings) without cloning every node batch.
        let retained_gpu_items = segments
            .is_some()
            .then(|| state.retained_gpu_items.take())
            .flatten();
        // An empty damage list means full invalidation at the UI layer. On a
        // newly attached (or resized) surface, make that explicit so the
        // first frame cannot take a partial replay/composite path.
        let full_surface_region = Rect {
            origin: Point::default(),
            size: Size {
                width: Dip(state.size.width as f32 / state.scale_factor.0.max(1.0) as f32),
                height: Dip(state.size.height as f32 / state.scale_factor.0.max(1.0) as f32),
            },
        };
        let full_frame = !state.has_contents && damage_regions.is_empty();
        let damage_regions = if full_frame {
            std::slice::from_ref(&full_surface_region)
        } else {
            damage_regions
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
        let batch_hash = cache_key.unwrap_or_else(|| command_list_hash(commands));
        let clear = commands
            .iter()
            .rev()
            .find_map(|command| match command {
                PaintCommand::Clear(color) => Some(wgpu::Color {
                    r: color.r as f64,
                    g: color.g as f64,
                    b: color.b as f64,
                    a: color.a as f64,
                }),
                _ => None,
            })
            .unwrap_or(wgpu::Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("zui-render frame"),
            });
        {
            let node_batches = segments.map(|_| {
                retained_gpu_items
                    .as_ref()
                    .expect("retained GPU batches are prepared before replay")
            });
            let gpu_batches = if let Some(node_batches) = &node_batches {
                let mut all = Vec::new();
                for item in node_batches.values() {
                    let batches = &item.batches;
                    let has_clip = batches
                        .iter()
                        .any(|batch| matches!(batch, GpuBatch::Clip(_, _, _)));
                    if has_clip {
                        all.push(GpuBatch::Clip(
                            ClipGeometry::Reset,
                            None,
                            Transform::IDENTITY,
                        ));
                    }
                    all.extend(batches.iter().cloned());
                    if has_clip {
                        all.push(GpuBatch::Clip(
                            ClipGeometry::Reset,
                            None,
                            Transform::IDENTITY,
                        ));
                    }
                }
                coalesce_gpu_batches(&self.device, all)
            } else {
                take_gpu_cache(state, batch_hash).unwrap_or_else(|| {
                    build_gpu_batches(
                        &self.device,
                        batches.expect("command batches are built without node items"),
                    )
                })
            };
            let replay_batches = if state.has_contents && !damage_regions.is_empty() {
                if let Some(node_batches) = &node_batches {
                    let mut replay = Vec::new();
                    for region in damage_regions {
                        replay.push(GpuBatch::Scissor(*region));
                        let candidates = state
                            .spatial_index
                            .as_ref()
                            .map(|index| index.query(*region))
                            .unwrap_or_else(|| node_batches.keys().cloned().collect());
                        for path in candidates {
                            let Some(item) = node_batches.get(&path) else {
                                continue;
                            };
                            if rect_intersects(item.bounds, *region) {
                                let batches = &item.batches;
                                let has_clip = batches
                                    .iter()
                                    .any(|batch| matches!(batch, GpuBatch::Clip(_, _, _)));
                                if has_clip {
                                    replay.push(GpuBatch::Clip(
                                        ClipGeometry::Reset,
                                        None,
                                        Transform::IDENTITY,
                                    ));
                                }
                                replay.extend(batches.iter().cloned());
                                if has_clip {
                                    replay.push(GpuBatch::Clip(
                                        ClipGeometry::Reset,
                                        None,
                                        Transform::IDENTITY,
                                    ));
                                }
                            }
                        }
                    }
                    coalesce_gpu_batches(&self.device, replay)
                } else {
                    damage_regions
                        .iter()
                        .flat_map(|region| {
                            std::iter::once(GpuBatch::Scissor(*region))
                                .chain(gpu_batches.iter().cloned())
                        })
                        .collect::<Vec<_>>()
                }
            } else {
                gpu_batches.clone()
            };
            let mut transform_groups = HashMap::new();
            for batch in &replay_batches {
                let transform = match batch {
                    GpuBatch::Draw(_, _, _, _, transform) | GpuBatch::Clip(_, _, transform) => {
                        Some(*transform)
                    }
                    GpuBatch::Scissor(_) => None,
                };
                if let Some(transform) = transform {
                    transform_groups
                        .entry(transform_key(transform))
                        .or_insert_with(|| {
                            transform_bind_group_for(state, &self.device, transform, render_size)
                        });
                }
            }
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("zui-render clear pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &state.canvas_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: if !damage_regions.is_empty() && state.has_contents {
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
            for batch in &replay_batches {
                if let GpuBatch::Scissor(region) = batch {
                    active_clip = None;
                    active_damage = Some(*region);
                    active_clip_depth = 0;
                    set_scissor(&mut pass, None, state.size, state.scale_factor);
                    pass.set_pipeline(&state.stencil_pipeline);
                    pass.set_stencil_reference(0);
                    pass.set_vertex_buffer(0, state.stencil_reset.slice(..));
                    pass.draw(0..6, 0..1);
                    continue;
                }
                if let GpuBatch::Clip(geometry, mask, transform) = batch {
                    if let Some((buffer, count)) = mask {
                        pass.set_pipeline(match geometry {
                            ClipGeometry::Rounded { .. } => &state.stencil_rounded_pipeline,
                            _ => &state.stencil_mask_pipeline,
                        });
                        pass.set_stencil_reference(active_clip_depth);
                        pass.set_bind_group(
                            0,
                            transform_groups
                                .get(&transform_key(*transform))
                                .expect("clip transform binding is prepared"),
                            &[],
                        );
                        pass.set_vertex_buffer(0, buffer.slice(..));
                        pass.draw(0..*count, 0..1);
                        active_clip_depth = active_clip_depth.saturating_add(1);
                    } else {
                        set_scissor(&mut pass, None, state.size, state.scale_factor);
                        pass.set_pipeline(&state.stencil_pipeline);
                        pass.set_stencil_reference(0);
                        pass.set_vertex_buffer(0, state.stencil_reset.slice(..));
                        pass.draw(0..6, 0..1);
                        active_clip_depth = 0;
                    }
                    active_clip = match geometry {
                        ClipGeometry::Rect(rect) | ClipGeometry::Rounded { rect, .. } => {
                            Some(transform.rect(*rect))
                        }
                        ClipGeometry::Path { bounds, .. } => Some(transform.rect(*bounds)),
                        ClipGeometry::Reset => None,
                    };
                    continue;
                }
                let (kind, draws, transform): (&BatchKind, Vec<(&wgpu::Buffer, u32)>, Transform) =
                    match batch {
                        GpuBatch::Draw(kind, buffer, count, _, transform) => {
                            (kind, vec![(buffer, *count)], *transform)
                        }
                        _ => continue,
                    };
                let scissor = intersect_clip(active_clip, active_damage);
                set_scissor(&mut pass, scissor, state.size, state.scale_factor);
                pass.set_stencil_reference(active_clip_depth);
                pass.set_pipeline(match kind {
                    BatchKind::Rect => &state.pipeline,
                    BatchKind::Rounded => &state.rounded_pipeline,
                    BatchKind::Line => &state.line_pipeline,
                    BatchKind::Image(_) => &state.image_pipeline,
                });
                pass.set_bind_group(
                    0,
                    transform_groups
                        .get(&transform_key(transform))
                        .expect("draw transform binding is prepared"),
                    &[],
                );
                if let BatchKind::Image(image) = *kind {
                    let layout = state.image_pipeline.get_bind_group_layout(1);
                    if let Some(bind_group) =
                        self.resources
                            .bind_group(&self.device, &self.queue, window, image, &layout)
                    {
                        pass.set_bind_group(1, &bind_group, &[]);
                        for (vertex_buffer, count) in draws {
                            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                            pass.draw(0..count, 0..1);
                        }
                    }
                } else {
                    for (vertex_buffer, count) in draws {
                        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                        pass.draw(0..count, 0..1);
                    }
                }
            }
            if node_batches.is_none() {
                insert_gpu_cache(state, batch_hash, gpu_batches);
            }
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
        if let Some(items) = retained_gpu_items {
            state.retained_gpu_items = Some(items);
        }
        self.queue.submit(std::iter::once(encoder.finish()));
        frame.present();
        Ok(())
    }

    /// Renders a retained node tree. The node can be kept by the UI layer
    /// between frames; unchanged fingerprints reuse the renderer's GPU batch
    /// cache while damage regions limit the canvas writes.
    pub fn render_node_with_damage_regions(
        &mut self,
        window: WindowId,
        node: &RenderNode,
        damage_regions: &[Rect],
        clear: Color,
    ) -> Result<(), RenderError> {
        self.render_node_with_damage_regions_indexed(window, node, damage_regions, clear, None)
    }

    /// Same retained rendering path, with explicit WidgetId-derived node
    /// paths supplied by `WidgetTree::render_index`. These paths let the GPU
    /// retained table update only the invalidated RenderNode subtrees.
    pub fn render_node_with_damage_regions_indexed(
        &mut self,
        window: WindowId,
        node: &RenderNode,
        damage_regions: &[Rect],
        clear: Color,
        dirty_paths: Option<&[Vec<usize>]>,
    ) -> Result<(), RenderError> {
        let mut retained = self
            .surfaces
            .get_mut(&window)
            .and_then(|state| state.retained_items.take())
            .unwrap_or_default();
        let initial_scene = retained.is_empty();
        let scene_changed = node.dirty.is_dirty() || initial_scene;
        let mut changed_paths = Vec::new();
        if scene_changed {
            let mut path = Vec::new();
            update_retained_subtree(
                node,
                &mut path,
                &mut retained,
                Transform::IDENTITY,
                None,
                1.0,
                &[],
                initial_scene,
                &mut changed_paths,
            );
        }
        if let Some(dirty_paths) = dirty_paths.filter(|_| !initial_scene) {
            changed_paths.extend_from_slice(dirty_paths);
        }
        if scene_changed {
            if let Some(state) = self.surfaces.get_mut(&window) {
                if initial_scene || state.spatial_index.is_none() {
                    state.spatial_index = Some(SpatialIndex::build(&retained));
                } else if let Some(index) = state.spatial_index.as_mut() {
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
            for changed_path in minimal_paths {
                retained_gpu_items.retain(|path, _| !path_starts_with(path, &changed_path));
                for (path, item) in retained
                    .iter()
                    .filter(|(path, _)| path_starts_with(path, &changed_path))
                {
                PaintCommand::validate_sequence(&item.commands)?;
                for command in &item.commands {
                    if let PaintCommand::Image { image, .. } = command {
                        if !self.resources.contains_image(*image) {
                            return Err(RenderError::MissingImage(*image));
                        }
                    }
                }
                let mut batches = Vec::new();
                for clip in &item.clips {
                    batches.extend(self.build_render_batches(
                        &[PaintCommand::Clip {
                            shape: clip.shape.clone(),
                        }],
                        render_size,
                        scale_factor,
                        clip.transform,
                        1.0,
                    ));
                }
                batches.extend(self.build_render_batches(
                    &item.commands,
                    render_size,
                    scale_factor,
                    item.transform,
                    item.opacity,
                ));
                    retained_gpu_items.insert(path.clone(), RetainedGpuItem {
                    bounds: item.bounds,
                    batches: build_gpu_batches(&self.device, batches),
                    });
                }
            }
            if let Some(state) = self.surfaces.get_mut(&window) {
                state.retained_gpu_items = Some(retained_gpu_items);
            }
        }
        let damage_regions = coalesce_damage_for_spatial_index(
            damage_regions,
            self.surfaces
                .get(&window)
                .and_then(|state| state.spatial_index.as_ref()),
        );
        // Retained batches have already been validated when they were built.
        // Clean and partial frames need only carry their clear color here.
        let commands = [PaintCommand::Clear(clear)];
        let result = self.render_commands_with_damage_regions_key(
            window,
            &commands,
            &damage_regions,
            None,
            Some(&[]),
        );
        if result.is_ok() {
            if let Some(state) = self.surfaces.get_mut(&window) {
                state.retained_items = Some(retained);
            }
        }
        result
    }

    /// Resolves dirty WidgetIds through the retained RenderNode index before
    /// updating GPU batches. This is the normal WidgetTree integration point.
    pub fn render_node_with_dirty_widgets(
        &mut self,
        window: WindowId,
        node: &RenderNode,
        damage_regions: &[Rect],
        clear: Color,
        index: &RenderNodeIndex,
        dirty_widgets: &[u64],
    ) -> Result<(), RenderError> {
        let dirty_paths = dirty_widgets
            .iter()
            .filter_map(|id| index.path_for(*id))
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        self.render_node_with_damage_regions_indexed(
            window,
            node,
            damage_regions,
            clear,
            Some(&dirty_paths),
        )
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
