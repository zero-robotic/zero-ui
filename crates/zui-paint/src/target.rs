use crate::Scene;
use alloc::{string::String, vec, vec::Vec};
use core::fmt;
use zui_types::{Color, Extent};

/// Physical target plus logical-to-physical scale. No windowing types required.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub extent: Extent,
    pub scale: f64,
    pub background: Color,
}

impl Frame {
    pub fn validate(self, limits: RenderLimits) -> Result<(), RenderError> {
        if self.extent.width == 0
            || self.extent.height == 0
            || !self.scale.is_finite()
            || self.scale <= 0.0
        {
            return Err(RenderError::InvalidTarget);
        }
        let pixels = u64::from(self.extent.width) * u64::from(self.extent.height);
        if pixels > limits.max_pixels || self.extent.rgba_len().is_none() {
            return Err(RenderError::TargetTooLarge);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RenderLimits {
    pub max_pixels: u64,
    pub max_commands: usize,
    pub max_path_elements: usize,
    pub max_clip_depth: usize,
    pub max_glyphs: usize,
    pub max_font_bytes: usize,
    pub max_images: usize,
    pub max_image_bytes: usize,
}

impl Default for RenderLimits {
    fn default() -> Self {
        Self {
            max_pixels: 16_777_216,
            max_commands: 100_000,
            max_path_elements: 1_000_000,
            max_clip_depth: 64,
            max_glyphs: 100_000,
            max_font_bytes: 64 * 1024 * 1024,
            max_images: 1_024,
            max_image_bytes: 64 * 1024 * 1024,
        }
    }
}

#[derive(Debug)]
pub enum RenderError {
    InvalidTarget,
    TargetTooLarge,
    InvalidGeometry,
    SceneTooComplex,
    UnbalancedClip,
    InvalidFont,
    UnsupportedImage,
    Backend(String),
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTarget => f.write_str("invalid target size or scale"),
            Self::TargetTooLarge => f.write_str("target exceeds pixel budget"),
            Self::InvalidGeometry => f.write_str("invalid drawing geometry"),
            Self::SceneTooComplex => f.write_str("scene exceeds rendering budget"),
            Self::UnbalancedClip => f.write_str("unbalanced clip commands"),
            Self::UnsupportedImage => f.write_str("image drawing is unsupported by this backend"),
            Self::InvalidFont => f.write_str("invalid or unsupported font resource"),
            Self::Backend(message) => write!(f, "render backend: {message}"),
        }
    }
}
impl core::error::Error for RenderError {}

/// Tightly packed, premultiplied RGBA8 in sRGB encoding.
pub struct PixelBuffer {
    extent: Extent,
    pixels: Vec<u8>,
}

impl PixelBuffer {
    pub fn new(extent: Extent, limits: RenderLimits) -> Result<Self, RenderError> {
        Frame {
            extent,
            scale: 1.0,
            background: Color::TRANSPARENT,
        }
        .validate(limits)?;
        let len = extent.rgba_len().ok_or(RenderError::TargetTooLarge)?;
        Ok(Self {
            extent,
            pixels: vec![0; len],
        })
    }
    pub fn extent(&self) -> Extent {
        self.extent
    }
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }
    pub fn pixels_mut(&mut self) -> &mut [u8] {
        &mut self.pixels
    }
    pub fn stride(&self) -> usize {
        self.extent.width as usize * 4
    }
}

/// An offscreen renderer. Window presentation remains a separate platform service.
/// GPU implementations may read back for this interface; direct surface rendering
/// is provided separately and should be used by interactive applications.
pub trait OffscreenRenderer {
    fn render(&mut self, scene: &Scene, frame: Frame) -> Result<PixelBuffer, RenderError>;
}
