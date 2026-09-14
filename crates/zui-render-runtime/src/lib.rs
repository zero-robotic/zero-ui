//! Startup-time renderer selection.
//!
//! `ActiveRenderer` is intentionally an enum, not a trait object. Selecting a
//! backend happens once at startup (or after device loss); each concrete
//! backend keeps static dispatch in its own frame hot path.

use zui_core::{Color, WindowId};
use zui_platform::spi::RawWindowHandleProvider;
use zui_render::{RenderError, RenderNode, RenderNodeIndex, Renderer, SceneUpdate, SurfaceMetrics};
use zui_render_cpu::CpuRenderer;
use zui_render_software_gpu::SoftwareGpuRenderer;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RendererKind {
    HardwareGpu,
    SoftwareGpu,
    Cpu,
}

#[derive(Debug)]
pub enum RendererError {
    HardwareGpu(RenderError),
    NoImplementedFallback,
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
        }
    }
}

impl std::error::Error for RendererError {}

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
