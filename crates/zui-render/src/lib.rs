//! GPU rendering owned by the application composition root.
//!
//! This crate deliberately exposes no `wgpu::Device` to widgets. Widgets produce
//! drawing data; `Renderer` owns the GPU and the per-window swap-chain surfaces.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Mutex, OnceLock};

use wgpu::util::DeviceExt;
use zui_core::{Color, Dip, PhysicalSize, Point, Rect, ScaleFactor, Size, WindowId};
use zui_platform::spi::RawWindowHandleProvider;

pub use wgpu;

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

#[derive(Debug)]
pub enum RenderError {
    AdapterUnavailable,
    Device(String),
    Surface(String),
    SurfaceNotAttached(WindowId),
    SurfaceLost,
    InvalidCommand(String),
    MissingImage(ImageId),
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AdapterUnavailable => write!(f, "no compatible GPU adapter available"),
            Self::Device(message) => write!(f, "GPU device error: {message}"),
            Self::Surface(message) => write!(f, "surface error: {message}"),
            Self::SurfaceNotAttached(id) => write!(f, "surface {id:?} is not attached"),
            Self::SurfaceLost => write!(f, "surface was lost or outdated"),
            Self::InvalidCommand(message) => write!(f, "invalid paint command: {message}"),
            Self::MissingImage(id) => write!(f, "image resource {id:?} is not registered"),
        }
    }
}

impl std::error::Error for RenderError {}

#[derive(Clone, Debug, PartialEq)]
pub enum ClipShape {
    Rect(Rect),
    RoundedRect { rect: Rect, radius: Dip },
    Path { path: IconPath },
}

impl ClipShape {
    pub fn bounds(&self) -> Rect {
        match self {
            Self::Rect(rect) | Self::RoundedRect { rect, .. } => *rect,
            Self::Path { path } => {
                let mut bounds = None;
                for segment in &path.segments {
                    for point in [segment.start, segment.end] {
                        bounds = Some(match bounds {
                            Some(bounds) => union_rect(
                                bounds,
                                Rect {
                                    origin: point,
                                    size: zui_core::Size::default(),
                                },
                            ),
                            None => Rect {
                                origin: point,
                                size: zui_core::Size::default(),
                            },
                        });
                    }
                }
                bounds.unwrap_or_default()
            }
        }
    }
}

