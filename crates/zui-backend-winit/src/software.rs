use crate::backend;
use std::{num::NonZeroU32, sync::Arc};
use zui_paint::PixelBuffer;
use zui_platform::{NativeWindow, PixelPresenter, PlatformError};
use zui_types::Extent;

/// Only presents pixels; it has no knowledge of CPU renderers or widget trees.
pub struct SoftwareSurface {
    surface: softbuffer::Surface<Arc<dyn NativeWindow>, Arc<dyn NativeWindow>>,
    extent: Option<Extent>,
}

impl SoftwareSurface {
    pub fn new(window: Arc<dyn NativeWindow>) -> Result<Self, PlatformError> {
        let context = softbuffer::Context::new(window.clone()).map_err(backend)?;
        let surface = softbuffer::Surface::new(&context, window).map_err(backend)?;
        Ok(Self {
            surface,
            extent: None,
        })
    }
}

impl PixelPresenter for SoftwareSurface {
    fn present(&mut self, pixels: &PixelBuffer) -> Result<(), PlatformError> {
        let extent = pixels.extent();
        if self.extent != Some(extent) {
            let width = NonZeroU32::new(extent.width)
                .ok_or(PlatformError::Unsupported("zero-width display"))?;
            let height = NonZeroU32::new(extent.height)
                .ok_or(PlatformError::Unsupported("zero-height display"))?;
            self.surface.resize(width, height).map_err(backend)?;
            self.extent = Some(extent);
        }
        let mut buffer = self.surface.buffer_mut().map_err(backend)?;
        pack_xrgb(pixels.pixels(), &mut buffer);
        buffer.present().map_err(backend)
    }
}

/// Softbuffer accepts 0x00RRGGBB. Premultiplied pixels composite over black.
fn pack_xrgb(source: &[u8], destination: &mut [u32]) {
    for (pixel, output) in source.as_chunks::<4>().0.iter().zip(destination) {
        *output = u32::from(pixel[0]) << 16 | u32::from(pixel[1]) << 8 | u32::from(pixel[2]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn color_channels_match_softbuffer_format() {
        let mut pixels = [0; 2];
        pack_xrgb(&[255, 128, 64, 255, 128, 0, 0, 128], &mut pixels);
        assert_eq!(pixels, [0x00ff8040, 0x00800000]);
    }
}
