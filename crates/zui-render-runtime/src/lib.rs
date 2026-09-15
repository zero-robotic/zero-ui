//! Startup-time renderer selection.
//!
//! `ActiveRenderer` is intentionally an enum, not a trait object. Selecting a
//! backend happens once at startup (or after device loss); each concrete
//! backend keeps static dispatch in its own frame hot path.

use std::collections::HashMap;

use zui_core::{Color, WindowId};
use zui_platform::{SurfaceTarget, SurfaceTargetKind};
use zui_render::{
    FrameOutcome, ImageId, ImageResource, RenderError, RenderNode, RenderNodeIndex, Renderer,
    RendererOptions, SceneUpdate, SurfaceLifecycle, SurfaceMetrics, SurfacePhase,
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
pub trait ApplicationRenderer {
    fn register_image(&mut self, id: ImageId, image: ImageResource);
    fn attach_surface(
        &mut self,
        window: WindowId,
        target: SurfaceTarget,
        metrics: SurfaceMetrics,
    ) -> Result<(), RendererError>;
    fn resize(&mut self, window: WindowId, metrics: SurfaceMetrics) -> Result<(), RendererError>;
    fn recover_surface(
        &mut self,
        window: WindowId,
        metrics: SurfaceMetrics,
    ) -> Result<(), RendererError>;
    fn detach_surface(&mut self, window: WindowId);
    fn surface_phase(&self, window: WindowId) -> SurfacePhase;
    fn render_scene(
        &mut self,
        window: WindowId,
        node: &RenderNode,
        clear: Color,
        index: &RenderNodeIndex,
        update: &SceneUpdate,
    ) -> Result<FrameOutcome, RendererError>;
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
    surfaces: HashMap<WindowId, HeadlessSurface>,
    images: HashMap<ImageId, ImageResource>,
    frames: Vec<HeadlessFrame>,
}

struct HeadlessSurface {
    _target: SurfaceTarget,
    metrics: SurfaceMetrics,
    lifecycle: SurfaceLifecycle,
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

impl ApplicationRenderer for HeadlessRenderer {
    fn register_image(&mut self, id: ImageId, image: ImageResource) {
        self.images.insert(id, image);
    }

    fn attach_surface(
        &mut self,
        window: WindowId,
        target: SurfaceTarget,
        metrics: SurfaceMetrics,
    ) -> Result<(), RendererError> {
        if target.kind() != SurfaceTargetKind::Headless {
            return Err(RendererError::Backend(
                "headless renderer requires a headless surface target".into(),
            ));
        }
        if self.surfaces.contains_key(&window) {
            return Err(RendererError::Backend(format!(
                "surface {window:?} is already attached"
            )));
        }
        let mut lifecycle = SurfaceLifecycle::default();
        lifecycle
            .transition(SurfacePhase::Attached)
            .map_err(|error| RendererError::Backend(error.to_string()))?;
        self.surfaces.insert(
            window,
            HeadlessSurface {
                _target: target,
                metrics,
                lifecycle,
            },
        );
        Ok(())
    }

    fn resize(&mut self, window: WindowId, metrics: SurfaceMetrics) -> Result<(), RendererError> {
        let surface = self
            .surfaces
            .get_mut(&window)
            .ok_or_else(|| RendererError::Backend(format!("surface {window:?} is not attached")))?;
        surface.metrics = metrics;
        surface
            .lifecycle
            .transition(SurfacePhase::Resized)
            .map_err(|error| RendererError::Backend(error.to_string()))?;
        Ok(())
    }

    fn recover_surface(
        &mut self,
        window: WindowId,
        metrics: SurfaceMetrics,
    ) -> Result<(), RendererError> {
        let surface = self
            .surfaces
            .get_mut(&window)
            .ok_or_else(|| RendererError::Backend(format!("surface {window:?} is not attached")))?;
        surface.metrics = metrics;
        surface
            .lifecycle
            .transition(SurfacePhase::Recovered)
            .map_err(|error| RendererError::Backend(error.to_string()))
    }

    fn detach_surface(&mut self, window: WindowId) {
        if let Some(mut surface) = self.surfaces.remove(&window) {
            let _ = surface.lifecycle.transition(SurfacePhase::Detached);
        }
    }

    fn surface_phase(&self, window: WindowId) -> SurfacePhase {
        self.surfaces
            .get(&window)
            .map(|surface| surface.lifecycle.phase())
            .unwrap_or(SurfacePhase::Detached)
    }

    fn render_scene(
        &mut self,
        window: WindowId,
        node: &RenderNode,
        clear: Color,
        index: &RenderNodeIndex,
        update: &SceneUpdate,
    ) -> Result<FrameOutcome, RendererError> {
        let surface = self
            .surfaces
            .get_mut(&window)
            .ok_or_else(|| RendererError::Backend(format!("surface {window:?} is not attached")))?;
        if surface.metrics.physical_size.width == 0 || surface.metrics.physical_size.height == 0 {
            surface
                .lifecycle
                .transition(SurfacePhase::Deferred(
                    zui_render::FrameDeferReason::Occluded,
                ))
                .map_err(|error| RendererError::Backend(error.to_string()))?;
            return Ok(FrameOutcome::Deferred(
                zui_render::FrameDeferReason::Occluded,
            ));
        }
        surface
            .lifecycle
            .transition(SurfacePhase::Presented)
            .map_err(|error| RendererError::Backend(error.to_string()))?;
        self.frames.push(HeadlessFrame {
            window,
            metrics: surface.metrics,
            clear,
            node: node.clone(),
            index: index.clone(),
            update: update.clone(),
        });
        Ok(FrameOutcome::Presented)
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
    ) -> Result<FrameOutcome, RendererError>;
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
    ) -> Result<FrameOutcome, RendererError> {
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
    ) -> Result<FrameOutcome, RendererError> {
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
    ) -> Result<FrameOutcome, RendererError> {
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
        Self::new_blocking_with_options(RendererOptions::default())
    }

    pub fn new_blocking_with_options(options: RendererOptions) -> Result<Self, RendererError> {
        match Renderer::new_blocking_with_options(options) {
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

    pub fn attach_surface(
        &mut self,
        window: WindowId,
        target: SurfaceTarget,
        metrics: SurfaceMetrics,
    ) -> Result<(), RendererError> {
        match self {
            Self::HardwareGpu(renderer) => renderer
                .attach_surface(window, target, metrics)
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

    pub fn recover_surface(
        &mut self,
        window: WindowId,
        metrics: SurfaceMetrics,
    ) -> Result<(), RendererError> {
        match self {
            Self::HardwareGpu(renderer) => renderer
                .recover_surface(window, metrics)
                .map_err(RendererError::HardwareGpu),
            Self::SoftwareGpu(_) | Self::Cpu(_) => Err(RendererError::NoImplementedFallback),
        }
    }

    pub fn surface_phase(&self, window: WindowId) -> SurfacePhase {
        match self {
            Self::HardwareGpu(renderer) => renderer.surface_phase(window),
            Self::SoftwareGpu(_) | Self::Cpu(_) => SurfacePhase::Detached,
        }
    }

    pub fn render_scene(
        &mut self,
        window: WindowId,
        node: &RenderNode,
        clear: Color,
        index: &RenderNodeIndex,
        update: &SceneUpdate,
    ) -> Result<FrameOutcome, RendererError> {
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

impl ApplicationRenderer for ActiveRenderer {
    fn register_image(&mut self, id: ImageId, image: ImageResource) {
        ActiveRenderer::register_image(self, id, image);
    }

    fn attach_surface(
        &mut self,
        window: WindowId,
        target: SurfaceTarget,
        metrics: SurfaceMetrics,
    ) -> Result<(), RendererError> {
        ActiveRenderer::attach_surface(self, window, target, metrics)
    }

    fn resize(&mut self, window: WindowId, metrics: SurfaceMetrics) -> Result<(), RendererError> {
        ActiveRenderer::resize(self, window, metrics)
    }

    fn detach_surface(&mut self, window: WindowId) {
        ActiveRenderer::detach_surface(self, window)
    }

    fn recover_surface(
        &mut self,
        window: WindowId,
        metrics: SurfaceMetrics,
    ) -> Result<(), RendererError> {
        ActiveRenderer::recover_surface(self, window, metrics)
    }

    fn surface_phase(&self, window: WindowId) -> SurfacePhase {
        ActiveRenderer::surface_phase(self, window)
    }

    fn render_scene(
        &mut self,
        window: WindowId,
        node: &RenderNode,
        clear: Color,
        index: &RenderNodeIndex,
        update: &SceneUpdate,
    ) -> Result<FrameOutcome, RendererError> {
        ActiveRenderer::render_scene(self, window, node, clear, index, update)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zui_core::{Color, Id, PhysicalSize, Rect, ScaleFactor};

    fn metrics(width: u32, height: u32) -> SurfaceMetrics {
        SurfaceMetrics::new(PhysicalSize { width, height }, ScaleFactor::default(), 1.0)
    }

    #[test]
    fn headless_renderer_uses_the_complete_surface_state_machine() {
        let window = WindowId(Id::new(1));
        let mut renderer = HeadlessRenderer::new();
        assert_eq!(renderer.surface_phase(window), SurfacePhase::Detached);

        renderer
            .attach_surface(window, SurfaceTarget::headless(), metrics(320, 180))
            .unwrap();
        assert_eq!(renderer.surface_phase(window), SurfacePhase::Attached);

        renderer.resize(window, metrics(640, 360)).unwrap();
        assert_eq!(renderer.surface_phase(window), SurfacePhase::Resized);

        renderer.resize(window, metrics(0, 0)).unwrap();
        assert_eq!(renderer.surface_phase(window), SurfacePhase::Resized);
        let outcome = renderer
            .render_scene(
                window,
                &RenderNode::for_widget(Rect::default()),
                Color::default(),
                &RenderNodeIndex::default(),
                &SceneUpdate::default(),
            )
            .unwrap();
        assert_eq!(
            outcome,
            FrameOutcome::Deferred(zui_render::FrameDeferReason::Occluded)
        );

        renderer.resize(window, metrics(640, 360)).unwrap();
        assert_eq!(renderer.surface_phase(window), SurfacePhase::Resized);

        renderer
            .surfaces
            .get_mut(&window)
            .unwrap()
            .lifecycle
            .transition(SurfacePhase::Deferred(
                zui_render::FrameDeferReason::Timeout,
            ))
            .unwrap();
        assert_eq!(
            renderer.surface_phase(window),
            SurfacePhase::Deferred(zui_render::FrameDeferReason::Timeout)
        );

        renderer
            .surfaces
            .get_mut(&window)
            .unwrap()
            .lifecycle
            .transition(SurfacePhase::Lost)
            .unwrap();
        renderer.recover_surface(window, metrics(640, 360)).unwrap();
        assert_eq!(renderer.surface_phase(window), SurfacePhase::Recovered);

        renderer
            .render_scene(
                window,
                &RenderNode::for_widget(Rect::default()),
                Color::default(),
                &RenderNodeIndex::default(),
                &SceneUpdate::default(),
            )
            .unwrap();
        assert_eq!(renderer.surface_phase(window), SurfacePhase::Presented);

        renderer.detach_surface(window);
        assert_eq!(renderer.surface_phase(window), SurfacePhase::Detached);
    }
}