fn transform_clip_shape(shape: &ClipShape, transform: Transform) -> ClipShape {
    match shape {
        ClipShape::Rect(rect) => ClipShape::Rect(transform.rect(*rect)),
        ClipShape::RoundedRect { rect, radius } => ClipShape::RoundedRect {
            rect: transform.rect(*rect),
            radius: *radius,
        },
        ClipShape::Path { path } => ClipShape::Path {
            path: IconPath::new(
                path.segments
                    .iter()
                    .map(|segment| LineSegment {
                        start: transform.point(segment.start),
                        end: transform.point(segment.end),
                    })
                    .collect::<Vec<_>>(),
            ),
        },
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum PaintCommand {
    Clear(Color),
    Rect {
        rect: Rect,
        color: Color,
    },
    RoundedRect {
        rect: Rect,
        radius: Dip,
        color: Color,
    },
    Line {
        start: Point,
        end: Point,
        width: Dip,
        color: Color,
    },
    Text {
        text: String,
        origin: Point,
        color: Color,
        scale: u32,
    },
    Icon {
        rect: Rect,
        path: IconPath,
        color: Color,
        stroke: Dip,
    },
    Image {
        rect: Rect,
        image: ImageId,
        opacity: f32,
    },
    Clip {
        shape: ClipShape,
    },
    Transform(Transform),
    Opacity(f32),
}

impl PaintCommand {
    pub fn bounds(&self) -> Option<Rect> {
        match self {
            Self::Clear(_) | Self::Transform(_) | Self::Opacity(_) => None,
            Self::Rect { rect, .. } | Self::RoundedRect { rect, .. } | Self::Image { rect, .. } => {
                Some(*rect)
            }
            Self::Clip { shape } => Some(shape.bounds()),
            Self::Line {
                start, end, width, ..
            } => Some(line_bounds(*start, *end, *width)),
            Self::Text {
                text,
                origin,
                scale,
                ..
            } => Some(Rect {
                origin: *origin,
                size: zui_core::Size {
                    width: measure_text(text, *scale),
                    height: Dip((*scale).max(1) as f32 * 7.0),
                },
            }),
            Self::Icon { rect, .. } => Some(*rect),
        }
    }

    pub fn validate(&self) -> Result<(), RenderError> {
        match self {
            Self::Clear(color) => validate_color(*color),
            Self::Rect { rect, color } => {
                validate_rect(*rect)?;
                validate_color(*color)
            }
            Self::RoundedRect {
                rect,
                radius,
                color,
            } => {
                validate_rect(*rect)?;
                if !radius.0.is_finite() || radius.0 < 0.0 {
                    return Err(RenderError::InvalidCommand(
                        "rounded radius must be finite and non-negative".into(),
                    ));
                }
                validate_color(*color)
            }
            Self::Line {
                start,
                end,
                width,
                color,
            } => {
                if !point_is_finite(*start)
                    || !point_is_finite(*end)
                    || !width.0.is_finite()
                    || width.0 <= 0.0
                {
                    return Err(RenderError::InvalidCommand(
                        "line geometry is invalid".into(),
                    ));
                }
                validate_color(*color)
            }
            Self::Text {
                origin,
                scale,
                color,
                ..
            } => {
                if !point_is_finite(*origin) || *scale == 0 {
                    return Err(RenderError::InvalidCommand(
                        "text geometry is invalid".into(),
                    ));
                }
                validate_color(*color)
            }
            Self::Icon {
                rect,
                path,
                stroke,
                color,
            } => {
                validate_rect(*rect)?;
                if !stroke.0.is_finite()
                    || stroke.0 <= 0.0
                    || path.segments.iter().any(|segment| {
                        !point_is_finite(segment.start) || !point_is_finite(segment.end)
                    })
                {
                    return Err(RenderError::InvalidCommand(
                        "icon geometry is invalid".into(),
                    ));
                }
                validate_color(*color)
            }
            Self::Image { rect, opacity, .. } => {
                validate_rect(*rect)?;
                if !opacity.is_finite() || !(0.0..=1.0).contains(opacity) {
                    return Err(RenderError::InvalidCommand(
                        "image opacity is outside 0..=1".into(),
                    ));
                }
                Ok(())
            }
            Self::Clip { shape } => {
                validate_rect(shape.bounds())?;
                if let ClipShape::RoundedRect { radius, .. } = shape {
                    if !radius.0.is_finite() || radius.0 < 0.0 {
                        return Err(RenderError::InvalidCommand(
                            "rounded clip radius must be finite and non-negative".into(),
                        ));
                    }
                }
                if let ClipShape::Path { path } = shape {
                    if path.segments.len() < 3
                        || path.segments.iter().any(|segment| {
                            !point_is_finite(segment.start) || !point_is_finite(segment.end)
                        })
                    {
                        return Err(RenderError::InvalidCommand(
                            "path clip must contain at least three finite segments".into(),
                        ));
                    }
                }
                Ok(())
            }
            Self::Transform(transform) => {
                if transform.matrix.iter().all(|value| value.is_finite()) {
                    Ok(())
                } else {
                    Err(RenderError::InvalidCommand(
                        "transform contains a non-finite value".into(),
                    ))
                }
            }
            Self::Opacity(value) => {
                if value.is_finite() && (0.0..=1.0).contains(value) {
                    Ok(())
                } else {
                    Err(RenderError::InvalidCommand(
                        "opacity is outside 0..=1".into(),
                    ))
                }
            }
        }
    }

    pub fn is_draw_command(&self) -> bool {
        !matches!(
            self,
            Self::Clear(_) | Self::Clip { .. } | Self::Transform(_) | Self::Opacity(_)
        )
    }
}

fn point_is_finite(point: Point) -> bool {
    point.x.0.is_finite() && point.y.0.is_finite()
}
fn validate_color(color: Color) -> Result<(), RenderError> {
    if [color.r, color.g, color.b, color.a]
        .iter()
        .all(|value| value.is_finite() && (0.0..=1.0).contains(value))
    {
        Ok(())
    } else {
        Err(RenderError::InvalidCommand("color is invalid".into()))
    }
}
fn validate_rect(rect: Rect) -> Result<(), RenderError> {
    if point_is_finite(rect.origin)
        && rect.size.width.0.is_finite()
        && rect.size.height.0.is_finite()
        && rect.size.width.0 >= 0.0
        && rect.size.height.0 >= 0.0
    {
        Ok(())
    } else {
        Err(RenderError::InvalidCommand(
            "rect geometry is invalid".into(),
        ))
    }
}
fn line_bounds(start: Point, end: Point, width: Dip) -> Rect {
    let half = width.0 / 2.0;
    Rect {
        origin: Point {
            x: Dip(start.x.0.min(end.x.0) - half),
            y: Dip(start.y.0.min(end.y.0) - half),
        },
        size: zui_core::Size {
            width: Dip((start.x.0.max(end.x.0) - start.x.0.min(end.x.0)) + width.0),
            height: Dip((start.y.0.max(end.y.0) - start.y.0.min(end.y.0)) + width.0),
        },
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineSegment {
    pub start: Point,
    pub end: Point,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct IconPath {
    pub segments: Vec<LineSegment>,
}

impl IconPath {
    pub fn new(segments: impl Into<Vec<LineSegment>>) -> Self {
        Self {
            segments: segments.into(),
        }
    }
}

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
    image_bind_groups: HashMap<(WindowId, ImageId), wgpu::BindGroup>,
    last_used: HashMap<ImageId, u64>,
    clock: u64,
    max_gpu_images: usize,
    image_sampler: wgpu::Sampler,
    fonts: &'static [fontdue::Font],
    glyph_cache: HashMap<(usize, char, u32, u32), CachedGlyph>,
    font_cache: HashMap<char, Option<usize>>,
}

impl ResourceManager {
    fn new(device: &wgpu::Device, fonts: &'static [fontdue::Font]) -> Self {
        Self {
            cpu: ResourceCache::default(),
            gpu_images: HashMap::new(),
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
            fonts,
            glyph_cache: HashMap::new(),
            font_cache: HashMap::new(),
        }
    }

    pub fn set_gpu_image_capacity(&mut self, capacity: usize) {
        self.max_gpu_images = capacity.max(1);
        self.evict_gpu_images();
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
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("zui-render image"),
            size: wgpu::Extent3d {
                width: image.width,
                height: image.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
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
        let view = texture_view(&texture);
        self.gpu_images.insert(
            id,
            GpuImage {
                _texture: texture,
                view,
            },
        );
        self.image_bind_groups
            .retain(|(_, image_id), _| *image_id != id);
        self.touch(id);
        self.evict_gpu_images();
    }

    fn touch(&mut self, id: ImageId) {
        self.clock = self.clock.wrapping_add(1);
        self.last_used.insert(id, self.clock);
    }

    fn evict_gpu_images(&mut self) {
        while self.gpu_images.len() > self.max_gpu_images {
            let Some(oldest) = self
                .last_used
                .iter()
                .min_by_key(|(_, stamp)| *stamp)
                .map(|(id, _)| *id)
            else {
                break;
            };
            self.gpu_images.remove(&oldest);
            self.last_used.remove(&oldest);
            self.image_bind_groups.retain(|(_, id), _| *id != oldest);
        }
    }

    fn remove_image(&mut self, id: ImageId) -> Option<ImageResource> {
        self.gpu_images.remove(&id);
        self.last_used.remove(&id);
        self.image_bind_groups
            .retain(|(_, image_id), _| *image_id != id);
        self.cpu.remove_image(id)
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
                        resource: wgpu::BindingResource::TextureView(&image.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.image_sampler),
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
struct RenderSegment {
    key: u64,
    bounds: Rect,
    transform: Transform,
    clip: Option<Rect>,
    opacity: f32,
    clips: Vec<ClipShape>,
    commands: Vec<PaintCommand>,
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

    fn flatten_segments(&self) -> Vec<RenderSegment> {
        let mut segments = Vec::new();
        self.flatten_segments_with_state(&mut segments, Transform::IDENTITY, None, 1.0, &[]);
        segments
    }

    fn flatten_segments_with_state(
        &self,
        segments: &mut Vec<RenderSegment>,
        parent_transform: Transform,
        parent_clip: Option<Rect>,
        parent_opacity: f32,
        parent_clips: &[ClipShape],
    ) {
        let transform = compose_transform(parent_transform, self.transform);
        let node_clip = self
            .clip
            .as_ref()
            .map(|clip| transform_clip_shape(clip, transform));
        let clip = intersect_clip(parent_clip, node_clip.as_ref().map(ClipShape::bounds));
        let opacity = parent_opacity * self.opacity;
        let mut clips = parent_clips.to_vec();
        if let Some(node_clip) = node_clip {
            clips.push(node_clip);
        }
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.local_gpu_cache_key().hash(&mut hasher);
        format!("{:?}{:?}{:?}{:?}", transform, clip, opacity, clips).hash(&mut hasher);
        segments.push(RenderSegment {
            key: hasher.finish(),
            bounds: clip
                .map(|clip| intersect_rect(transform.rect(self.local_bounds), clip))
                .unwrap_or_else(|| transform.rect(self.local_bounds)),
            transform,
            clip,
            opacity,
            clips: clips.clone(),
            commands: self.commands.clone(),
        });
        for child in &self.children {
            child.flatten_segments_with_state(segments, transform, clip, opacity, &clips);
        }
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

fn clip_rect(rect: Rect, clip: Option<Rect>) -> Option<Rect> {
    let Some(clip) = clip else {
        return Some(rect);
    };
    let left = rect.origin.x.0.max(clip.origin.x.0);
    let top = rect.origin.y.0.max(clip.origin.y.0);
    let right = (rect.origin.x.0 + rect.size.width.0).min(clip.origin.x.0 + clip.size.width.0);
    let bottom = (rect.origin.y.0 + rect.size.height.0).min(clip.origin.y.0 + clip.size.height.0);
    if right <= left || bottom <= top {
        None
    } else {
        Some(Rect {
            origin: Point {
                x: Dip(left),
                y: Dip(top),
            },
            size: zui_core::Size {
                width: Dip(right - left),
                height: Dip(bottom - top),
            },
        })
    }
}

fn clip_line(start: Point, end: Point, clip: Option<Rect>) -> Option<LineSegment> {
    let Some(clip) = clip else {
        return Some(LineSegment { start, end });
    };
    let x_min = clip.origin.x.0;
    let x_max = x_min + clip.size.width.0;
    let y_min = clip.origin.y.0;
    let y_max = y_min + clip.size.height.0;
    let dx = end.x.0 - start.x.0;
    let dy = end.y.0 - start.y.0;
    let mut low: f32 = 0.0;
    let mut high: f32 = 1.0;
    for (p, q) in [
        (-dx, start.x.0 - x_min),
        (dx, x_max - start.x.0),
        (-dy, start.y.0 - y_min),
        (dy, y_max - start.y.0),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
        } else {
            let value = q / p;
            if p < 0.0 {
                low = low.max(value);
            } else {
                high = high.min(value);
            }
            if low > high {
                return None;
            }
        }
    }
    Some(LineSegment {
        start: Point {
            x: Dip(start.x.0 + low * dx),
            y: Dip(start.y.0 + low * dy),
        },
        end: Point {
            x: Dip(start.x.0 + high * dx),
            y: Dip(start.y.0 + high * dy),
        },
    })
}

fn transform_command(
    command: &PaintCommand,
    transform: Transform,
    clip: Option<Rect>,
    opacity: f32,
) -> Option<PaintCommand> {
    let apply_opacity = |mut color: Color| {
        color.a *= opacity;
        color
    };
    let visible = |rect: Rect| clip.map(|clip| rect_intersects(rect, clip)).unwrap_or(true);
    match command {
        PaintCommand::Clear(color) => Some(PaintCommand::Clear(apply_opacity(*color))),
        PaintCommand::Rect { rect, color } => {
            let rect = transform.rect(*rect);
            clip_rect(rect, clip)
                .filter(|_| visible(rect))
                .map(|rect| PaintCommand::Rect {
                    rect,
                    color: apply_opacity(*color),
                })
        }
        PaintCommand::RoundedRect {
            rect,
            radius,
            color,
        } => {
            let rect = transform.rect(*rect);
            clip_rect(rect, clip)
                .filter(|_| visible(rect))
                .map(|rect| PaintCommand::RoundedRect {
                    rect,
                    radius: *radius,
                    color: apply_opacity(*color),
                })
        }
        PaintCommand::Line {
            start,
            end,
            width,
            color,
        } => {
            let start = transform.point(*start);
            let end = transform.point(*end);
            let bounds = line_bounds(start, end, *width);
            clip_line(start, end, clip)
                .filter(|_| visible(bounds))
                .map(|segment| PaintCommand::Line {
                    start: segment.start,
                    end: segment.end,
                    width: *width,
                    color: apply_opacity(*color),
                })
        }
        PaintCommand::Text {
            text,
            origin,
            color,
            scale,
        } => {
            let origin = transform.point(*origin);
            let rect = Rect {
                origin,
                size: zui_core::Size {
                    width: Dip(1.0),
                    height: Dip(1.0),
                },
            };
            visible(rect).then_some(PaintCommand::Text {
                text: text.clone(),
                origin,
                color: apply_opacity(*color),
                scale: *scale,
            })
        }
        PaintCommand::Icon {
            rect,
            path,
            color,
            stroke,
        } => {
            let rect = transform.rect(*rect);
            visible(rect).then_some(PaintCommand::Icon {
                rect,
                path: IconPath::new(
                    path.segments
                        .iter()
                        .filter_map(|segment| {
                            clip_line(
                                transform.point(segment.start),
                                transform.point(segment.end),
                                clip,
                            )
                        })
                        .collect::<Vec<_>>(),
                ),
                color: apply_opacity(*color),
                stroke: *stroke,
            })
        }
        PaintCommand::Image {
            rect,
            image,
            opacity: image_opacity,
        } => {
            let rect = transform.rect(*rect);
            visible(rect).then_some(PaintCommand::Image {
                rect,
                image: *image,
                opacity: image_opacity * opacity,
            })
        }
        PaintCommand::Clip { shape } => Some(PaintCommand::Clip {
            shape: transform_clip_shape(shape, transform),
        }),
        PaintCommand::Transform(transform) => Some(PaintCommand::Transform(*transform)),
        PaintCommand::Opacity(value) => Some(PaintCommand::Opacity(value * opacity)),
    }
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

fn coalesce_damage_for_segments(regions: &[Rect], segments: &[RenderSegment]) -> Vec<Rect> {
    let mut result = regions.to_vec();
    let mut changed = true;
    while changed {
        changed = false;
        'outer: for left in 0..result.len() {
            for right in (left + 1)..result.len() {
                if segments.iter().any(|segment| {
                    rect_intersects(segment.bounds, result[left])
                        && rect_intersects(segment.bounds, result[right])
                }) {
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
    canvas: wgpu::Texture,
    canvas_view: wgpu::TextureView,
    stencil: wgpu::Texture,
    stencil_view: wgpu::TextureView,
    stencil_reset: wgpu::Buffer,
    blit_pipeline: wgpu::RenderPipeline,
    blit_bind_group: wgpu::BindGroup,
    scale_factor: ScaleFactor,
    has_contents: bool,
    /// GPU vertex buffers keyed by the retained render-node/display-list
    /// fingerprint. Unchanged nodes reuse their buffers across frames.
    node_gpu_cache: HashMap<u64, GpuBatchCacheEntry>,
    cache_clock: u64,
    max_gpu_cache_entries: usize,
}

struct GpuBatchCacheEntry {
    batches: Vec<GpuBatch>,
    last_used: u64,
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
}

enum RenderBatch {
    Rect(Vec<RectVertex>),
    Rounded(Vec<RoundedRectVertex>),
    Line(Vec<LineVertex>),
    Image {
        image: ImageId,
        vertices: Vec<ImageVertex>,
    },
    Clip(ClipGeometry),
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
    Draw(BatchKind, wgpu::Buffer, u32, Vec<u8>),
    Clip(ClipGeometry, Option<(wgpu::Buffer, u32)>),
    Scissor(Rect),
}

fn rect_batch(batches: &mut Vec<RenderBatch>) -> &mut Vec<RectVertex> {
    if !matches!(batches.last(), Some(RenderBatch::Rect(_))) {
        batches.push(RenderBatch::Rect(Vec::new()));
    }
    match batches.last_mut().expect("rect batch was just added") {
        RenderBatch::Rect(vertices) => vertices,
        _ => unreachable!(),
    }
}

fn rounded_batch(batches: &mut Vec<RenderBatch>) -> &mut Vec<RoundedRectVertex> {
    if !matches!(batches.last(), Some(RenderBatch::Rounded(_))) {
        batches.push(RenderBatch::Rounded(Vec::new()));
    }
    match batches.last_mut().expect("rounded batch was just added") {
        RenderBatch::Rounded(vertices) => vertices,
        _ => unreachable!(),
    }
}

fn line_batch(batches: &mut Vec<RenderBatch>) -> &mut Vec<LineVertex> {
    if !matches!(batches.last(), Some(RenderBatch::Line(_))) {
        batches.push(RenderBatch::Line(Vec::new()));
    }
    match batches.last_mut().expect("line batch was just added") {
        RenderBatch::Line(vertices) => vertices,
        _ => unreachable!(),
    }
}

fn image_batch(batches: &mut Vec<RenderBatch>, image: ImageId) -> &mut Vec<ImageVertex> {
    if !matches!(batches.last(), Some(RenderBatch::Image { image: current, .. }) if *current == image)
    {
        batches.push(RenderBatch::Image {
            image,
            vertices: Vec::new(),
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
    // Kept alive for the view and bind groups.
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
}

struct CachedGlyph {
    metrics: fontdue::Metrics,
    pixels: Vec<CachedGlyphPixel>,
}

struct CachedGlyphPixel {
    x: u16,
    y: u16,
    alpha: f32,
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
        let config = surface
            .get_default_config(&self.adapter, size.width.max(1), size.height.max(1))
            .ok_or_else(|| RenderError::Surface("adapter cannot present to this surface".into()))?;
        surface.configure(&self.device, &config);
        let pipeline = create_rect_pipeline(&self.device, config.format);
        let rounded_pipeline = create_rounded_rect_pipeline(&self.device, config.format);
        let stencil_rounded_pipeline = create_stencil_rounded_pipeline(&self.device, config.format);
        let line_pipeline = create_line_pipeline(&self.device, config.format);
        let image_pipeline = create_image_pipeline(&self.device, config.format);
        let stencil_pipeline = create_stencil_pipeline(&self.device, config.format);
        let stencil_mask_pipeline = create_stencil_mask_pipeline(&self.device, config.format);
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
                canvas,
                canvas_view,
                stencil,
                stencil_view,
                stencil_reset,
                blit_pipeline,
                blit_bind_group,
                scale_factor,
                has_contents: false,
                node_gpu_cache: HashMap::new(),
                cache_clock: 0,
                max_gpu_cache_entries: 256,
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
        let config = surface
            .get_default_config(&self.adapter, size.width.max(1), size.height.max(1))
            .ok_or_else(|| RenderError::Surface("adapter cannot present to this surface".into()))?;
        surface.configure(&self.device, &config);
        let pipeline = create_rect_pipeline(&self.device, config.format);
        let rounded_pipeline = create_rounded_rect_pipeline(&self.device, config.format);
        let stencil_rounded_pipeline = create_stencil_rounded_pipeline(&self.device, config.format);
        let line_pipeline = create_line_pipeline(&self.device, config.format);
        let image_pipeline = create_image_pipeline(&self.device, config.format);
        let stencil_pipeline = create_stencil_pipeline(&self.device, config.format);
        let stencil_mask_pipeline = create_stencil_mask_pipeline(&self.device, config.format);
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
                canvas,
                canvas_view,
                stencil,
                stencil_view,
                stencil_reset,
                blit_pipeline,
                blit_bind_group,
                scale_factor,
                has_contents: false,
                node_gpu_cache: HashMap::new(),
                cache_clock: 0,
                max_gpu_cache_entries: 256,
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
        state.cache_clock = 0;
        Ok(())
    }

    pub fn detach_surface(&mut self, window: WindowId) {
        self.surfaces.remove(&window);
    }

    fn build_render_batches(
        fonts: &'static [fontdue::Font],
        glyph_cache: &mut HashMap<(usize, char, u32, u32), CachedGlyph>,
        font_cache: &mut HashMap<char, Option<usize>>,
        commands: &[PaintCommand],
        render_size: PhysicalSize,
        scale_factor: f32,
    ) -> Vec<RenderBatch> {
        let mut batches = Vec::new();
        for command in commands {
            match command {
                PaintCommand::Clip { shape } => {
                    batches.push(RenderBatch::Clip(match shape {
                        ClipShape::Rect(rect) => ClipGeometry::Rect(*rect),
                        ClipShape::RoundedRect { rect, radius } => ClipGeometry::Rounded {
                            rect: *rect,
                            radius: *radius,
                        },
                        ClipShape::Path { path } => ClipGeometry::Path {
                            bounds: path_bounds(path),
                            vertices: path_mask_vertices(path, render_size),
                        },
                    }));
                    continue;
                }
                _ => {}
            }
            match command {
                PaintCommand::Rect { rect, color } => {
                    append_rect(rect_batch(&mut batches), *rect, *color, render_size);
                }
                PaintCommand::RoundedRect {
                    rect,
                    radius,
                    color,
                } => append_rounded_rect(
                    rounded_batch(&mut batches),
                    *rect,
                    *radius,
                    *color,
                    render_size,
                ),
                PaintCommand::Line {
                    start,
                    end,
                    width,
                    color,
                } => append_line(
                    line_batch(&mut batches),
                    *start,
                    *end,
                    *width,
                    *color,
                    render_size,
                ),
                PaintCommand::Text {
                    text,
                    origin,
                    color,
                    scale,
                } => append_text(
                    rect_batch(&mut batches),
                    fonts,
                    glyph_cache,
                    font_cache,
                    text,
                    *origin,
                    *color,
                    *scale,
                    render_size,
                    scale_factor,
                ),
                PaintCommand::Icon {
                    path,
                    color,
                    stroke,
                    ..
                } => {
                    for segment in &path.segments {
                        append_line(
                            line_batch(&mut batches),
                            segment.start,
                            segment.end,
                            *stroke,
                            *color,
                            render_size,
                        );
                    }
                }
                PaintCommand::Image {
                    rect,
                    image,
                    opacity,
                } => append_image(
                    image_batch(&mut batches, *image),
                    *rect,
                    *opacity,
                    render_size,
                ),
                PaintCommand::Transform(_) | PaintCommand::Opacity(_) | PaintCommand::Clear(_) => {}
                PaintCommand::Clip { .. } => unreachable!(),
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
        segments: Option<&[RenderSegment]>,
    ) -> Result<(), RenderError> {
        commands.iter().try_for_each(PaintCommand::validate)?;
        for command in commands {
            if let PaintCommand::Image { image, .. } = command {
                if !self.resources.contains_image(*image) {
                    return Err(RenderError::MissingImage(*image));
                }
            }
        }
        let state = self
            .surfaces
            .get_mut(&window)
            .ok_or(RenderError::SurfaceNotAttached(window))?;
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
        let resolved_commands = resolve_commands(commands);
        let batch_hash = cache_key.unwrap_or_else(|| command_list_hash(&resolved_commands));
        let clear = resolved_commands
            .iter()
            .rev()
            .find_map(|command| match command {
                PaintCommand::Clear(color) => Some(wgpu::Color {
                    r: color.r as f64,
                    g: color.g as f64,
                    b: color.b as f64,
                    a: color.a as f64,
                }),
                PaintCommand::Rect { .. } => None,
                PaintCommand::RoundedRect { .. } => None,
                PaintCommand::Line { .. } => None,
                PaintCommand::Text { .. } => None,
                PaintCommand::Icon { .. }
                | PaintCommand::Image { .. }
                | PaintCommand::Clip { .. }
                | PaintCommand::Transform(_)
                | PaintCommand::Opacity(_) => None,
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
            let scale = state.scale_factor.0.max(1.0);
            let render_size = PhysicalSize {
                width: (state.size.width as f64 / scale).round().max(1.0) as u32,
                height: (state.size.height as f64 / scale).round().max(1.0) as u32,
            };
            let batches = Self::build_render_batches(
                self.resources.fonts,
                &mut self.resources.glyph_cache,
                &mut self.resources.font_cache,
                &resolved_commands,
                render_size,
                state.scale_factor.0 as f32,
            );
            let node_batches = segments.map(|segments| {
                segments
                    .iter()
                    .map(|segment| {
                        let resolved = resolve_segment_commands(segment);
                        let gpu_batches = if let Some(cached) = take_gpu_cache(state, segment.key) {
                            cached
                        } else {
                            let batches = Self::build_render_batches(
                                self.resources.fonts,
                                &mut self.resources.glyph_cache,
                                &mut self.resources.font_cache,
                                &resolved,
                                render_size,
                                state.scale_factor.0 as f32,
                            );
                            build_gpu_batches(&self.device, batches, render_size)
                        };
                        (segment.key, segment.bounds, gpu_batches)
                    })
                    .collect::<Vec<_>>()
            });
            let gpu_batches = if let Some(node_batches) = &node_batches {
                let mut all = Vec::new();
                for (_, _, batches) in node_batches {
                    let has_clip = batches
                        .iter()
                        .any(|batch| matches!(batch, GpuBatch::Clip(_, _)));
                    if has_clip {
                        all.push(GpuBatch::Clip(ClipGeometry::Reset, None));
                    }
                    all.extend(batches.iter().cloned());
                    if has_clip {
                        all.push(GpuBatch::Clip(ClipGeometry::Reset, None));
                    }
                }
                coalesce_gpu_batches(&self.device, all)
            } else {
                take_gpu_cache(state, batch_hash)
                    .unwrap_or_else(|| build_gpu_batches(&self.device, batches, render_size))
            };
            let replay_batches = if state.has_contents && !damage_regions.is_empty() {
                if let Some(node_batches) = &node_batches {
                    let mut replay = Vec::new();
                    for region in damage_regions {
                        replay.push(GpuBatch::Scissor(*region));
                        for (_, bounds, batches) in node_batches {
                            if rect_intersects(*bounds, *region) {
                                let has_clip = batches
                                    .iter()
                                    .any(|batch| matches!(batch, GpuBatch::Clip(_, _)));
                                if has_clip {
                                    replay.push(GpuBatch::Clip(ClipGeometry::Reset, None));
                                }
                                replay.extend(batches.iter().cloned());
                                if has_clip {
                                    replay.push(GpuBatch::Clip(ClipGeometry::Reset, None));
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
                if let GpuBatch::Clip(geometry, mask) = batch {
                    if let Some((buffer, count)) = mask {
                        pass.set_pipeline(match geometry {
                            ClipGeometry::Rounded { .. } => &state.stencil_rounded_pipeline,
                            _ => &state.stencil_mask_pipeline,
                        });
                        pass.set_stencil_reference(active_clip_depth);
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
                            Some(*rect)
                        }
                        ClipGeometry::Path { bounds, .. } => Some(*bounds),
                        ClipGeometry::Reset => None,
                    };
                    continue;
                }
                let (kind, draws): (&BatchKind, Vec<(&wgpu::Buffer, u32)>) = match batch {
                    GpuBatch::Draw(kind, buffer, count, _) => (kind, vec![(buffer, *count)]),
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
                if let BatchKind::Image(image) = *kind {
                    let layout = state.image_pipeline.get_bind_group_layout(0);
                    if let Some(bind_group) =
                        self.resources
                            .bind_group(&self.device, &self.queue, window, image, &layout)
                    {
                        pass.set_bind_group(0, &bind_group, &[]);
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
            if let Some(node_batches) = node_batches {
                for (key, _, batches) in node_batches {
                    insert_gpu_cache(state, key, batches);
                }
            } else {
                insert_gpu_cache(state, batch_hash, gpu_batches);
            }
        }
        {
            // The swap-chain texture is not a persistent render target. In
            // particular, Metal does not guarantee that LoadOp::Load contains
            // the previous frame's pixels. A partial composite would
            // therefore discard controls outside the current damage region.
            // The retained canvas is still updated locally above; presenting
            // it is a cheap full-screen blit and must always cover the whole
            // swap-chain image.
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
        state.has_contents = true;
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
        let segments = node.flatten_segments();
        let damage_regions = coalesce_damage_for_segments(damage_regions, &segments);
        let mut commands = vec![PaintCommand::Clear(clear)];
        for segment in &segments {
            commands.extend(segment.commands.iter().cloned());
        }
        self.render_commands_with_damage_regions_key(
            window,
            &commands,
            &damage_regions,
            None,
            Some(&segments),
        )
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }
}

fn build_gpu_batches(
    device: &wgpu::Device,
    batches: Vec<RenderBatch>,
    render_size: PhysicalSize,
) -> Vec<GpuBatch> {
    coalesce_gpu_batches(
        device,
        batches
            .into_iter()
            .filter_map(|batch| match batch {
                RenderBatch::Rect(vertices) if !vertices.is_empty() => Some(GpuBatch::Draw(
                    BatchKind::Rect,
                    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("zui-render rectangles"),
                        contents: bytemuck::cast_slice(&vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    }),
                    vertices.len() as u32,
                    bytemuck::cast_slice(&vertices).to_vec(),
                )),
                RenderBatch::Rounded(vertices) if !vertices.is_empty() => Some(GpuBatch::Draw(
                    BatchKind::Rounded,
                    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("zui-render rounded rectangles"),
                        contents: bytemuck::cast_slice(&vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    }),
                    vertices.len() as u32,
                    bytemuck::cast_slice(&vertices).to_vec(),
                )),
                RenderBatch::Line(vertices) if !vertices.is_empty() => Some(GpuBatch::Draw(
                    BatchKind::Line,
                    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("zui-render lines"),
                        contents: bytemuck::cast_slice(&vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    }),
                    vertices.len() as u32,
                    bytemuck::cast_slice(&vertices).to_vec(),
                )),
                RenderBatch::Image { image, vertices } if !vertices.is_empty() => {
                    Some(GpuBatch::Draw(
                        BatchKind::Image(image),
                        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("zui-render images"),
                            contents: bytemuck::cast_slice(&vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        }),
                        vertices.len() as u32,
                        bytemuck::cast_slice(&vertices).to_vec(),
                    ))
                }
                RenderBatch::Clip(geometry) => {
                    let mask = match geometry {
                        ClipGeometry::Reset => None,
                        ClipGeometry::Rect(rect) => {
                            let mut vertices = Vec::new();
                            append_rect(&mut vertices, rect, Color::WHITE, render_size);
                            let buffer =
                                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                    label: Some("zui-render stencil clip"),
                                    contents: bytemuck::cast_slice(&vertices),
                                    usage: wgpu::BufferUsages::VERTEX,
                                });
                            Some((buffer, vertices.len() as u32))
                        }
                        ClipGeometry::Rounded { rect, radius } => {
                            let mut vertices = Vec::new();
                            append_rounded_rect(
                                &mut vertices,
                                rect,
                                radius,
                                Color::WHITE,
                                render_size,
                            );
                            let buffer =
                                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                    label: Some("zui-render rounded stencil clip"),
                                    contents: bytemuck::cast_slice(&vertices),
                                    usage: wgpu::BufferUsages::VERTEX,
                                });
                            Some((buffer, vertices.len() as u32))
                        }
                        ClipGeometry::Path { ref vertices, .. } => {
                            let buffer =
                                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                    label: Some("zui-render path stencil clip"),
                                    contents: bytemuck::cast_slice(&vertices),
                                    usage: wgpu::BufferUsages::VERTEX,
                                });
                            Some((buffer, vertices.len() as u32))
                        }
                    };
                    Some(GpuBatch::Clip(geometry, mask))
                }
                _ => None,
            })
            .collect(),
    )
}

fn coalesce_gpu_batches(device: &wgpu::Device, batches: Vec<GpuBatch>) -> Vec<GpuBatch> {
    let mut result = Vec::with_capacity(batches.len());
    for batch in batches {
        match batch {
            GpuBatch::Draw(kind, buffer, count, bytes) => {
                let mut merged = false;
                if let Some(GpuBatch::Draw(
                    previous_kind,
                    previous_buffer,
                    previous_count,
                    previous_bytes,
                )) = result.last_mut()
                {
                    if *previous_kind == kind {
                        previous_bytes.extend_from_slice(&bytes);
                        *previous_count += count;
                        *previous_buffer =
                            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                label: Some("zui-render merged vertex batch"),
                                contents: previous_bytes,
                                usage: wgpu::BufferUsages::VERTEX,
                            });
                        merged = true;
                    }
                }
                if !merged {
                    result.push(GpuBatch::Draw(kind, buffer, count, bytes));
                }
            }
            other => result.push(other),
        }
    }
    result
}

fn cached_system_fonts() -> &'static [fontdue::Font] {
    SYSTEM_FONTS.get_or_init(load_system_fonts).as_slice()
}

fn load_system_fonts() -> Vec<fontdue::Font> {
    let candidates = [
        std::env::var("ZUI_FONT_PATH").ok(),
        std::env::var("ZUI_LATIN_FONT_PATH").ok(),
        Some("/System/Library/Fonts/SFNS.ttf".into()),
        Some("/System/Library/Fonts/SFNSRounded.ttf".into()),
        Some("/System/Library/Fonts/Hiragino Sans GB.ttc".into()),
        Some("/System/Library/Fonts/Supplemental/Verdana.ttf".into()),
        Some("/System/Library/Fonts/Supplemental/Tahoma.ttf".into()),
        Some("/System/Library/Fonts/Supplemental/Arial.ttf".into()),
        Some("/System/Library/Fonts/Supplemental/Arial Unicode.ttf".into()),
        Some("/System/Library/Fonts/Supplemental/NISC18030.ttf".into()),
        Some("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf".into()),
        std::env::var("ZUI_CJK_FONT_PATH").ok(),
        Some("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc".into()),
        Some("/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc".into()),
        Some("C:\\Windows\\Fonts\\segoeui.ttf".into()),
        Some("C:\\Windows\\Fonts\\msyh.ttc".into()),
    ];
    candidates
        .into_iter()
        .flatten()
        .filter_map(|path| std::fs::read(path).ok())
        .filter_map(|bytes| fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default()).ok())
        .collect()
}

fn append_text(
    vertices: &mut Vec<RectVertex>,
    fonts: &[fontdue::Font],
    glyph_cache: &mut HashMap<(usize, char, u32, u32), CachedGlyph>,
    font_cache: &mut HashMap<char, Option<usize>>,
    text: &str,
    origin: Point,
    color: Color,
    scale: u32,
    size: PhysicalSize,
    scale_factor: f32,
) {
    if !fonts.is_empty() {
        let scale_factor = scale_factor.max(1.0);
        let logical_font_size = (scale.max(1) * 7) as f32;
        let font_size = logical_font_size * scale_factor;
        let baseline = (origin.y.0 + logical_font_size * 0.8) * scale_factor;
        let mut x = origin.x.0;
        for character in text.chars() {
            let font_id = *font_cache.entry(character).or_insert_with(|| {
                fonts
                    .iter()
                    .enumerate()
                    .find(|(_, font)| font.lookup_glyph_index(character) != 0)
                    .map(|(font_id, _)| font_id)
            });
            let Some(font_id) = font_id else {
                x += font_size;
                continue;
            };
            let font = &fonts[font_id];
            let glyph = glyph_cache
                .entry((
                    font_id,
                    character,
                    scale.max(1),
                    (scale_factor * 100.0).round() as u32,
                ))
                .or_insert_with(|| {
                    let (metrics, bitmap) = font.rasterize(character, font_size);
                    let mut pixels = Vec::new();
                    for row in 0..metrics.height {
                        for column in 0..metrics.width {
                            let alpha = bitmap[row * metrics.width + column] as f32 / 255.0;
                            if alpha > 0.01 {
                                pixels.push(CachedGlyphPixel {
                                    x: column as u16,
                                    y: row as u16,
                                    alpha,
                                });
                            }
                        }
                    }
                    CachedGlyph { metrics, pixels }
                });
            let metrics = glyph.metrics;
            // Fontdue reports glyph bounds relative to the baseline. Keeping
            // one baseline for the complete run prevents punctuation and
            // lowercase glyphs from drifting vertically.
            let top = baseline - metrics.height as f32 - metrics.ymin as f32;
            for pixel in &glyph.pixels {
                append_rect(
                    vertices,
                    Rect {
                        origin: Point {
                            x: Dip((x * scale_factor + pixel.x as f32) / scale_factor),
                            y: Dip((top + pixel.y as f32) / scale_factor),
                        },
                        size: zui_core::Size {
                            width: Dip(1.0 / scale_factor),
                            height: Dip(1.0 / scale_factor),
                        },
                    },
                    Color {
                        a: color.a * pixel.alpha,
                        ..color
                    },
                    size,
                );
            }
            x += metrics.advance_width / scale_factor;
        }
        return;
    }

    let scale = scale.max(1) as f32;
    let mut x = origin.x.0;
    for character in text.chars() {
        for (row, bits) in glyph_rows(character).iter().enumerate() {
            for column in 0..5 {
                if bits & (1 << (4 - column)) != 0 {
                    append_rect(
                        vertices,
                        Rect {
                            origin: Point {
                                x: Dip(x + column as f32 * scale as f32),
                                y: Dip(origin.y.0 + row as f32 * scale as f32),
                            },
                            size: zui_core::Size {
                                width: Dip(scale as f32),
                                height: Dip(scale as f32),
                            },
                        },
                        color,
                        size,
                    );
                }
            }
        }
        x += 6.0 * scale;
    }
}

fn resolve_segment_commands(segment: &RenderSegment) -> Vec<PaintCommand> {
    let mut resolved = segment
        .clips
        .iter()
        .cloned()
        .map(|shape| PaintCommand::Clip { shape })
        .collect::<Vec<_>>();
    resolved.extend(
        resolve_commands(&segment.commands)
            .into_iter()
            .filter_map(|command| {
                transform_command(&command, segment.transform, segment.clip, segment.opacity)
            }),
    );
    resolved
}

fn resolve_commands(commands: &[PaintCommand]) -> Vec<PaintCommand> {
    let mut resolved = Vec::with_capacity(commands.len());
    let mut transform = Transform::IDENTITY;
    let mut clip = None;
    let mut opacity = 1.0;
    for command in commands {
        match command {
            PaintCommand::Transform(next) => transform = compose_transform(transform, *next),
            PaintCommand::Clip { shape } => {
                let shape = transform_clip_shape(shape, transform);
                clip = intersect_clip(clip, Some(shape.bounds()));
                if clip.is_some() {
                    resolved.push(PaintCommand::Clip { shape });
                }
            }
            PaintCommand::Opacity(value) => opacity *= value.clamp(0.0, 1.0),
            command => {
                if let Some(command) = transform_command(command, transform, clip, opacity) {
                    resolved.push(command);
                }
            }
        }
    }
    resolved
}

fn append_rect(vertices: &mut Vec<RectVertex>, rect: Rect, color: Color, size: PhysicalSize) {
    let left = rect.origin.x.0 / size.width.max(1) as f32 * 2.0 - 1.0;
    let right = (rect.origin.x.0 + rect.size.width.0) / size.width.max(1) as f32 * 2.0 - 1.0;
    let top = 1.0 - rect.origin.y.0 / size.height.max(1) as f32 * 2.0;
    let bottom = 1.0 - (rect.origin.y.0 + rect.size.height.0) / size.height.max(1) as f32 * 2.0;
    let color = [color.r, color.g, color.b, color.a];
    vertices.extend([
        RectVertex {
            position: [left, top],
            color,
        },
        RectVertex {
            position: [right, top],
            color,
        },
        RectVertex {
            position: [right, bottom],
            color,
        },
        RectVertex {
            position: [left, top],
            color,
        },
        RectVertex {
            position: [right, bottom],
            color,
        },
        RectVertex {
            position: [left, bottom],
            color,
        },
    ]);
}

fn path_bounds(path: &IconPath) -> Rect {
    ClipShape::Path { path: path.clone() }.bounds()
}

fn path_mask_vertices(path: &IconPath, size: PhysicalSize) -> Vec<RectVertex> {
    let points = path
        .segments
        .iter()
        .map(|segment| segment.start)
        .collect::<Vec<_>>();
    if points.len() < 3 {
        return Vec::new();
    }
    let to_position = |point: Point| {
        [
            point.x.0 / size.width.max(1) as f32 * 2.0 - 1.0,
            1.0 - point.y.0 / size.height.max(1) as f32 * 2.0,
        ]
    };
    let mut vertices = Vec::with_capacity((points.len() - 2) * 3);
    for index in 1..points.len() - 1 {
        for point in [points[0], points[index], points[index + 1]] {
            vertices.push(RectVertex {
                position: to_position(point),
                color: [1.0; 4],
            });
        }
    }
    vertices
}

fn append_line(
    vertices: &mut Vec<LineVertex>,
    start: Point,
    end: Point,
    width: Dip,
    color: Color,
    size: PhysicalSize,
) {
    let dx = end.x.0 - start.x.0;
    let dy = end.y.0 - start.y.0;
    let length = (dx * dx + dy * dy).sqrt();
    if length <= f32::EPSILON || width.0 <= 0.0 {
        return;
    }
    let padding = width.0 / 2.0 + 1.0;
    let nx = -dy / length * padding;
    let ny = dx / length * padding;
    let points = [
        Point {
            x: Dip(start.x.0 + nx),
            y: Dip(start.y.0 + ny),
        },
        Point {
            x: Dip(end.x.0 + nx),
            y: Dip(end.y.0 + ny),
        },
        Point {
            x: Dip(end.x.0 - nx),
            y: Dip(end.y.0 - ny),
        },
        Point {
            x: Dip(start.x.0 - nx),
            y: Dip(start.y.0 - ny),
        },
    ];
    let to_vertex = |point: Point| LineVertex {
        position: [
            point.x.0 / size.width.max(1) as f32 * 2.0 - 1.0,
            1.0 - point.y.0 / size.height.max(1) as f32 * 2.0,
        ],
        point: [point.x.0, point.y.0],
        start: [start.x.0, start.y.0],
        end: [end.x.0, end.y.0],
        width: width.0,
        color: [color.r, color.g, color.b, color.a],
    };
    vertices.extend([
        to_vertex(points[0]),
        to_vertex(points[1]),
        to_vertex(points[2]),
        to_vertex(points[0]),
        to_vertex(points[2]),
        to_vertex(points[3]),
    ]);
}

fn append_image(vertices: &mut Vec<ImageVertex>, rect: Rect, opacity: f32, size: PhysicalSize) {
    let left = rect.origin.x.0 / size.width.max(1) as f32 * 2.0 - 1.0;
    let right = (rect.origin.x.0 + rect.size.width.0) / size.width.max(1) as f32 * 2.0 - 1.0;
    let top = 1.0 - rect.origin.y.0 / size.height.max(1) as f32 * 2.0;
    let bottom = 1.0 - (rect.origin.y.0 + rect.size.height.0) / size.height.max(1) as f32 * 2.0;
    vertices.extend([
        ImageVertex {
            position: [left, top],
            uv: [0.0, 0.0],
            opacity,
        },
        ImageVertex {
            position: [right, top],
            uv: [1.0, 0.0],
            opacity,
        },
        ImageVertex {
            position: [right, bottom],
            uv: [1.0, 1.0],
            opacity,
        },
        ImageVertex {
            position: [left, top],
            uv: [0.0, 0.0],
            opacity,
        },
        ImageVertex {
            position: [right, bottom],
            uv: [1.0, 1.0],
            opacity,
        },
        ImageVertex {
            position: [left, bottom],
            uv: [0.0, 1.0],
            opacity,
        },
    ]);
}

fn set_scissor(
    pass: &mut wgpu::RenderPass<'_>,
    rect: Option<Rect>,
    size: PhysicalSize,
    scale_factor: ScaleFactor,
) {
    let rect = rect.unwrap_or(Rect {
        origin: Point::default(),
        size: zui_core::Size {
            width: Dip(size.width as f32 / scale_factor.0 as f32),
            height: Dip(size.height as f32 / scale_factor.0 as f32),
        },
    });
    let scale = scale_factor.0.max(1.0) as f32;
    let x = (rect.origin.x.0 * scale).max(0.0).min(size.width as f32) as u32;
    let y = (rect.origin.y.0 * scale).max(0.0).min(size.height as f32) as u32;
    let right = ((rect.origin.x.0 + rect.size.width.0) * scale)
        .min(size.width as f32)
        .max(x as f32) as u32;
    let bottom = ((rect.origin.y.0 + rect.size.height.0) * scale)
        .min(size.height as f32)
        .max(y as f32) as u32;
    pass.set_scissor_rect(x, y, right.saturating_sub(x), bottom.saturating_sub(y));
}

fn command_list_hash(commands: &[PaintCommand]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    format!("{commands:?}").hash(&mut hasher);
    hasher.finish()
}

fn append_rounded_rect(
    vertices: &mut Vec<RoundedRectVertex>,
    rect: Rect,
    radius: Dip,
    color: Color,
    size: PhysicalSize,
) {
    let radius = radius
        .0
        .max(0.0)
        .min(rect.size.width.0 / 2.0)
        .min(rect.size.height.0 / 2.0);
    let left = rect.origin.x.0 / size.width.max(1) as f32 * 2.0 - 1.0;
    let right = (rect.origin.x.0 + rect.size.width.0) / size.width.max(1) as f32 * 2.0 - 1.0;
    let top = 1.0 - rect.origin.y.0 / size.height.max(1) as f32 * 2.0;
    let bottom = 1.0 - (rect.origin.y.0 + rect.size.height.0) / size.height.max(1) as f32 * 2.0;
    let color = [color.r, color.g, color.b, color.a];
    let make_vertex = |position: [f32; 2], local: [f32; 2]| RoundedRectVertex {
        position,
        local,
        size: [rect.size.width.0, rect.size.height.0],
        radius,
        color,
    };
    vertices.extend([
        make_vertex([left, top], [0.0, 0.0]),
        make_vertex([right, top], [rect.size.width.0, 0.0]),
        make_vertex([right, bottom], [rect.size.width.0, rect.size.height.0]),
        make_vertex([left, top], [0.0, 0.0]),
        make_vertex([right, bottom], [rect.size.width.0, rect.size.height.0]),
        make_vertex([left, bottom], [0.0, rect.size.height.0]),
    ]);
}

fn glyph_rows(character: char) -> [u8; 7] {
    match character.to_ascii_uppercase() {
        'A' => [
            0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'B' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110,
        ],
        'C' => [
            0b01111, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b01111,
        ],
        'D' => [
            0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110,
        ],
        'E' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111,
        ],
        'F' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'G' => [
            0b01111, 0b10000, 0b10000, 0b10111, 0b10001, 0b10001, 0b01111,
        ],
        'H' => [
            0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'I' => [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b11111,
        ],
        'J' => [
            0b00111, 0b00010, 0b00010, 0b00010, 0b00010, 0b10010, 0b01100,
        ],
        'K' => [
            0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001,
        ],
        'L' => [
            0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111,
        ],
        'M' => [
            0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001,
        ],
        'N' => [
            0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001,
        ],
        'O' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'P' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'Q' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101,
        ],
        'R' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001,
        ],
        'S' => [
            0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        'T' => [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'U' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'V' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100,
        ],
        'W' => [
            0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b11011, 0b10001,
        ],
        'X' => [
            0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001,
        ],
        'Y' => [
            0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'Z' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111,
        ],
        '0' => [
            0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110,
        ],
        '1' => [
            0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        '2' => [
            0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111,
        ],
        '3' => [
            0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        '4' => [
            0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010,
        ],
        '5' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b00001, 0b00001, 0b11110,
        ],
        '6' => [
            0b01110, 0b10000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110,
        ],
        '7' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000,
        ],
        '8' => [
            0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
        ],
        '9' => [
            0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00001, 0b01110,
        ],
        _ => [0; 7],
    }
}

fn create_canvas(
    device: &wgpu::Device,
    size: PhysicalSize,
    format: wgpu::TextureFormat,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("zui-render canvas"),
        size: wgpu::Extent3d {
            width: size.width.max(1),
            height: size.height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

fn create_stencil(device: &wgpu::Device, size: PhysicalSize) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("zui-render clip stencil"),
        size: wgpu::Extent3d {
            width: size.width.max(1),
            height: size.height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth24PlusStencil8,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

fn create_stencil_reset_buffer(device: &wgpu::Device) -> wgpu::Buffer {
    let color = [1.0, 1.0, 1.0, 1.0];
    let vertices = [
        RectVertex {
            position: [-1.0, 1.0],
            color,
        },
        RectVertex {
            position: [1.0, 1.0],
            color,
        },
        RectVertex {
            position: [1.0, -1.0],
            color,
        },
        RectVertex {
            position: [-1.0, 1.0],
            color,
        },
        RectVertex {
            position: [1.0, -1.0],
            color,
        },
        RectVertex {
            position: [-1.0, -1.0],
            color,
        },
    ];
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("zui-render stencil reset"),
        contents: bytemuck::cast_slice(&vertices),
        usage: wgpu::BufferUsages::VERTEX,
    })
}

fn stencil_draw_state() -> wgpu::DepthStencilState {
    wgpu::DepthStencilState {
        format: wgpu::TextureFormat::Depth24PlusStencil8,
        depth_write_enabled: Some(false),
        depth_compare: Some(wgpu::CompareFunction::Always),
        stencil: wgpu::StencilState {
            front: wgpu::StencilFaceState {
                compare: wgpu::CompareFunction::Equal,
                fail_op: wgpu::StencilOperation::Keep,
                depth_fail_op: wgpu::StencilOperation::Keep,
                pass_op: wgpu::StencilOperation::Keep,
            },
            back: wgpu::StencilFaceState {
                compare: wgpu::CompareFunction::Equal,
                fail_op: wgpu::StencilOperation::Keep,
                depth_fail_op: wgpu::StencilOperation::Keep,
                pass_op: wgpu::StencilOperation::Keep,
            },
            read_mask: 1,
            write_mask: 0,
        },
        bias: wgpu::DepthBiasState::default(),
    }
}

fn stencil_mask_state() -> wgpu::DepthStencilState {
    wgpu::DepthStencilState {
        format: wgpu::TextureFormat::Depth24PlusStencil8,
        depth_write_enabled: Some(false),
        depth_compare: Some(wgpu::CompareFunction::Always),
        stencil: wgpu::StencilState {
            front: wgpu::StencilFaceState {
                compare: wgpu::CompareFunction::Equal,
                fail_op: wgpu::StencilOperation::Keep,
                depth_fail_op: wgpu::StencilOperation::Keep,
                pass_op: wgpu::StencilOperation::IncrementClamp,
            },
            back: wgpu::StencilFaceState {
                compare: wgpu::CompareFunction::Equal,
                fail_op: wgpu::StencilOperation::Keep,
                depth_fail_op: wgpu::StencilOperation::Keep,
                pass_op: wgpu::StencilOperation::IncrementClamp,
            },
            read_mask: 0xff,
            write_mask: 1,
        },
        bias: wgpu::DepthBiasState::default(),
    }
}

fn stencil_reset_state() -> wgpu::DepthStencilState {
    let mut state = stencil_mask_state();
    state.stencil.front.compare = wgpu::CompareFunction::Always;
    state.stencil.back.compare = wgpu::CompareFunction::Always;
    state.stencil.read_mask = 0;
    state.stencil.front.pass_op = wgpu::StencilOperation::Replace;
    state.stencil.back.pass_op = wgpu::StencilOperation::Replace;
    state
}

fn create_blit_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("zui-render canvas blit shader"),
        source: wgpu::ShaderSource::Wgsl(
            r#"
            @group(0) @binding(0) var canvas: texture_2d<f32>;
            @group(0) @binding(1) var canvas_sampler: sampler;

            struct VertexOutput {
                @builtin(position) position: vec4<f32>,
                @location(0) uv: vec2<f32>,
            };

            @vertex
            fn vs(@builtin(vertex_index) index: u32) -> VertexOutput {
                var positions = array<vec2<f32>, 3>(
                    vec2<f32>(-1.0, -1.0),
                    vec2<f32>(3.0, -1.0),
                    vec2<f32>(-1.0, 3.0)
                );
                var uvs = array<vec2<f32>, 3>(
                    vec2<f32>(0.0, 1.0),
                    vec2<f32>(2.0, 1.0),
                    vec2<f32>(0.0, -1.0)
                );
                var output: VertexOutput;
                output.position = vec4<f32>(positions[index], 0.0, 1.0);
                output.uv = uvs[index];
                return output;
            }

            @fragment
            fn fs(input: VertexOutput) -> @location(0) vec4<f32> {
                return textureSample(canvas, canvas_sampler, input.uv);
            }
        "#
            .into(),
        ),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("zui-render canvas blit pipeline"),
        layout: None,
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

fn create_blit_bind_group(
    device: &wgpu::Device,
    pipeline: &wgpu::RenderPipeline,
    canvas_view: &wgpu::TextureView,
) -> wgpu::BindGroup {
    let layout = pipeline.get_bind_group_layout(0);
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("zui-render canvas sampler"),
        mag_filter: wgpu::FilterMode::Nearest,
        min_filter: wgpu::FilterMode::Nearest,
        ..Default::default()
    });
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("zui-render canvas bind group"),
        layout: &layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(canvas_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    })
}

fn create_stencil_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    create_stencil_pipeline_with_state(device, format, stencil_reset_state())
}

fn create_stencil_mask_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    create_stencil_pipeline_with_state(device, format, stencil_mask_state())
}

fn create_stencil_pipeline_with_state(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    stencil_state: wgpu::DepthStencilState,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("zui-render stencil mask shader"),
        source: wgpu::ShaderSource::Wgsl(
            r#"
            @vertex
            fn vs(@location(0) position: vec2<f32>, @location(1) color: vec4<f32>) -> @builtin(position) vec4<f32> {
                return vec4<f32>(position, 0.0, 1.0);
            }

            @fragment
            fn fs() -> @location(0) vec4<f32> {
                return vec4<f32>(0.0);
            }
        "#
            .into(),
        ),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("zui-render stencil mask pipeline"),
        layout: None,
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<RectVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4],
            }],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::empty(),
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(stencil_state),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

fn create_rect_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("zui-render rectangle shader"),
        source: wgpu::ShaderSource::Wgsl(
            r#"
            struct VertexOutput {
                @builtin(position) position: vec4<f32>,
                @location(0) color: vec4<f32>,
            };
            @vertex
            fn vs(@location(0) position: vec2<f32>, @location(1) color: vec4<f32>) -> VertexOutput {
                var output: VertexOutput;
                output.position = vec4<f32>(position, 0.0, 1.0);
                output.color = color;
                return output;
            }
            @fragment
            fn fs(@location(0) color: vec4<f32>) -> @location(0) vec4<f32> { return color; }
        "#
            .into(),
        ),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("zui-render rectangle pipeline"),
        layout: None,
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<RectVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4],
            }],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(stencil_draw_state()),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

fn create_image_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("zui-render image shader"),
        source: wgpu::ShaderSource::Wgsl(
            r#"
            @group(0) @binding(0) var image: texture_2d<f32>;
            @group(0) @binding(1) var image_sampler: sampler;

            struct VertexOutput {
                @builtin(position) position: vec4<f32>,
                @location(0) uv: vec2<f32>,
                @location(1) opacity: f32,
            };

            @vertex
            fn vs(
                @location(0) position: vec2<f32>,
                @location(1) uv: vec2<f32>,
                @location(2) opacity: f32,
            ) -> VertexOutput {
                var output: VertexOutput;
                output.position = vec4<f32>(position, 0.0, 1.0);
                output.uv = uv;
                output.opacity = opacity;
                return output;
            }

            @fragment
            fn fs(input: VertexOutput) -> @location(0) vec4<f32> {
                let color = textureSample(image, image_sampler, input.uv);
                return vec4<f32>(color.rgb, color.a * input.opacity);
            }
        "#
            .into(),
        ),
    });
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("zui-render image bind group layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("zui-render image pipeline layout"),
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("zui-render image pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<ImageVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32],
            }],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(stencil_draw_state()),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

