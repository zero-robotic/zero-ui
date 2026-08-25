//! GPU rendering owned by the application composition root.
//!
//! This crate deliberately exposes no `wgpu::Device` to widgets. Widgets produce
//! drawing data; `Renderer` owns the GPU and the per-window swap-chain surfaces.

use std::collections::HashMap;
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
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AdapterUnavailable => write!(f, "no compatible GPU adapter available"),
            Self::Device(message) => write!(f, "GPU device error: {message}"),
            Self::Surface(message) => write!(f, "surface error: {message}"),
            Self::SurfaceNotAttached(id) => write!(f, "surface {id:?} is not attached"),
            Self::SurfaceLost => write!(f, "surface was lost or outdated"),
        }
    }
}

impl std::error::Error for RenderError {}

#[derive(Clone, Debug, PartialEq)]
pub enum PaintCommand {
    Clear(Color),
    FillRect {
        rect: Rect,
        color: Color,
    },
    FillRoundedRect {
        rect: Rect,
        radius: Dip,
        color: Color,
    },
    Text {
        text: String,
        origin: Point,
        color: Color,
        scale: u32,
    },
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
        self.commands.push(PaintCommand::FillRect { rect, color });
    }
    pub fn fill_rounded_rect(&mut self, rect: Rect, radius: Dip, color: Color) {
        self.commands.push(PaintCommand::FillRoundedRect {
            rect,
            radius,
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
    pub fn commands(&self) -> &[PaintCommand] {
        &self.commands
    }
}

struct SurfaceState {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    size: PhysicalSize,
    pipeline: wgpu::RenderPipeline,
    canvas: wgpu::Texture,
    canvas_view: wgpu::TextureView,
    blit_pipeline: wgpu::RenderPipeline,
    blit_bind_group: wgpu::BindGroup,
    scale_factor: ScaleFactor,
    has_contents: bool,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct RectVertex {
    position: [f32; 2],
    color: [f32; 4],
}

pub struct Renderer {
    pub(crate) instance: wgpu::Instance,
    pub(crate) adapter: wgpu::Adapter,
    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
    surfaces: HashMap<WindowId, SurfaceState>,
    fonts: &'static [fontdue::Font],
    glyph_cache: HashMap<(usize, char, u32), CachedGlyph>,
    font_cache: HashMap<char, Option<usize>>,
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
        Ok(Self {
            instance,
            adapter,
            device,
            queue,
            surfaces: HashMap::new(),
            fonts: cached_system_fonts(),
            glyph_cache: HashMap::new(),
            font_cache: HashMap::new(),
        })
    }

    pub fn new_blocking() -> Result<Self, RenderError> {
        pollster::block_on(Self::new())
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
                canvas,
                canvas_view,
                blit_pipeline,
                blit_bind_group,
                scale_factor,
                has_contents: false,
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
                canvas,
                canvas_view,
                blit_pipeline,
                blit_bind_group,
                scale_factor,
                has_contents: false,
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
        let clear = display_list
            .commands()
            .iter()
            .rev()
            .find_map(|command| match command {
                PaintCommand::Clear(color) => Some(wgpu::Color {
                    r: color.r as f64,
                    g: color.g as f64,
                    b: color.b as f64,
                    a: color.a as f64,
                }),
                PaintCommand::FillRect { .. } => None,
                PaintCommand::FillRoundedRect { .. } => None,
                PaintCommand::Text { .. } => None,
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
            let mut rects = Vec::new();
            for command in display_list.commands() {
                if let Some(damage) = damage.filter(|_| state.has_contents) {
                    if !command_intersects(command, damage) {
                        continue;
                    }
                }
                match command {
                    PaintCommand::FillRect { rect, color } => {
                        append_rect(&mut rects, *rect, *color, render_size);
                    }
                    PaintCommand::FillRoundedRect {
                        rect,
                        radius,
                        color,
                    } => {
                        append_rounded_rect(&mut rects, *rect, *radius, *color, render_size);
                    }
                    PaintCommand::Text {
                        text,
                        origin,
                        color,
                        scale,
                    } => {
                        append_text(
                            &mut rects,
                            self.fonts,
                            &mut self.glyph_cache,
                            &mut self.font_cache,
                            text,
                            *origin,
                            *color,
                            *scale,
                            render_size,
                        );
                    }
                    PaintCommand::Clear(_) => {}
                }
            }
            let vertex_buffer = if rects.is_empty() {
                None
            } else {
                Some(
                    self.device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("zui-render rectangles"),
                            contents: bytemuck::cast_slice(&rects),
                            usage: wgpu::BufferUsages::VERTEX,
                        }),
                )
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
            if let Some(damage) = damage.filter(|_| state.has_contents) {
                let scale = state.scale_factor.0 as f32;
                let x = (damage.origin.x.0 * scale).max(0.0) as u32;
                let y = (damage.origin.y.0 * scale).max(0.0) as u32;
                let right = ((damage.origin.x.0 + damage.size.width.0) * scale)
                    .min(state.size.width as f32)
                    .max(x as f32) as u32;
                let bottom = ((damage.origin.y.0 + damage.size.height.0) * scale)
                    .min(state.size.height as f32)
                    .max(y as f32) as u32;
                pass.set_scissor_rect(x, y, right.saturating_sub(x), bottom.saturating_sub(y));
            }
            if let Some(vertex_buffer) = vertex_buffer {
                pass.set_pipeline(&state.pipeline);
                pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                pass.draw(0..rects.len() as u32, 0..1);
            }
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

fn cached_system_fonts() -> &'static [fontdue::Font] {
    SYSTEM_FONTS.get_or_init(load_system_fonts).as_slice()
}

fn load_system_fonts() -> Vec<fontdue::Font> {
    let candidates = [
        std::env::var("ZUI_FONT_PATH").ok(),
        std::env::var("ZUI_LATIN_FONT_PATH").ok(),
        Some("/System/Library/Fonts/Supplemental/Verdana.ttf".into()),
        Some("/System/Library/Fonts/Supplemental/Tahoma.ttf".into()),
        Some("/System/Library/Fonts/Supplemental/Arial.ttf".into()),
        Some("/System/Library/Fonts/Supplemental/Arial Unicode.ttf".into()),
        Some("/System/Library/Fonts/SFNS.ttf".into()),
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
    glyph_cache: &mut HashMap<(usize, char, u32), CachedGlyph>,
    font_cache: &mut HashMap<char, Option<usize>>,
    text: &str,
    origin: Point,
    color: Color,
    scale: u32,
    size: PhysicalSize,
) {
    if !fonts.is_empty() {
        let font_size = (scale.max(1) * 7) as f32;
        let baseline = origin.y.0 + font_size * 0.8;
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
                .entry((font_id, character, scale.max(1)))
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
                            x: Dip(x + pixel.x as f32),
                            y: Dip(top + pixel.y as f32),
                        },
                        size: zui_core::Size {
                            width: Dip(1.0),
                            height: Dip(1.0),
                        },
                    },
                    Color {
                        a: color.a * pixel.alpha,
                        ..color
                    },
                    size,
                );
            }
            x += metrics.advance_width;
        }
        return;
    }

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
        x += 6.0 * scale as f32;
    }
}

