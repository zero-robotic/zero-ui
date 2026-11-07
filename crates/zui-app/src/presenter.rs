use crate::Backend;
use std::sync::Arc;
use zui_paint::{Frame, RenderLimits, Scene};
use zui_platform::{NativeWindow, PlatformError};

pub(crate) enum Presentation {
    #[cfg(any(feature = "cpu", feature = "gpu"))]
    Presented,
    #[cfg(feature = "gpu")]
    Retry,
    #[cfg(feature = "gpu")]
    Occluded,
}

pub(crate) trait Presenter {
    fn present(&mut self, scene: &Scene, frame: Frame) -> Result<Presentation, PlatformError>;
}

pub(crate) fn create(
    backend: Backend,
    window: Arc<dyn NativeWindow>,
    limits: RenderLimits,
) -> Result<Box<dyn Presenter>, PlatformError> {
    match backend {
        Backend::Cpu => {
            #[cfg(feature = "cpu")]
            {
                Ok(Box::new(cpu::CpuPresenter::new(window, limits)?))
            }
            #[cfg(not(feature = "cpu"))]
            {
                let _ = (window, limits);
                Err(PlatformError::Unsupported(
                    "CPU backend was not enabled at build time",
                ))
            }
        }
        Backend::Gpu => {
            #[cfg(feature = "gpu")]
            {
                Ok(Box::new(gpu::GpuPresenter::new(window, limits)?))
            }
            #[cfg(not(feature = "gpu"))]
            {
                let _ = (window, limits);
                Err(PlatformError::Unsupported(
                    "GPU backend was not enabled at build time",
                ))
            }
        }
    }
}

#[cfg(feature = "cpu")]
mod cpu {
    use super::*;
    use zui_platform::PixelPresenter;

    pub(super) struct CpuPresenter {
        renderer: zui_render_cpu::RetainedCpuRenderer,
        surface: zui_backend_winit::SoftwareSurface,
    }

    impl CpuPresenter {
        pub fn new(
            window: Arc<dyn NativeWindow>,
            limits: RenderLimits,
        ) -> Result<Self, PlatformError> {
            Ok(Self {
                renderer: zui_render_cpu::RetainedCpuRenderer::new(limits),
                surface: zui_backend_winit::SoftwareSurface::new(window)?,
            })
        }
    }

    impl Presenter for CpuPresenter {
        fn present(&mut self, scene: &Scene, frame: Frame) -> Result<Presentation, PlatformError> {
            let pixels = self.renderer.render(scene, frame)?;
            self.surface.present(pixels)?;
            Ok(Presentation::Presented)
        }
    }
}

#[cfg(feature = "gpu")]
mod gpu {
    use super::*;
    use zui_render_gpu::{GpuSurface, PresentStatus};

    pub(super) struct GpuPresenter {
        surface: GpuSurface<Arc<dyn NativeWindow>>,
    }

    impl GpuPresenter {
        pub fn new(
            window: Arc<dyn NativeWindow>,
            limits: RenderLimits,
        ) -> Result<Self, PlatformError> {
            let surface = pollster::block_on(GpuSurface::new(window, limits))?;
            Ok(Self { surface })
        }
    }

    impl Presenter for GpuPresenter {
        fn present(&mut self, scene: &Scene, frame: Frame) -> Result<Presentation, PlatformError> {
            Ok(match self.surface.present(scene, frame)? {
                PresentStatus::Presented => Presentation::Presented,
                PresentStatus::Retry => Presentation::Retry,
                PresentStatus::Occluded => Presentation::Occluded,
            })
        }
    }
}