fn create_rounded_rect_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("zui-render rounded rectangle SDF shader"),
        source: wgpu::ShaderSource::Wgsl(
            r#"
            struct VertexOutput {
                @builtin(position) position: vec4<f32>,
                @location(0) local: vec2<f32>,
                @location(1) size: vec2<f32>,
                @location(2) radius: f32,
                @location(3) color: vec4<f32>,
            };

            @vertex
            fn vs(
                @location(0) position: vec2<f32>,
                @location(1) local: vec2<f32>,
                @location(2) size: vec2<f32>,
                @location(3) radius: f32,
                @location(4) color: vec4<f32>,
            ) -> VertexOutput {
                var output: VertexOutput;
                output.position = vec4<f32>(position, 0.0, 1.0);
                output.local = local;
                output.size = size;
                output.radius = radius;
                output.color = color;
                return output;
            }

            @fragment
            fn fs(input: VertexOutput) -> @location(0) vec4<f32> {
                let half_size = input.size * 0.5;
                let point = input.local - half_size;
                let rounded = half_size - vec2<f32>(input.radius, input.radius);
                let q = abs(point) - rounded;
                let distance = length(max(q, vec2<f32>(0.0, 0.0)))
                    + min(max(q.x, q.y), 0.0)
                    - input.radius;
                let antialias = max(fwidth(distance), 0.0001);
                let alpha = 1.0 - smoothstep(-antialias, antialias, distance);
                return vec4<f32>(input.color.rgb, input.color.a * alpha);
            }
        "#
            .into(),
        ),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("zui-render rounded rectangle SDF pipeline"),
        layout: None,
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<RoundedRectVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![
                    0 => Float32x2,
                    1 => Float32x2,
                    2 => Float32x2,
                    3 => Float32,
                    4 => Float32x4,
                ],
            }],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(stencil_draw_state()),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

