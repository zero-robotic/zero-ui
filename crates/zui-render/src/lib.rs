//! GPU rendering owned by the application composition root.
//!
//! This crate deliberately exposes no `wgpu::Device` to widgets. Widgets produce
//! drawing data; `Renderer` owns the GPU and the per-window swap-chain surfaces.

use std::collections::HashMap;
use std::sync::OnceLock;

use wgpu::util::DeviceExt;
use zui_core::{Color, Dip, PhysicalSize, Point, Rect, ScaleFactor, WindowId};
use zui_platform::spi::RawWindowHandleProvider;

pub use wgpu;

static SYSTEM_FONT: OnceLock<Option<fontdue::Font>> = OnceLock::new();

/// Measures text using the same system font used by the renderer.
pub fn measure_text(text: &str, scale: u32) -> Dip {
    if let Some(font) = cached_system_font() {
        let size = (scale.max(1) * 7) as f32;
        Dip(text
            .chars()
            .map(|character| font.metrics(character, size).advance_width)
            .sum())
    } else {
        Dip(text.chars().count() as f32 * 6.0 * scale.max(1) as f32)
    }
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
    font: Option<&'static fontdue::Font>,
    glyph_cache: HashMap<(char, u32), CachedGlyph>,
}

struct CachedGlyph {
    metrics: fontdue::Metrics,
    bitmap: Vec<u8>,
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
            font: cached_system_font(),
            glyph_cache: HashMap::new(),
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
        self.surfaces.insert(
            window,
            SurfaceState {
                surface,
                config,
                size,
            },
        );
        let _ = scale_factor;
        Ok(())
    }

    /// Attach a native window directly. This is the preferred path for the
    /// initial winit backend and avoids leaking wgpu handles into platform API.
    pub fn attach_native_surface<W: wgpu::rwh::HasDisplayHandle + wgpu::rwh::HasWindowHandle>(
        &mut self,
        window: WindowId,
        host: &W,
        size: PhysicalSize,
        _scale_factor: ScaleFactor,
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
        self.surfaces.insert(
            window,
            SurfaceState {
                surface,
                config,
                size,
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
        let state = self
            .surfaces
            .get(&window)
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
            let mut rects = Vec::new();
            for command in display_list.commands() {
                match command {
                    PaintCommand::FillRect { rect, color } => {
                        append_rect(&mut rects, *rect, *color, state.size);
                    }
                    PaintCommand::FillRoundedRect {
                        rect,
                        radius,
                        color,
                    } => {
                        append_rounded_rect(&mut rects, *rect, *radius, *color, state.size);
                    }
                    PaintCommand::Text {
                        text,
                        origin,
                        color,
                        scale,
                    } => {
                        append_text(
                            &mut rects,
                            &self.font,
                            &mut self.glyph_cache,
                            text,
                            *origin,
                            *color,
                            *scale,
                            state.size,
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
            let pipeline = create_rect_pipeline(&self.device, state.config.format);
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("zui-render clear pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });
            if let Some(vertex_buffer) = vertex_buffer {
                pass.set_pipeline(&pipeline);
                pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                pass.draw(0..rects.len() as u32, 0..1);
            }
        }
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

fn cached_system_font() -> Option<&'static fontdue::Font> {
    SYSTEM_FONT.get_or_init(|| load_system_font()).as_ref()
}

fn load_system_font() -> Option<fontdue::Font> {
    let candidates = [
        std::env::var("ZUI_FONT_PATH").ok(),
        Some("/System/Library/Fonts/Supplemental/Arial Unicode.ttf".into()),
        Some("/System/Library/Fonts/Supplemental/NISC18030.ttf".into()),
        Some("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc".into()),
        Some("/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc".into()),
        Some("C:\\Windows\\Fonts\\msyh.ttc".into()),
    ];
    candidates
        .into_iter()
        .flatten()
        .find_map(|path| std::fs::read(path).ok())
        .and_then(|bytes| fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default()).ok())
}

fn append_text(
    vertices: &mut Vec<RectVertex>,
    font: &Option<&fontdue::Font>,
    glyph_cache: &mut HashMap<(char, u32), CachedGlyph>,
    text: &str,
    origin: Point,
    color: Color,
    scale: u32,
    size: PhysicalSize,
) {
    if let Some(font) = font {
        let font_size = (scale.max(1) * 7) as f32;
        let mut x = origin.x.0;
        for character in text.chars() {
            let glyph = glyph_cache
                .entry((character, scale.max(1)))
                .or_insert_with(|| {
                    let (metrics, bitmap) = font.rasterize(character, font_size);
                    CachedGlyph { metrics, bitmap }
                });
            let metrics = glyph.metrics;
            let top = origin.y.0 + (font_size - metrics.height as f32) + metrics.ymin as f32;
            for row in 0..metrics.height {
                for column in 0..metrics.width {
                    let alpha = glyph.bitmap[row * metrics.width + column] as f32 / 255.0;
                    if alpha > 0.01 {
                        append_rect(
                            vertices,
                            Rect {
                                origin: Point {
                                    x: Dip(x + column as f32),
                                    y: Dip(top + row as f32),
                                },
                                size: zui_core::Size {
                                    width: Dip(1.0),
                                    height: Dip(1.0),
                                },
                            },
                            Color {
                                a: color.a * alpha,
                                ..color
                            },
                            size,
                        );
                    }
                }
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
    fn noop_device_can_create_gpu_objects() {
        let (device, _queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let _encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    }
}
