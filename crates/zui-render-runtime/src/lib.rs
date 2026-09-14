//! Startup-time renderer selection.
//!
//! `ActiveRenderer` is intentionally an enum, not a trait object. Selecting a
//! backend happens once at startup (or after device loss); each concrete
//! backend keeps static dispatch in its own frame hot path.

use std::collections::HashMap;

use zui_core::{Color, WindowId};
use zui_platform::spi::RawWindowHandleProvider;
use zui_platform::Host;
use zui_render::{
    ImageId, ImageResource, RenderError, RenderNode, RenderNodeIndex, Renderer, SceneUpdate,
    SurfaceMetrics,
};
use zui_render_cpu::CpuRenderer;
use zui_render_software_gpu::SoftwareGpuRenderer;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RendererKind {
    HardwareGpu,
    SoftwareGpu,
    Cpu,
    Headless,
}

#[derive(Debug)]
pub enum RendererError {
    HardwareGpu(RenderError),
    NoImplementedFallback,
    Backend(String),
}

impl std::fmt::Display for RendererError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::HardwareGpu(error) => {
                write!(formatter, "hardware GPU initialization failed: {error}")
            }
            Self::NoImplementedFallback => formatter.write_str(
                "no renderer is available: software GPU and CPU backends are placeholders",
            ),
            Self::Backend(message) => write!(formatter, "renderer backend error: {message}"),
        }
    }
}

impl std::error::Error for RendererError {}

/// Renderer lifecycle required by `zui-app` for one platform host type.
/// Native GPU renderers attach to a window surface; deterministic/headless
/// renderers can attach an in-memory target while preserving the same calls.
pub trait ApplicationRenderer<H: Host> {
    fn register_image(&mut self, id: ImageId, image: ImageResource);
    fn attach_surface(
        &mut self,
        window: WindowId,
        host: &H,
        metrics: SurfaceMetrics,
    ) -> Result<(), RendererError>;
    fn resize(&mut self, window: WindowId, metrics: SurfaceMetrics) -> Result<(), RendererError>;
    fn detach_surface(&mut self, window: WindowId);
    fn render_scene(
        &mut self,
        window: WindowId,
        node: &RenderNode,
        clear: Color,
        index: &RenderNodeIndex,
        update: &SceneUpdate,
    ) -> Result<(), RendererError>;
}

/// One deterministic frame submitted through the same application renderer
/// contract as a native GPU frame. It retains the exact scene revision used by
/// the renderer, making end-to-end headless tests inspectable without a window.
#[derive(Clone)]
pub struct HeadlessFrame {
    pub window: WindowId,
    pub metrics: SurfaceMetrics,
    pub clear: Color,
    pub node: RenderNode,
    pub index: RenderNodeIndex,
    pub update: SceneUpdate,
}

#[derive(Default)]
pub struct HeadlessRenderer {
    surfaces: HashMap<WindowId, SurfaceMetrics>,
    images: HashMap<ImageId, ImageResource>,
    frames: Vec<HeadlessFrame>,
}

impl HeadlessRenderer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn frames(&self) -> &[HeadlessFrame] {
        &self.frames
    }

    pub fn last_frame(&self) -> Option<&HeadlessFrame> {
        self.frames.last()
    }

    pub fn image_count(&self) -> usize {
        self.images.len()
    }

    pub fn kind(&self) -> RendererKind {
        RendererKind::Headless
    }
}

impl<H: Host> ApplicationRenderer<H> for HeadlessRenderer {
    fn register_image(&mut self, id: ImageId, image: ImageResource) {
        self.images.insert(id, image);
    }

    fn attach_surface(
        &mut self,
        window: WindowId,
        _host: &H,
        metrics: SurfaceMetrics,
    ) -> Result<(), RendererError> {
        self.surfaces.insert(window, metrics);
        Ok(())
    }

    fn resize(&mut self, window: WindowId, metrics: SurfaceMetrics) -> Result<(), RendererError> {
        let surface = self
            .surfaces
            .get_mut(&window)
            .ok_or_else(|| RendererError::Backend(format!("surface {window:?} is not attached")))?;
        *surface = metrics;
        Ok(())
    }

    fn detach_surface(&mut self, window: WindowId) {
        self.surfaces.remove(&window);
    }

    fn render_scene(
        &mut self,
        window: WindowId,
        node: &RenderNode,
        clear: Color,
        index: &RenderNodeIndex,
        update: &SceneUpdate,
    ) -> Result<(), RendererError> {
        let metrics =
            self.surfaces.get(&window).copied().ok_or_else(|| {
                RendererError::Backend(format!("surface {window:?} is not attached"))
            })?;
        self.frames.push(HeadlessFrame {
            window,
            metrics,
            clear,
            node: node.clone(),
            index: index.clone(),
            update: update.clone(),
        });
        Ok(())
    }
}

/// Shared frame-level contract for all renderer implementations. It is never
/// stored behind `dyn RenderBackend`; `ActiveRenderer` keeps concrete variants
/// so the selected backend retains static dispatch in its hot path.
pub trait RenderBackend {
    fn kind(&self) -> RendererKind;

    fn render_scene(
        &mut self,
        window: WindowId,
        node: &RenderNode,
        clear: Color,
        index: &RenderNodeIndex,
        update: &SceneUpdate,
    ) -> Result<(), RendererError>;
}

impl RenderBackend for Renderer {
    fn kind(&self) -> RendererKind {
        RendererKind::HardwareGpu
    }