fn create_stencil_rounded_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("zui-render rounded stencil mask shader"),
        source: wgpu::ShaderSource::Wgsl(
            r#"
            struct VertexOutput {
                @builtin(position) position: vec4<f32>,
                @location(0) local: vec2<f32>,
                @location(1) size: vec2<f32>,
                @location(2) radius: f32,
            };
            @vertex
            fn vs(@location(0) position: vec2<f32>, @location(1) local: vec2<f32>,
                  @location(2) size: vec2<f32>, @location(3) radius: f32,
                  @location(4) color: vec4<f32>) -> VertexOutput {
                var output: VertexOutput;
                output.position = vec4<f32>(position, 0.0, 1.0);
                output.local = local;
                output.size = size;
                output.radius = radius;
                return output;
            }
            @fragment
            fn fs(input: VertexOutput) -> @location(0) vec4<f32> {
                let half_size = input.size * 0.5;
                let point = input.local - half_size;
                let rounded = half_size - vec2<f32>(input.radius, input.radius);
                let q = abs(point) - rounded;
                let distance = length(max(q, vec2<f32>(0.0, 0.0)))
                    + min(max(q.x, q.y), 0.0) - input.radius;
                if distance > 0.0 { discard; }
                return vec4<f32>(0.0);
            }
        "#
            .into(),
        ),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("zui-render rounded stencil mask pipeline"),
        layout: None,
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<RoundedRectVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![
                    0 => Float32x2, 1 => Float32x2, 2 => Float32x2, 3 => Float32, 4 => Float32x4,
                ],
            }],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::empty(),
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(stencil_mask_state()),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