fn command_intersects(command: &PaintCommand, damage: Rect) -> bool {
    let bounds = match command {
        PaintCommand::Clear(_) => return true,
        PaintCommand::FillRect { rect, .. } | PaintCommand::FillRoundedRect { rect, .. } => *rect,
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

fn append_rounded_rect(
    vertices: &mut Vec<RectVertex>,
    rect: Rect,
    radius: Dip,
    color: Color,
    size: PhysicalSize,
) {
    let radius = radius
        .0
        .min(rect.size.width.0 / 2.0)
        .min(rect.size.height.0 / 2.0);
    if radius <= 0.0 {
        append_rect(vertices, rect, color, size);
        return;
    }

    let center = Point {
        x: Dip(rect.origin.x.0 + rect.size.width.0 / 2.0),
        y: Dip(rect.origin.y.0 + rect.size.height.0 / 2.0),
    };
    let corners = [
        (
            rect.origin.x.0 + radius,
            rect.origin.y.0 + radius,
            std::f32::consts::PI,
        ),
        (
            rect.origin.x.0 + rect.size.width.0 - radius,
            rect.origin.y.0 + radius,
            1.5 * std::f32::consts::PI,
        ),
        (
            rect.origin.x.0 + rect.size.width.0 - radius,
            rect.origin.y.0 + rect.size.height.0 - radius,
            0.0,
        ),
        (
            rect.origin.x.0 + radius,
            rect.origin.y.0 + rect.size.height.0 - radius,
            0.5 * std::f32::consts::PI,
        ),
    ];
    let mut points = Vec::with_capacity(20);
    for (cx, cy, start) in corners {
        for step in 0..=4 {
            let angle = start + step as f32 * std::f32::consts::FRAC_PI_2 / 4.0;
            points.push(Point {
                x: Dip(cx + radius * angle.cos()),
                y: Dip(cy + radius * angle.sin()),
            });
        }
    }
    let color = [color.r, color.g, color.b, color.a];
    for pair in points.windows(2) {
        append_triangle(vertices, center, pair[0], pair[1], color, size);
    }
    append_triangle(
        vertices,
        center,
        *points.last().unwrap(),
        points[0],
        color,
        size,
    );
}

fn append_triangle(
    vertices: &mut Vec<RectVertex>,
    a: Point,
    b: Point,
    c: Point,
    color: [f32; 4],
    size: PhysicalSize,
) {
    let to_vertex = |point: Point| RectVertex {
        position: [
            point.x.0 / size.width.max(1) as f32 * 2.0 - 1.0,
            1.0 - point.y.0 / size.height.max(1) as f32 * 2.0,
        ],
        color,
    };
    vertices.extend([to_vertex(a), to_vertex(b), to_vertex(c)]);
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
        list.fill_rect(Rect::default(), Color::WHITE);
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
    }
}