    fn render_scene(
        &mut self,
        window: WindowId,
        node: &RenderNode,
        clear: Color,
        index: &RenderNodeIndex,
        update: &SceneUpdate,
    ) -> Result<(), RendererError> {
        self.render_scene(window, node, clear, index, update)
            .map_err(RendererError::HardwareGpu)
    }
}

impl RenderBackend for SoftwareGpuRenderer {
    fn kind(&self) -> RendererKind {
        RendererKind::SoftwareGpu
    }

    fn render_scene(
        &mut self,
        _: WindowId,
        _: &RenderNode,
        _: Color,
        _: &RenderNodeIndex,
        _: &SceneUpdate,
    ) -> Result<(), RendererError> {
        Err(RendererError::NoImplementedFallback)
    }
}

impl RenderBackend for CpuRenderer {
    fn kind(&self) -> RendererKind {
        RendererKind::Cpu
    }

    fn render_scene(
        &mut self,
        _: WindowId,
        _: &RenderNode,
        _: Color,
        _: &RenderNodeIndex,
        _: &SceneUpdate,
    ) -> Result<(), RendererError> {
        Err(RendererError::NoImplementedFallback)
    }
}

pub enum ActiveRenderer {
    HardwareGpu(Renderer),
    SoftwareGpu(SoftwareGpuRenderer),
    Cpu(CpuRenderer),
}

impl ActiveRenderer {
    pub fn register_image(&mut self, id: zui_render::ImageId, image: zui_render::ImageResource) {
        if let Self::HardwareGpu(renderer) = self {
            renderer.register_image(id, image);
        }
    }
    /// Selects the first available backend. The latter two branches are
    /// intentionally unreachable until their packages gain implementations.
    pub fn new_blocking() -> Result<Self, RendererError> {
        match Renderer::new_blocking() {
            Ok(renderer) => Ok(Self::HardwareGpu(renderer)),
            Err(error) => {
                let _ = SoftwareGpuRenderer::new();
                let _ = CpuRenderer::new();
                Err(match error {
                    RenderError::AdapterUnavailable => RendererError::NoImplementedFallback,
                    error => RendererError::HardwareGpu(error),
                })
            }
        }
    }

    pub fn kind(&self) -> RendererKind {
        match self {
            Self::HardwareGpu(_) => RendererKind::HardwareGpu,
            Self::SoftwareGpu(_) => RendererKind::SoftwareGpu,
            Self::Cpu(_) => RendererKind::Cpu,
        }
    }

    pub fn attach_surface<H: RawWindowHandleProvider>(
        &mut self,
        window: WindowId,
        host: &H,
        metrics: SurfaceMetrics,
    ) -> Result<(), RendererError> {
        match self {
            Self::HardwareGpu(renderer) => renderer
                .attach_surface(window, host, metrics)
                .map_err(RendererError::HardwareGpu),
            Self::SoftwareGpu(_) | Self::Cpu(_) => Err(RendererError::NoImplementedFallback),
        }
    }

    pub fn resize(
        &mut self,
        window: WindowId,
        metrics: SurfaceMetrics,
    ) -> Result<(), RendererError> {
        match self {
            Self::HardwareGpu(renderer) => renderer
                .resize(window, metrics)
                .map_err(RendererError::HardwareGpu),
            Self::SoftwareGpu(_) | Self::Cpu(_) => Err(RendererError::NoImplementedFallback),
        }
    }

    pub fn detach_surface(&mut self, window: WindowId) {
        match self {
            Self::HardwareGpu(renderer) => renderer.detach_surface(window),
            Self::SoftwareGpu(_) | Self::Cpu(_) => {}
        }
    }

    pub fn render_scene(
        &mut self,
        window: WindowId,
        node: &RenderNode,
        clear: Color,
        index: &RenderNodeIndex,
        update: &SceneUpdate,
    ) -> Result<(), RendererError> {
        match self {
            Self::HardwareGpu(renderer) => {
                RenderBackend::render_scene(renderer, window, node, clear, index, update)
            }
            Self::SoftwareGpu(renderer) => {
                RenderBackend::render_scene(renderer, window, node, clear, index, update)
            }
            Self::Cpu(renderer) => {
                RenderBackend::render_scene(renderer, window, node, clear, index, update)
            }
        }
    }
}

impl<H> ApplicationRenderer<H> for ActiveRenderer
where
    H: Host + RawWindowHandleProvider,
{
    fn register_image(&mut self, id: ImageId, image: ImageResource) {
        ActiveRenderer::register_image(self, id, image);
    }

    fn attach_surface(
        &mut self,
        window: WindowId,
        host: &H,
        metrics: SurfaceMetrics,
    ) -> Result<(), RendererError> {
        ActiveRenderer::attach_surface(self, window, host, metrics)
    }

    fn resize(&mut self, window: WindowId, metrics: SurfaceMetrics) -> Result<(), RendererError> {
        ActiveRenderer::resize(self, window, metrics)
    }

    fn detach_surface(&mut self, window: WindowId) {
        ActiveRenderer::detach_surface(self, window)
    }

    fn render_scene(
        &mut self,
        window: WindowId,
        node: &RenderNode,
        clear: Color,
        index: &RenderNodeIndex,
        update: &SceneUpdate,
    ) -> Result<(), RendererError> {
        ActiveRenderer::render_scene(self, window, node, clear, index, update)
    }
}
