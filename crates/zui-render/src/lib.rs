//! GPU rendering owned by the application composition root.
//!
//! This crate deliberately exposes no `wgpu::Device` to widgets. Widgets produce
//! drawing data; `Renderer` owns the GPU and the per-window swap-chain surfaces.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Mutex, OnceLock};

use wgpu::util::DeviceExt;
use zui_core::{Color, Dip, PhysicalSize, Point, Rect, ScaleFactor, WindowId};
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
        rect: Rect,
    },
    Transform(Transform),
    Opacity(f32),
}

impl PaintCommand {
    pub fn bounds(&self) -> Option<Rect> {
        match self {
            Self::Clear(_) | Self::Transform(_) | Self::Opacity(_) => None,
            Self::Rect { rect, .. }
            | Self::RoundedRect { rect, .. }
            | Self::Image { rect, .. }
            | Self::Clip { rect } => Some(*rect),
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
            Self::Clip { rect } => validate_rect(*rect),
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

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DisplayList {
    commands: Vec<PaintCommand>,
}

impl DisplayList {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn clear(&mut self, color: Color) {
        self.commands.clear();
        self.commands.push(PaintCommand::Clear(color));
    }
    pub fn fill_rect(&mut self, rect: Rect, color: Color) {
        self.commands.push(PaintCommand::Rect { rect, color });
    }
    pub fn fill_rounded_rect(&mut self, rect: Rect, radius: Dip, color: Color) {
        self.commands.push(PaintCommand::RoundedRect {
            rect,
            radius,
            color,
        });
    }
    pub fn line(&mut self, start: Point, end: Point, width: Dip, color: Color) {
        self.commands.push(PaintCommand::Line {
            start,
            end,
            width,
            color,
        });
    }
    pub fn text(&mut self, text: impl Into<String>, origin: Point, color: Color, scale: u32) {
        self.commands.push(PaintCommand::Text {
            text: text.into(),
            origin,
            color,
            scale: scale.max(1),
        });
    }
    pub fn icon(&mut self, rect: Rect, path: IconPath, color: Color, stroke: Dip) {
        self.commands.push(PaintCommand::Icon {
            rect,
            path,
            color,
            stroke,
        });
    }
    pub fn image(&mut self, rect: Rect, image: ImageId, opacity: f32) {
        self.commands.push(PaintCommand::Image {
            rect,
            image,
            opacity: opacity.clamp(0.0, 1.0),
        });
    }
    pub fn clip(&mut self, rect: Rect) {
        self.commands.push(PaintCommand::Clip { rect });
    }
    pub fn transform(&mut self, transform: Transform) {
        self.commands.push(PaintCommand::Transform(transform));
    }
    pub fn opacity(&mut self, opacity: f32) {
        self.commands
            .push(PaintCommand::Opacity(opacity.clamp(0.0, 1.0)));
    }
    pub fn commands(&self) -> &[PaintCommand] {
        &self.commands
    }

    pub fn validate(&self) -> Result<(), RenderError> {
        self.commands.iter().try_for_each(PaintCommand::validate)
    }

    pub fn append(&mut self, other: &mut Self) {
        self.commands.append(&mut other.commands);
    }
}

/// Retained intermediate representation between widgets and the renderer.
///
/// The current renderer still consumes a flattened `DisplayList`; keeping the
/// node tree here allows caching, clipping and partial rebuilds to be added
/// without changing widget painting APIs again.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RenderNode {
    /// Bounds in the node's local coordinate system.
    pub local_bounds: Rect,
    /// The originating widget identity, when this node was built by a Widget.
    pub source_id: Option<u64>,
    pub transform: Transform,
    pub clip: Option<Rect>,
    pub opacity: f32,
    pub commands: DisplayList,
    pub children: Vec<Self>,
    pub dirty_flags: DirtyFlags,
    /// Compatibility view for callers that only need to know if a node is dirty.
    pub dirty: bool,
    pub dirty_region: Option<Rect>,
}

impl RenderNode {
    pub fn new(bounds: Rect) -> Self {
        Self {
            local_bounds: bounds,
            source_id: None,
            transform: Transform::IDENTITY,
            clip: None,
            opacity: 1.0,
            commands: DisplayList::new(),
            children: Vec::new(),
            dirty_flags: DirtyFlags::LAYOUT.union(DirtyFlags::PAINT),
            dirty: true,
            dirty_region: None,
        }
    }

    pub fn add_child(&mut self, child: Self) {
        self.children.push(child);
        self.mark_dirty(DirtyFlags::CHILDREN);
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
        self.dirty || !self.dirty_flags.is_empty() || self.children.iter().any(Self::is_dirty)
    }

    pub fn accumulated_dirty_region(&self) -> Option<Rect> {
        let mut region = self.dirty_region;
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

    pub fn set_clip(&mut self, clip: Option<Rect>) {
        self.clip = clip;
        self.mark_dirty(DirtyFlags::PAINT);
    }

    pub fn set_opacity(&mut self, opacity: f32) {
        self.opacity = opacity.clamp(0.0, 1.0);
        self.mark_dirty(DirtyFlags::PAINT);
    }

    pub fn mark_dirty(&mut self, flags: DirtyFlags) {
        self.dirty_flags = self.dirty_flags.union(flags);
        self.dirty = true;
    }

    pub fn mark_dirty_region(&mut self, region: Rect) {
        self.mark_dirty(DirtyFlags::PAINT);
        self.dirty_region = Some(match self.dirty_region {
            Some(current) => union_rect(current, region),
            None => region,
        });
    }

    pub fn clear_dirty(&mut self) {
        self.dirty_flags = DirtyFlags::empty();
        self.dirty = false;
        self.dirty_region = None;
        for child in &mut self.children {
            child.clear_dirty();
        }
    }

    pub fn flatten_into(&self, display_list: &mut DisplayList) {
        self.flatten_with_state(display_list, Transform::IDENTITY, None, 1.0);
    }

    fn flatten_with_state(
        &self,
        display_list: &mut DisplayList,
        parent_transform: Transform,
        parent_clip: Option<Rect>,
        parent_opacity: f32,
    ) {
        let transform = compose_transform(parent_transform, self.transform);
        let clip = intersect_clip(parent_clip, self.clip.map(|clip| transform.rect(clip)));
        let opacity = parent_opacity * self.opacity;
        for command in self.commands.commands() {
            if let Some(command) = transform_command(command, transform, clip, opacity) {
                display_list.commands.push(command);
            }
        }
        for child in &self.children {
            child.flatten_with_state(display_list, transform, clip, opacity);
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
        PaintCommand::Clip { rect } => Some(PaintCommand::Clip {
            rect: transform.rect(*rect),
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

    pub fn commands_mut(&mut self) -> &mut DisplayList {
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

    pub fn clip(&mut self, clip: Option<Rect>) -> &mut Self {
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
    line_pipeline: wgpu::RenderPipeline,
    image_pipeline: wgpu::RenderPipeline,
    canvas: wgpu::Texture,
    canvas_view: wgpu::TextureView,
    blit_pipeline: wgpu::RenderPipeline,
    blit_bind_group: wgpu::BindGroup,
    scale_factor: ScaleFactor,
    has_contents: bool,
    batch_cache: Option<(u64, Vec<GpuBatch>)>,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
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
    Clip(Option<Rect>),
}

#[derive(Clone, Copy)]
enum BatchKind {
    Rect,
    Rounded,
    Line,
    Image(ImageId),
}

enum GpuBatch {
    Draw(BatchKind, wgpu::Buffer, u32),
    Clip(Option<Rect>),
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
    fonts: &'static [fontdue::Font],
    glyph_cache: HashMap<(usize, char, u32, u32), CachedGlyph>,
    font_cache: HashMap<char, Option<usize>>,
    resources: ResourceCache,
    gpu_images: HashMap<ImageId, GpuImage>,
    image_sampler: wgpu::Sampler,
    image_bind_groups: HashMap<(WindowId, ImageId), wgpu::BindGroup>,
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
        let image_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("zui-render image sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Ok(Self {
            instance,
            adapter,
            device,
            queue,
            surfaces: HashMap::new(),
            fonts: cached_system_fonts(),
            glyph_cache: HashMap::new(),
            font_cache: HashMap::new(),
            resources: ResourceCache::default(),
            gpu_images: HashMap::new(),
            image_sampler,
            image_bind_groups: HashMap::new(),
        })
    }

    pub fn new_blocking() -> Result<Self, RenderError> {
        pollster::block_on(Self::new())
    }

    pub fn register_image(&mut self, id: ImageId, image: ImageResource) {
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
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
        self.queue.write_texture(
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
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.gpu_images.insert(
            id,
            GpuImage {
                _texture: texture,
                view,
            },
        );
        self.image_bind_groups
            .retain(|(_, image_id), _| *image_id != id);
        self.resources.register_image(id, image);
    }

    pub fn remove_image(&mut self, id: ImageId) -> Option<ImageResource> {
        self.gpu_images.remove(&id);
        self.image_bind_groups
            .retain(|(_, image_id), _| *image_id != id);
        self.resources.remove_image(id)
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
        let line_pipeline = create_line_pipeline(&self.device, config.format);
        let image_pipeline = create_image_pipeline(&self.device, config.format);
        let (canvas, canvas_view) = create_canvas(&self.device, size, config.format);
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
                line_pipeline,
                image_pipeline,
                canvas,
                canvas_view,
                blit_pipeline,
                blit_bind_group,
                scale_factor,
                has_contents: false,
                batch_cache: None,
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
        let line_pipeline = create_line_pipeline(&self.device, config.format);
        let image_pipeline = create_image_pipeline(&self.device, config.format);
        let (canvas, canvas_view) = create_canvas(&self.device, size, config.format);
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
                line_pipeline,
                image_pipeline,
                canvas,
                canvas_view,
                blit_pipeline,
                blit_bind_group,
                scale_factor,
                has_contents: false,
                batch_cache: None,
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
        state.blit_bind_group =
            create_blit_bind_group(&self.device, &state.blit_pipeline, &state.canvas_view);
        state.has_contents = false;
        state.batch_cache = None;
        Ok(())
    }

    pub fn detach_surface(&mut self, window: WindowId) {
        self.surfaces.remove(&window);
    }

    pub fn render_frame(
        &mut self,
        window: WindowId,
        display_list: &DisplayList,
    ) -> Result<(), RenderError> {
        self.render_frame_with_damage(window, display_list, None)
    }

    pub fn render_frame_with_damage(
        &mut self,
        window: WindowId,
        display_list: &DisplayList,
        damage: Option<Rect>,
    ) -> Result<(), RenderError> {
        display_list.validate()?;
        for command in display_list.commands() {
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
        let resolved_commands = resolve_commands(display_list);
        let batch_hash = display_list_hash(&resolved_commands);
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
            let mut batches = Vec::new();
            for command in &resolved_commands {
                if let PaintCommand::Clip { rect } = command {
                    batches.push(RenderBatch::Clip(Some(*rect)));
                    continue;
                }
                if let Some(damage) = damage.filter(|_| state.has_contents) {
                    if !command_intersects(command, damage) {
                        continue;
                    }
                }
                match command {
                    PaintCommand::Rect { rect, color } => {
                        append_rect(rect_batch(&mut batches), *rect, *color, render_size);
                    }
                    PaintCommand::RoundedRect {
                        rect,
                        radius,
                        color,
                    } => {
                        append_rounded_rect(
                            rounded_batch(&mut batches),
                            *rect,
                            *radius,
                            *color,
                            render_size,
                        );
                    }
                    PaintCommand::Line {
                        start,
                        end,
                        width,
                        color,
                    } => {
                        append_line(
                            line_batch(&mut batches),
                            *start,
                            *end,
                            *width,
                            *color,
                            render_size,
                        );
                    }
                    PaintCommand::Text {
                        text,
                        origin,
                        color,
                        scale,
                    } => {
                        append_text(
                            rect_batch(&mut batches),
                            self.fonts,
                            &mut self.glyph_cache,
                            &mut self.font_cache,
                            text,
                            *origin,
                            *color,
                            *scale,
                            render_size,
                            state.scale_factor.0 as f32,
                        );
                    }
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
                    } => {
                        append_image(
                            image_batch(&mut batches, *image),
                            *rect,
                            *opacity,
                            render_size,
                        );
                    }
                    // These commands are retained in the IR for validation
                    // and future scoped GPU state. Node-level state is
                    // resolved before flattening, so they do not draw by
                    // themselves. Image resources are handled by the image
                    // resource backend when it is attached.
                    PaintCommand::Clip { .. }
                    | PaintCommand::Transform(_)
                    | PaintCommand::Opacity(_) => {}
                    PaintCommand::Clear(_) => {}
                }
            }
            let cached_batches = state.batch_cache.take();
            let gpu_batches = if let Some((cached_hash, cached_batches)) = cached_batches {
                if cached_hash == batch_hash {
                    cached_batches
                } else {
                    build_gpu_batches(&self.device, batches)
                }
            } else {
                build_gpu_batches(&self.device, batches)
            };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("zui-render clear pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &state.canvas_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: if damage.is_some() && state.has_contents {
                            wgpu::LoadOp::Load
                        } else {
                            wgpu::LoadOp::Clear(clear)
                        },
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });
            let mut active_clip = None;
            for batch in &gpu_batches {
                if let GpuBatch::Clip(clip) = batch {
                    active_clip = *clip;
                    continue;
                }
                let GpuBatch::Draw(kind, vertex_buffer, count) = batch else {
                    continue;
                };
                let scissor = intersect_clip(active_clip, damage.filter(|_| state.has_contents));
                set_scissor(&mut pass, scissor, state.size, state.scale_factor);
                pass.set_pipeline(match kind {
                    BatchKind::Rect => &state.pipeline,
                    BatchKind::Rounded => &state.rounded_pipeline,
                    BatchKind::Line => &state.line_pipeline,
                    BatchKind::Image(_) => &state.image_pipeline,
                });
                pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                if let BatchKind::Image(image) = *kind {
                    if let Some(gpu_image) = self.gpu_images.get(&image) {
                        if !self.image_bind_groups.contains_key(&(window, image)) {
                            let bind_group =
                                self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                                    label: Some("zui-render image bind group"),
                                    layout: &state.image_pipeline.get_bind_group_layout(0),
                                    entries: &[
                                        wgpu::BindGroupEntry {
                                            binding: 0,
                                            resource: wgpu::BindingResource::TextureView(
                                                &gpu_image.view,
                                            ),
                                        },
                                        wgpu::BindGroupEntry {
                                            binding: 1,
                                            resource: wgpu::BindingResource::Sampler(
                                                &self.image_sampler,
                                            ),
                                        },
                                    ],
                                });
                            self.image_bind_groups.insert((window, image), bind_group);
                        }
                        if let Some(bind_group) = self.image_bind_groups.get(&(window, image)) {
                            pass.set_bind_group(0, bind_group, &[]);
                            pass.draw(0..*count, 0..1);
                        }
                    }
                } else {
                    pass.draw(0..*count, 0..1);
                }
            }
            state.batch_cache = Some((batch_hash, gpu_batches));
        }
        {
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
            pass.draw(0..3, 0..1);
        }
        state.has_contents = true;
        self.queue.submit(std::iter::once(encoder.finish()));
        frame.present();
        Ok(())
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }
}

fn build_gpu_batches(device: &wgpu::Device, batches: Vec<RenderBatch>) -> Vec<GpuBatch> {
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
            )),
            RenderBatch::Rounded(vertices) if !vertices.is_empty() => Some(GpuBatch::Draw(
                BatchKind::Rounded,
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("zui-render rounded rectangles"),
                    contents: bytemuck::cast_slice(&vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                }),
                vertices.len() as u32,
            )),
            RenderBatch::Line(vertices) if !vertices.is_empty() => Some(GpuBatch::Draw(
                BatchKind::Line,
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("zui-render lines"),
                    contents: bytemuck::cast_slice(&vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                }),
                vertices.len() as u32,
            )),
            RenderBatch::Image { image, vertices } if !vertices.is_empty() => Some(GpuBatch::Draw(
                BatchKind::Image(image),
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("zui-render images"),
                    contents: bytemuck::cast_slice(&vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                }),
                vertices.len() as u32,
            )),
            RenderBatch::Clip(clip) => Some(GpuBatch::Clip(clip)),
            _ => None,
        })
        .collect()
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