fn create_line_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("zui-render antialiased line shader"),
        source: wgpu::ShaderSource::Wgsl(
            r#"
            struct VertexOutput {
                @builtin(position) position: vec4<f32>,
                @location(0) point: vec2<f32>,
                @location(1) start: vec2<f32>,
                @location(2) end: vec2<f32>,
                @location(3) width: f32,
                @location(4) color: vec4<f32>,
            };

            @vertex
            fn vs(
                @location(0) position: vec2<f32>,
                @location(1) point: vec2<f32>,
                @location(2) start: vec2<f32>,
                @location(3) end: vec2<f32>,
                @location(4) width: f32,
                @location(5) color: vec4<f32>,
            ) -> VertexOutput {
                var output: VertexOutput;
                output.position = vec4<f32>(position, 0.0, 1.0);
                output.point = point;
                output.start = start;
                output.end = end;
                output.width = width;
                output.color = color;
                return output;
            }

            @fragment
            fn fs(input: VertexOutput) -> @location(0) vec4<f32> {
                let segment = input.end - input.start;
                let segment_length = max(dot(segment, segment), 0.0001);
                let projection = clamp(
                    dot(input.point - input.start, segment) / segment_length,
                    0.0,
                    1.0,
                );
                let nearest = input.start + segment * projection;
                let line_distance = distance(input.point, nearest) - input.width * 0.5;
                let antialias = max(fwidth(line_distance), 0.0001);
                let alpha = 1.0 - smoothstep(-antialias, antialias, line_distance);
                return vec4<f32>(input.color.rgb, input.color.a * alpha);
            }
        "#
            .into(),
        ),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("zui-render antialiased line pipeline"),
        layout: None,
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<LineVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![
                    0 => Float32x2,
                    1 => Float32x2,
                    2 => Float32x2,
                    3 => Float32x2,
                    4 => Float32,
                    5 => Float32x4,
                ],
            }],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(stencil_draw_state()),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noop_device_can_create_gpu_objects() {
        let (device, _queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let _encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        let _pipeline = create_rounded_rect_pipeline(&device, wgpu::TextureFormat::Rgba8Unorm);
        let _stencil_pipeline = create_stencil_pipeline(&device, wgpu::TextureFormat::Rgba8Unorm);
        let _rounded_stencil_pipeline =
            create_stencil_rounded_pipeline(&device, wgpu::TextureFormat::Rgba8Unorm);
        let _line_pipeline = create_line_pipeline(&device, wgpu::TextureFormat::Rgba8Unorm);
        let _image_pipeline = create_image_pipeline(&device, wgpu::TextureFormat::Rgba8Unorm);
    }

    #[test]
    fn render_node_flattens_commands_before_children() {
        let mut node = RenderNode::new(Rect::default());
        node.commands.push(PaintCommand::Rect {
            rect: Rect::default(),
            color: Color::WHITE,
        });
        let mut child = RenderNode::new(Rect::default());
        child.commands.push(PaintCommand::Clear(Color::BLACK));
        node.add_child(child);
        assert!(matches!(node.commands[0], PaintCommand::Rect { .. }));
        assert!(matches!(
            node.children[0].commands[0],
            PaintCommand::Clear(_)
        ));
    }

    #[test]
    fn render_node_index_resolves_nested_widget_paths() {
        let mut root = RenderNode::new(Rect::default());
        root.source_id = Some(1);
        let mut child = RenderNode::new(Rect::default());
        child.source_id = Some(2);
        let mut grandchild = RenderNode::new(Rect::default());
        grandchild.source_id = Some(3);
        child.add_child(grandchild);
        root.add_child(child);

        let index = root.build_index();
        assert_eq!(index.path_for(1), Some([].as_slice()));
        assert_eq!(index.path_for(3), Some([0, 0].as_slice()));
        assert_eq!(
            index.node(&root, 3).and_then(|node| node.source_id),
            Some(3)
        );
    }

    #[test]
    fn clean_render_node_subtrees_are_reused_by_source_id() {
        let mut previous = RenderNode::new(Rect::default());
        previous.source_id = Some(1);
        let mut previous_clean = RenderNode::new(Rect::default());
        previous_clean.source_id = Some(2);
        previous_clean.commands.push(PaintCommand::Rect {
            rect: Rect::default(),
            color: Color::WHITE,
        });
        previous.add_child(previous_clean);
        let mut previous_dirty = RenderNode::new(Rect::default());
        previous_dirty.source_id = Some(3);
        previous.add_child(previous_dirty);

        let mut current = previous.clone();
        current.children[0].commands[0] = PaintCommand::Rect {
            rect: Rect::default(),
            color: Color::BLACK,
        };
        current.children[1]
            .commands
            .push(PaintCommand::Clear(Color::BLACK));
        let dirty = vec![vec![1]];
        let merged = current.reuse_clean_subtrees(Some(&previous), &dirty, false);

        assert_eq!(
            merged.children[0].commands[0],
            previous.children[0].commands[0]
        );
        assert_eq!(merged.children[1].commands.len(), 1);
    }

    #[test]
    fn shared_segment_damage_is_coalesced_once() {
        let segments = vec![RenderSegment {
            key: 1,
            bounds: Rect {
                origin: Point {
                    x: Dip(4.0),
                    y: Dip(4.0),
                },
                size: zui_core::Size {
                    width: Dip(4.0),
                    height: Dip(4.0),
                },
            },
            transform: Transform::IDENTITY,
            clip: None,
            opacity: 1.0,
            clips: Vec::new(),
            commands: Vec::new(),
        }];
        let regions = [
            Rect {
                origin: Point::default(),
                size: zui_core::Size {
                    width: Dip(6.0),
                    height: Dip(10.0),
                },
            },
            Rect {
                origin: Point {
                    x: Dip(6.0),
                    y: Dip(0.0),
                },
                size: zui_core::Size {
                    width: Dip(6.0),
                    height: Dip(10.0),
                },
            },
        ];
        let merged = coalesce_damage_for_segments(&regions, &segments);
        assert_eq!(merged.len(), 1);
    }

    #[test]
    fn render_node_applies_transform_and_opacity() {
        let mut node = RenderNode::new(Rect::default());
        node.set_transform(Transform::translate(Dip(10.0), Dip(20.0)));
        node.set_opacity(0.5);
        node.commands.push(PaintCommand::Rect {
            rect: Rect {
                origin: Point::default(),
                size: zui_core::Size {
                    width: Dip(4.0),
                    height: Dip(5.0),
                },
            },
            color: Color::WHITE,
        });
        assert_eq!(node.transform, Transform::translate(Dip(10.0), Dip(20.0)));
        assert_eq!(node.opacity, 0.5);
        assert_eq!(node.commands.len(), 1);
        assert!(matches!(node.commands[0], PaintCommand::Rect { .. }));
    }

    #[test]
    fn render_node_clips_commands_and_propagates_dirty_state() {
        let mut node = RenderNode::new(Rect::default());
        node.set_clip(Some(ClipShape::Rect(Rect {
            origin: Point::default(),
            size: zui_core::Size {
                width: Dip(5.0),
                height: Dip(5.0),
            },
        })));
        node.commands.push(PaintCommand::Rect {
            rect: Rect {
                origin: Point {
                    x: Dip(20.0),
                    y: Dip(20.0),
                },
                size: zui_core::Size {
                    width: Dip(2.0),
                    height: Dip(2.0),
                },
            },
            color: Color::WHITE,
        });
        assert!(node.dirty.flags.contains(DirtyFlags::PAINT));
        assert_eq!(
            node.clip,
            Some(ClipShape::Rect(Rect {
                origin: Point::default(),
                size: zui_core::Size {
                    width: Dip(5.0),
                    height: Dip(5.0)
                },
            }))
        );

        node.clear_dirty();
        assert!(!node.dirty.is_dirty());
        assert!(node.dirty.flags.is_empty());
        node.mark_dirty_region(Rect::default());
        assert!(node.dirty.is_dirty());
        assert_eq!(node.dirty.regions.union(), Some(Rect::default()));
    }

    #[test]
    fn paint_commands_validate_geometry_and_report_bounds() {
        let command = PaintCommand::RoundedRect {
            rect: Rect {
                origin: Point {
                    x: Dip(2.0),
                    y: Dip(3.0),
                },
                size: zui_core::Size {
                    width: Dip(10.0),
                    height: Dip(8.0),
                },
            },
            radius: Dip(2.0),
            color: Color::WHITE,
        };
        assert!(command.validate().is_ok());
        assert_eq!(command.bounds().unwrap().size.width, Dip(10.0));

        let invalid = PaintCommand::Line {
            start: Point::default(),
            end: Point {
                x: Dip(1.0),
                y: Dip(1.0),
            },
            width: Dip(0.0),
            color: Color::WHITE,
        };
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn image_resource_requires_matching_rgba_data() {
        assert!(ImageResource::new(2, 2, vec![0; 16]).is_ok());
        assert!(ImageResource::new(2, 2, vec![0; 15]).is_err());
    }

    #[test]
    fn clip_commands_are_preserved_for_gpu_scissor_segments() {
        let mut commands = Vec::new();
        commands.push(PaintCommand::Clip {
            shape: ClipShape::Rect(Rect {
                origin: Point {
                    x: Dip(1.0),
                    y: Dip(2.0),
                },
                size: zui_core::Size {
                    width: Dip(10.0),
                    height: Dip(11.0),
                },
            }),
        });
        commands.push(PaintCommand::Rect {
            rect: Rect {
                origin: Point {
                    x: Dip(2.0),
                    y: Dip(3.0),
                },
                size: zui_core::Size {
                    width: Dip(4.0),
                    height: Dip(5.0),
                },
            },
            color: Color::WHITE,
        });
        let resolved = resolve_commands(&commands);
        assert!(matches!(resolved.first(), Some(PaintCommand::Clip { .. })));
        assert!(matches!(resolved.get(1), Some(PaintCommand::Rect { .. })));
    }

    #[test]
    fn normalizing_nested_cached_nodes_is_idempotent() {
        let mut root = RenderNode::for_widget(Rect {
            origin: Point {
                x: Dip(10.0),
                y: Dip(20.0),
            },
            size: zui_core::Size {
                width: Dip(100.0),
                height: Dip(80.0),
            },
        });
        root.add_child(RenderNode::for_widget(Rect {
            origin: Point {
                x: Dip(30.0),
                y: Dip(50.0),
            },
            size: zui_core::Size {
                width: Dip(20.0),
                height: Dip(10.0),
            },
        }));
        root.normalize_local_coordinates();
        assert_eq!(root.transform.matrix[4], 10.0);
        assert_eq!(root.children[0].transform.matrix[4], 20.0);
        let snapshot = root.clone();
        root.normalize_local_coordinates();
        assert_eq!(root, snapshot);
    }

    #[test]
    fn dirty_regions_keep_separate_damage_areas() {
        let mut regions = DirtyRegionSet::new();
        regions.add(Rect {
            origin: Point {
                x: Dip(1.0),
                y: Dip(1.0),
            },
            size: zui_core::Size {
                width: Dip(4.0),
                height: Dip(4.0),
            },
        });
        regions.add(Rect {
            origin: Point {
                x: Dip(20.0),
                y: Dip(20.0),
            },
            size: zui_core::Size {
                width: Dip(4.0),
                height: Dip(4.0),
            },
        });
        assert_eq!(regions.as_slice().len(), 2);
        regions.add(Rect {
            origin: Point {
                x: Dip(3.0),
                y: Dip(3.0),
            },
            size: zui_core::Size {
                width: Dip(4.0),
                height: Dip(4.0),
            },
        });
        assert_eq!(regions.as_slice().len(), 2);
    }

    #[test]
    fn render_node_gpu_key_changes_with_subtree() {
        let mut parent = RenderNode::new(Rect::default());
        let first = parent.gpu_cache_key();
        parent.add_child(RenderNode::new(Rect {
            origin: Point::default(),
            size: zui_core::Size {
                width: Dip(4.0),
                height: Dip(4.0),
            },
        }));
        assert_ne!(first, parent.gpu_cache_key());
    }
}
