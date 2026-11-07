//! Image decoding from bytes, with an optional bounded background worker.
//! No file, UI or renderer dependencies.
use std::{fmt, io::Cursor};
use zui_paint::{ImageError, ImageLimits, ImageResource};
use zui_types::Extent;

#[derive(Clone, Copy, Debug)]
pub struct DecodeLimits {
    pub image: ImageLimits,
    pub max_encoded_bytes: usize,
    /// PNG decoder's internal allocation budget; excludes our output buffers.
    pub max_decoder_bytes: usize,
}
impl Default for DecodeLimits {
    fn default() -> Self {
        Self {
            image: ImageLimits::default(),
            max_encoded_bytes: 16 * 1024 * 1024,
            max_decoder_bytes: 64 * 1024 * 1024,
        }
    }
}
#[derive(Debug)]
pub enum DecodeError {
    EncodedTooLarge,
    DecoderLimit,
    AnimatedPng,
    InvalidPng(String),
    Image(ImageError),
}
impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EncodedTooLarge => f.write_str("encoded image exceeds byte limit"),
            Self::DecoderLimit => f.write_str("PNG decoder allocation limit exceeded"),
            Self::AnimatedPng => f.write_str("animated PNG is not supported"),
            Self::InvalidPng(message) => write!(f, "invalid PNG: {message}"),
            Self::Image(error) => write!(f, "decoded image: {error}"),
        }
    }
}
impl std::error::Error for DecodeError {}
impl From<ImageError> for DecodeError {
    fn from(error: ImageError) -> Self {
        Self::Image(error)
    }
}
fn png_error(error: png::DecodingError) -> DecodeError {
    if matches!(error, png::DecodingError::LimitsExceeded) {
        DecodeError::DecoderLimit
    } else {
        DecodeError::InvalidPng(error.to_string())
    }
}

/// Decode a static PNG to the shared premultiplied RGBA8 resource.
/// Palette/grayscale/tRNS are expanded; 16-bit samples retain their high byte.
/// No ICC/gamma/orientation transform is performed; encoded color values are
/// interpreted as sRGB. APNG is rejected rather than silently choosing a frame.
pub fn decode_png(bytes: &[u8], limits: DecodeLimits) -> Result<ImageResource, DecodeError> {
    if bytes.len() > limits.max_encoded_bytes {
        return Err(DecodeError::EncodedTooLarge);
    }
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_limits(png::Limits {
        bytes: limits.max_decoder_bytes,
    });
    decoder.set_ignore_text_chunk(true);
    decoder.set_ignore_iccp_chunk(true);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(png_error)?;
    let info = reader.info();
    if info.animation_control.is_some() {
        return Err(DecodeError::AnimatedPng);
    }
    let extent = Extent::new(info.width, info.height);
    if extent.width == 0 || extent.height == 0 {
        return Err(ImageError::InvalidExtent.into());
    }
    let rgba_len = extent.rgba_len().ok_or(ImageError::TooLarge)?;
    if extent.width > limits.image.max_dimension
        || extent.height > limits.image.max_dimension
        || rgba_len > limits.image.max_bytes
    {
        return Err(ImageError::TooLarge.into());
    }
    let output_len = reader.output_buffer_size().ok_or(ImageError::TooLarge)?;
    // Normalization above produces at most four 8-bit channels.
    if output_len > rgba_len {
        return Err(ImageError::TooLarge.into());
    }
    let mut decoded = vec![0; output_len];
    let output = reader.next_frame(&mut decoded).map_err(png_error)?;
    reader.finish().map_err(png_error)?;
    if output.bit_depth != png::BitDepth::Eight
        || output.width != extent.width
        || output.height != extent.height
    {
        return Err(DecodeError::InvalidPng(
            "unexpected normalized frame format".into(),
        ));
    }
    decoded.truncate(output.buffer_size());
    let rgba = if output.color_type == png::ColorType::Rgba {
        decoded
    } else {
        let mut rgba = Vec::with_capacity(rgba_len);
        match output.color_type {
            png::ColorType::Rgb => {
                for rgb in decoded.as_chunks::<3>().0 {
                    rgba.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
                }
            }
            png::ColorType::Grayscale => {
                for gray in decoded {
                    rgba.extend_from_slice(&[gray, gray, gray, 255]);
                }
            }
            png::ColorType::GrayscaleAlpha => {
                for ga in decoded.as_chunks::<2>().0 {
                    rgba.extend_from_slice(&[ga[0], ga[0], ga[0], ga[1]]);
                }
            }
            _ => {
                return Err(DecodeError::InvalidPng(
                    "unexpected normalized color type".into(),
                ));
            }
        }
        rgba
    };
    Ok(ImageResource::from_rgba8(extent, rgba, limits.image)?)
}

#[cfg(feature = "worker")]
mod worker;
#[cfg(feature = "worker")]
pub use worker::{
    DecodeCompletion, DecodeRequestId, DecodeWorkerClosed, PngDecodeWorker, SubmitError,
};
