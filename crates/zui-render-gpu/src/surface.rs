use crate::{GpuRenderer, backend};
use wgpu::{CurrentSurfaceTexture, SurfaceConfiguration, TextureFormat};
use zui_paint::{Frame, RenderError, RenderLimits, Scene};
use zui_types::Extent;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentStatus {
    Presented,
    /// Retry on a later event-loop tick; do not busy-loop on acquisition.
    Retry,
    Occluded,
}

/// Native surface renderer independent of winit. The owner keeps the native
/// handle alive and this adapter owns its compatible GPU device and swapchain.
pub struct GpuSurface<W: wgpu::DisplayAndWindowHandle + Clone + 'static> {
    surface: wgpu::Surface<'static>,
    renderer: GpuRenderer,
    instance: wgpu::Instance,
    window: W,
    config: SurfaceConfiguration,
    configured: Option<Extent>,
}

impl<W: wgpu::DisplayAndWindowHandle + Clone + 'static> GpuSurface<W> {
    pub async fn new(window: W, limits: RenderLimits) -> Result<Self, RenderError> {
        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window.clone()).map_err(backend)?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await
            .map_err(backend)?;
        let capabilities = surface.get_capabilities(&adapter);
        let format = select_format(&capabilities.formats).ok_or_else(|| {
            RenderError::Backend("surface has no supported 8-bit color format".into())
        })?;
        let view_format = format.remove_srgb_suffix();
        let mut config = surface.get_default_config(&adapter, 1, 1).ok_or_else(|| {
            RenderError::Backend("GPU is incompatible with the native surface".into())
        })?;
        config.format = format;
        // Vello writes sRGB-encoded channel values. Rendering to a linear view
        // avoids an additional sRGB encoding by the attachment hardware.
        config.view_formats = if format != view_format {
            vec![view_format]
        } else {
            vec![]
        };
        config.present_mode = wgpu::PresentMode::Fifo;
        config.desired_maximum_frame_latency = 2;
        if capabilities
            .alpha_modes
            .contains(&wgpu::CompositeAlphaMode::Opaque)
        {
            config.alpha_mode = wgpu::CompositeAlphaMode::Opaque;
        }
        let renderer = GpuRenderer::from_adapter(&adapter, view_format, limits).await?;
        Ok(Self {
            surface,
            renderer,
            instance,
            window,
            config,
            configured: None,
        })
    }

    pub fn adapter_name(&self) -> &str {
        self.renderer.adapter_name()
    }

    pub fn present(&mut self, scene: &Scene, frame: Frame) -> Result<PresentStatus, RenderError> {
        // Validate before configuring/allocating GPU surfaces, including device limits.
        zui_vello::dimensions(frame, self.renderer.limits)?;
        scene.validate(self.renderer.limits)?;
        let maximum = self.renderer.device.limits().max_texture_dimension_2d;
        if frame.extent.width > maximum || frame.extent.height > maximum {
            return Err(RenderError::TargetTooLarge);
        }
        if self.configured != Some(frame.extent) {
            self.configure(frame.extent);
        }
        let (texture, reconfigure) = match self.surface.get_current_texture() {
            CurrentSurfaceTexture::Success(texture) => (texture, false),
            CurrentSurfaceTexture::Suboptimal(texture) => (texture, true),
            CurrentSurfaceTexture::Timeout => return Ok(PresentStatus::Retry),
            CurrentSurfaceTexture::Occluded => return Ok(PresentStatus::Occluded),
            CurrentSurfaceTexture::Outdated => {
                self.configured = None;
                return Ok(PresentStatus::Retry);
            }
            CurrentSurfaceTexture::Lost => {
                self.surface = self
                    .instance
                    .create_surface(self.window.clone())
                    .map_err(backend)?;
                self.configured = None;
                return Ok(PresentStatus::Retry);
            }
            CurrentSurfaceTexture::Validation => {
                return Err(RenderError::Backend(
                    "surface acquisition validation failed".into(),
                ));
            }
        };
        let view = texture.texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(self.renderer.target_format),
            ..Default::default()
        });
        self.renderer.render_to_view(scene, frame, &view)?;
        self.renderer.queue.present(texture);
        if reconfigure {
            self.configured = None;
        }
        Ok(PresentStatus::Presented)
    }

    fn configure(&mut self, extent: Extent) {
        self.config.width = extent.width;
        self.config.height = extent.height;
        self.surface.configure(&self.renderer.device, &self.config);
        self.configured = Some(extent);
    }
}

fn select_format(formats: &[TextureFormat]) -> Option<TextureFormat> {
    [
        TextureFormat::Bgra8Unorm,
        TextureFormat::Rgba8Unorm,
        TextureFormat::Bgra8UnormSrgb,
        TextureFormat::Rgba8UnormSrgb,
    ]
    .into_iter()
    .find(|format| formats.contains(format))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn surface_format_prefers_encoded_byte_targets() {
        assert_eq!(
            select_format(&[TextureFormat::Rgba8UnormSrgb, TextureFormat::Bgra8Unorm]),
            Some(TextureFormat::Bgra8Unorm)
        );
        assert_eq!(
            select_format(&[TextureFormat::Bgra8UnormSrgb]),
            Some(TextureFormat::Bgra8UnormSrgb)
        );
        assert_eq!(select_format(&[TextureFormat::Rgba16Float]), None);
    }
}