fn command_intersects(command: &PaintCommand, damage: Rect) -> bool {
    let bounds = match command {
        PaintCommand::Clear(_) => return true,
        PaintCommand::Rect { rect, .. } | PaintCommand::RoundedRect { rect, .. } => *rect,
        PaintCommand::Line {
            start, end, width, ..
        } => {
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
        PaintCommand::Text {
            text,
            origin,
            scale,
            ..
        } => Rect {
            origin: *origin,
            size: zui_core::Size {
                width: measure_text(text, *scale),
                height: Dip((*scale).max(1) as f32 * 7.0),
            },
        },
        PaintCommand::Icon { rect, .. } | PaintCommand::Image { rect, .. } => *rect,
        PaintCommand::Clip { rect } => *rect,
        PaintCommand::Transform(_) | PaintCommand::Opacity(_) => return true,
    };
    let bounds_right = bounds.origin.x.0 + bounds.size.width.0;
    let bounds_bottom = bounds.origin.y.0 + bounds.size.height.0;
    let damage_right = damage.origin.x.0 + damage.size.width.0;
    let damage_bottom = damage.origin.y.0 + damage.size.height.0;
    bounds.origin.x.0 < damage_right
        && bounds_right > damage.origin.x.0
        && bounds.origin.y.0 < damage_bottom
        && bounds_bottom > damage.origin.y.0
}

fn resolve_commands(display_list: &DisplayList) -> Vec<PaintCommand> {
    let mut resolved = Vec::with_capacity(display_list.commands().len());
    let mut transform = Transform::IDENTITY;
    let mut clip = None;
    let mut opacity = 1.0;
    for command in display_list.commands() {
        match command {
            PaintCommand::Transform(next) => transform = compose_transform(transform, *next),
            PaintCommand::Clip { rect } => {
                clip = intersect_clip(clip, Some(transform.rect(*rect)));
                if let Some(rect) = clip {
                    resolved.push(PaintCommand::Clip { rect });
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

fn display_list_hash(commands: &[PaintCommand]) -> u64 {
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
        depth_stencil: None,
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
        depth_stencil: None,
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
        depth_stencil: None,
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
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_list_records_clear_command() {
        let mut list = DisplayList::new();
        list.clear(Color::BLACK);
        assert_eq!(list.commands(), &[PaintCommand::Clear(Color::BLACK)]);
    }

    #[test]
    fn display_list_clear_starts_a_new_frame() {
        let mut list = DisplayList::new();
        list.fill_rect(
            Rect {
                origin: Point {
                    x: Dip(2.0),
                    y: Dip(3.0),
                },
                size: zui_core::Size {
                    width: Dip(4.0),
                    height: Dip(5.0),
                },
            },
            Color::WHITE,
        );
        list.clear(Color::BLACK);
        list.text("new frame", Point::default(), Color::WHITE, 1);

        assert_eq!(
            list.commands(),
            &[
                PaintCommand::Clear(Color::BLACK),
                PaintCommand::Text {
                    text: "new frame".into(),
                    origin: Point::default(),
                    color: Color::WHITE,
                    scale: 1,
                },
            ]
        );
    }

    #[test]
    fn noop_device_can_create_gpu_objects() {
        let (device, _queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let _encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        let _pipeline = create_rounded_rect_pipeline(&device, wgpu::TextureFormat::Rgba8Unorm);
        let _line_pipeline = create_line_pipeline(&device, wgpu::TextureFormat::Rgba8Unorm);
        let _image_pipeline = create_image_pipeline(&device, wgpu::TextureFormat::Rgba8Unorm);
    }

    #[test]
    fn render_node_flattens_commands_before_children() {
        let mut node = RenderNode::new(Rect::default());
        node.commands.fill_rect(Rect::default(), Color::WHITE);
        let mut child = RenderNode::new(Rect::default());
        child.commands.clear(Color::BLACK);
        node.add_child(child);
        let mut list = DisplayList::new();
        node.flatten_into(&mut list);
        assert_eq!(list.commands().len(), 2);
        assert!(matches!(list.commands()[0], PaintCommand::Rect { .. }));
        assert!(matches!(list.commands()[1], PaintCommand::Clear(_)));
    }

    #[test]
    fn render_node_applies_transform_and_opacity() {
        let mut node = RenderNode::new(Rect::default());
        node.set_transform(Transform::translate(Dip(10.0), Dip(20.0)));
        node.set_opacity(0.5);
        node.commands.fill_rect(
            Rect {
                origin: Point::default(),
                size: zui_core::Size {
                    width: Dip(4.0),
                    height: Dip(5.0),
                },
            },
            Color::WHITE,
        );
        let mut list = DisplayList::new();
        node.flatten_into(&mut list);
        assert_eq!(
            list.commands(),
            &[PaintCommand::Rect {
                rect: Rect {
                    origin: Point {
                        x: Dip(10.0),
                        y: Dip(20.0)
                    },
                    size: zui_core::Size {
                        width: Dip(4.0),
                        height: Dip(5.0)
                    },
                },
                color: Color {
                    r: 1.0,
                    g: 1.0,
                    b: 1.0,
                    a: 0.5
                },
            }]
        );
    }

    #[test]
    fn render_node_clips_commands_and_propagates_dirty_state() {
        let mut node = RenderNode::new(Rect::default());
        node.set_clip(Some(Rect {
            origin: Point::default(),
            size: zui_core::Size {
                width: Dip(5.0),
                height: Dip(5.0),
            },
        }));
        node.commands.fill_rect(
            Rect {
                origin: Point {
                    x: Dip(20.0),
                    y: Dip(20.0),
                },
                size: zui_core::Size {
                    width: Dip(2.0),
                    height: Dip(2.0),
                },
            },
            Color::WHITE,
        );
        assert!(node.dirty_flags.contains(DirtyFlags::PAINT));
        let mut list = DisplayList::new();
        node.flatten_into(&mut list);
        assert!(list.commands().is_empty());

        node.clear_dirty();
        assert!(!node.dirty);
        assert!(node.dirty_flags.is_empty());
        node.mark_dirty_region(Rect::default());
        assert!(node.dirty);
        assert_eq!(node.dirty_region, Some(Rect::default()));
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
        let mut list = DisplayList::new();
        list.clip(Rect {
            origin: Point {
                x: Dip(1.0),
                y: Dip(2.0),
            },
            size: zui_core::Size {
                width: Dip(10.0),
                height: Dip(11.0),
            },
        });
        list.fill_rect(
            Rect {
                origin: Point {
                    x: Dip(2.0),
                    y: Dip(3.0),
                },
                size: zui_core::Size {
                    width: Dip(4.0),
                    height: Dip(5.0),
                },
            },
            Color::WHITE,
        );
        let resolved = resolve_commands(&list);
        assert!(matches!(resolved.first(), Some(PaintCommand::Clip { .. })));
        assert!(matches!(resolved.get(1), Some(PaintCommand::Rect { .. })));
    }
}
