//! Vello GPU adapter for ZUI's portable drawing protocol.
mod images;
mod readback;
mod surface;

pub use surface::{GpuSurface, PresentStatus};

use zui_paint::{Frame, OffscreenRenderer, PixelBuffer, RenderError, RenderLimits, Scene};
use zui_vello::{
    Canvas,
    kurbo::{Affine, BezPath, Stroke},
    peniko::Color,
};

/// Owns a GPU device for offscreen work. A separate surface integration can use
/// `render_to_view` without a CPU readback on each frame.
pub struct GpuRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: vello_gpu::Renderer,
    resources: vello_gpu::Resources,
    fonts: zui_vello::FontCache,
    images: images::Images,
    scene: vello_gpu::Scene,
    limits: RenderLimits,
    adapter_name: String,
    target_format: wgpu::TextureFormat,
    depth: Option<(u16, u16, wgpu::TextureView)>,
}

impl GpuRenderer {
    pub async fn new(limits: RenderLimits) -> Result<Self, RenderError> {
        let instance = wgpu::Instance::default();
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .map_err(backend)?;
        Self::from_adapter(&adapter, wgpu::TextureFormat::Rgba8Unorm, limits).await
    }

    async fn from_adapter(
        adapter: &wgpu::Adapter,
        target_format: wgpu::TextureFormat,
        limits: RenderLimits,
    ) -> Result<Self, RenderError> {
        let adapter_name = adapter.get_info().name;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .map_err(backend)?;
        let (renderer, resources) = vello_gpu::Renderer::new(
            &device,
            &vello_gpu::RenderTargetConfig {
                format: target_format,
                width: 1,
                height: 1,
            },
        );
        Ok(Self {
            device,
            queue,
            renderer,
            resources,
            fonts: zui_vello::FontCache::default(),
            images: images::Images::default(),
            scene: vello_gpu::Scene::new(1, 1),
            limits,
            adapter_name,
            target_format,
            depth: None,
        })
    }

    pub fn adapter_name(&self) -> &str {
        &self.adapter_name
    }

    /// Adapter-level interop only. Framework paint contracts never expose wgpu.
    /// The view must use this device, the renderer's target format, matching extent, one sample,
    /// and RENDER_ATTACHMENT usage. Callers retain surface ownership.
    pub fn render_to_view(
        &mut self,
        scene: &Scene,
        frame: Frame,
        view: &wgpu::TextureView,
    ) -> Result<(), RenderError> {
        let (width, height) = self.prepare(scene, frame)?;
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        self.encode(&mut encoder, view, width, height)?;
        self.queue.submit([encoder.finish()]);
        Ok(())
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn target_format(&self) -> wgpu::TextureFormat {
        self.target_format
    }

    fn prepare(&mut self, scene: &Scene, frame: Frame) -> Result<(u16, u16), RenderError> {
        let (width, height) = zui_vello::dimensions(frame, self.limits)?;
        scene.validate(self.limits)?;
        let maximum = self.device.limits().max_texture_dimension_2d;
        if frame.extent.width > maximum || frame.extent.height > maximum {
            return Err(RenderError::TargetTooLarge);
        }
        self.images.prepare(scene, &self.device, &self.queue)?;
        self.scene.reset_and_resize(width, height);
        zui_vello::replay(
            scene,
            frame,
            &mut self.fonts,
            &mut GpuCanvas(&mut self.scene, &mut self.resources, &self.images),
        )?;
        Ok((width, height))
    }

    fn encode(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        width: u16,
        height: u16,
    ) -> Result<(), RenderError> {
        let size = vello_gpu::RenderSize { width, height };
        if !self
            .depth
            .as_ref()
            .is_some_and(|(w, h, _)| *w == width && *h == height)
        {
            self.depth = Some((
                width,
                height,
                vello_gpu::Renderer::create_depth_texture_view(&self.device, &size),
            ));
        }
        self.renderer
            .render(
                &self.scene,
                &mut self.resources,
                &self.device,
                &self.queue,
                encoder,
                &size,
                view,
                self.depth.as_ref().map(|(_, _, view)| view),
                &self.images.bindings,
                vello_gpu::TargetInit::Clear(vello_gpu::ClearSettings::default()),
            )
            .map_err(backend)
    }
}

impl OffscreenRenderer for GpuRenderer {
    fn render(&mut self, scene: &Scene, frame: Frame) -> Result<PixelBuffer, RenderError> {
        let (width, height) = self.prepare(scene, frame)?;
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ZUI offscreen target"),
            size: wgpu::Extent3d {
                width: frame.extent.width,
                height: frame.extent.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.target_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        self.encode(&mut encoder, &view, width, height)?;
        let mut pixels = readback::copy(
            &self.device,
            &self.queue,
            encoder,
            &texture,
            frame.extent,
            self.limits,
        )?;
        if self.target_format == wgpu::TextureFormat::Bgra8Unorm {
            for pixel in pixels.pixels_mut().as_chunks_mut::<4>().0 {
                pixel.swap(0, 2);
            }
        }
        Ok(pixels)
    }
}

fn backend(error: impl std::fmt::Display) -> RenderError {
    RenderError::Backend(error.to_string())
}

struct GpuCanvas<'a>(
    &'a mut vello_gpu::Scene,
    &'a mut vello_gpu::Resources,
    &'a images::Images,
);
impl Canvas for GpuCanvas<'_> {
    fn image(
        &mut self,
        image: &zui_paint::ImageResource,
        sampling: zui_paint::ImageSampling,
    ) -> Result<(), RenderError> {
        use zui_vello::kurbo::Shape;
        self.0.set_paint(zui_vello::paint::Image {
            image: self.2.source(image)?,
            sampler: zui_vello::image_sampler(sampling),
        });
        let e = image.extent();
        self.0.fill_path(
            &zui_vello::kurbo::Rect::new(0.0, 0.0, e.width as f64, e.height as f64).to_path(0.1),
        );
        Ok(())
    }
    fn glyphs(
        &mut self,
        run: &zui_paint::GlyphRun,
        font: &zui_vello::peniko::FontData,
        color: Color,
    ) -> Result<(), RenderError> {
        self.0.set_paint(color);
        self.0
            .glyph_run(self.1, font)
            .font_size(run.font_size)
            // Grid-fit small text at the renderer’s device scale.
            .hint(true)
            .normalized_coords(zui_vello::coords(run))
            .fill_glyphs(zui_vello::glyphs(run))
            .map_err(backend)
    }
    fn transform(&mut self, transform: Affine) {
        self.0.set_transform(transform);
    }
    fn fill(&mut self, path: &BezPath, color: Color) {
        self.0.set_paint(color);
        self.0.fill_path(path);
    }
    fn stroke(&mut self, path: &BezPath, color: Color, stroke: Stroke) {
        self.0.set_stroke(stroke);
        self.0.set_paint(color);
        self.0.stroke_path(path);
    }
    fn push_clip(&mut self, path: &BezPath) {
        self.0.push_clip_path(path);
    }
    fn pop_clip(&mut self) {
        self.0.pop_clip();
    }
}
