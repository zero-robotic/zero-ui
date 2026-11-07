//! Immutable decoded raster resources, independent of codecs and backends.
use alloc::{sync::Arc, vec::Vec};
use core::{
    fmt,
    sync::atomic::{AtomicUsize, Ordering},
};
use zui_types::Extent;

static NEXT_IMAGE_ID: AtomicUsize = AtomicUsize::new(1);
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ImageId(usize);

/// Per-resource limits, checked before conversion or resource allocation.
/// These do not bound the aggregate memory of all live images or GPU uploads.
#[derive(Clone, Copy, Debug)]
pub struct ImageLimits {
    pub max_dimension: u32,
    pub max_bytes: usize,
}
impl Default for ImageLimits {
    fn default() -> Self {
        Self {
            max_dimension: 16_384,
            max_bytes: 64 * 1024 * 1024,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageError {
    InvalidExtent,
    TooLarge,
    InvalidByteLength,
    /// Index of the first pixel whose RGB channels exceed its alpha.
    InvalidPremultipliedPixel(usize),
    IdentityExhausted,
}
impl fmt::Display for ImageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidExtent => f.write_str("image dimensions must be nonzero"),
            Self::TooLarge => f.write_str("image exceeds resource limits or addressable size"),
            Self::InvalidByteLength => f.write_str("image requires tightly packed RGBA8 bytes"),
            Self::InvalidPremultipliedPixel(index) => {
                write!(f, "invalid premultiplied image pixel {index}")
            }
            Self::IdentityExhausted => f.write_str("image resource identities exhausted"),
        }
    }
}
impl core::error::Error for ImageError {}
struct ImageData {
    id: ImageId,
    extent: Extent,
    bytes: Arc<[u8]>,
}

/// Tightly packed, top-to-bottom, premultiplied RGBA8 in sRGB encoding.
/// Cloning shares immutable pixels; separately created resources have distinct IDs.
#[derive(Clone)]
pub struct ImageResource(Arc<ImageData>);
impl ImageResource {
    pub fn from_premultiplied_rgba8(
        extent: Extent,
        bytes: Arc<[u8]>,
        limits: ImageLimits,
    ) -> Result<Self, ImageError> {
        validate_size(extent, bytes.len(), limits)?;
        for (index, pixel) in bytes.as_chunks::<4>().0.iter().enumerate() {
            if pixel[..3].iter().any(|channel| *channel > pixel[3]) {
                return Err(ImageError::InvalidPremultipliedPixel(index));
            }
        }
        Self::validated(extent, bytes)
    }
    /// Converts straight alpha in the supplied buffer before sharing it.
    /// Channels are multiplied in sRGB encoding, with rounding to nearest byte.
    pub fn from_rgba8(
        extent: Extent,
        mut bytes: Vec<u8>,
        limits: ImageLimits,
    ) -> Result<Self, ImageError> {
        validate_size(extent, bytes.len(), limits)?;
        for pixel in bytes.as_chunks_mut::<4>().0 {
            let alpha = u16::from(pixel[3]);
            for channel in &mut pixel[..3] {
                *channel = ((u16::from(*channel) * alpha + 127) / 255) as u8;
            }
        }
        Self::validated(extent, bytes.into())
    }
    fn validated(extent: Extent, bytes: Arc<[u8]>) -> Result<Self, ImageError> {
        let id = NEXT_IMAGE_ID
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map_err(|_| ImageError::IdentityExhausted)?;
        Ok(Self(Arc::new(ImageData {
            id: ImageId(id),
            extent,
            bytes,
        })))
    }
    pub fn id(&self) -> ImageId {
        self.0.id
    }
    pub fn extent(&self) -> Extent {
        self.0.extent
    }
    pub fn bytes(&self) -> &[u8] {
        &self.0.bytes
    }
    pub fn stride(&self) -> usize {
        self.extent().width as usize * 4
    }
}
fn validate_size(extent: Extent, len: usize, limits: ImageLimits) -> Result<(), ImageError> {
    if extent.width == 0 || extent.height == 0 {
        return Err(ImageError::InvalidExtent);
    }
    if extent.width > limits.max_dimension || extent.height > limits.max_dimension {
        return Err(ImageError::TooLarge);
    }
    let required = extent.rgba_len().ok_or(ImageError::TooLarge)?;
    if required > limits.max_bytes {
        return Err(ImageError::TooLarge);
    }
    if required != len {
        return Err(ImageError::InvalidByteLength);
    }
    Ok(())
}
impl AsRef<[u8]> for ImageResource {
    fn as_ref(&self) -> &[u8] {
        self.bytes()
    }
}
impl PartialEq for ImageResource {
    fn eq(&self, other: &Self) -> bool {
        self.id() == other.id()
    }
}
impl Eq for ImageResource {}
impl fmt::Debug for ImageResource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImageResource")
            .field("id", &self.id())
            .field("extent", &self.extent())
            .field("byte_len", &self.bytes().len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    #[test]
    fn straight_alpha_conversion_rounds_and_clears_transparent_rgb() {
        let image = ImageResource::from_rgba8(
            Extent::new(3, 1),
            vec![255, 127, 1, 128, 255, 255, 255, 0, 13, 55, 210, 255],
            ImageLimits::default(),
        )
        .unwrap();
        assert_eq!(
            image.bytes(),
            &[128, 64, 1, 128, 0, 0, 0, 0, 13, 55, 210, 255]
        );
        assert_eq!(image.stride(), 12);
        // Exhaust every channel/alpha combination, including rounding boundaries.
        let mut bytes = Vec::new();
        for alpha in 0..=255u8 {
            for channel in 0..=255u8 {
                bytes.extend_from_slice(&[channel, channel, channel, alpha]);
            }
        }
        let image = ImageResource::from_rgba8(Extent::new(256, 256), bytes, ImageLimits::default())
            .unwrap();
        for (index, pixel) in image.bytes().as_chunks::<4>().0.iter().enumerate() {
            let expected = (((index % 256) * (index / 256) + 127) / 255) as u8;
            assert_eq!(pixel, &[expected, expected, expected, (index / 256) as u8]);
        }
    }
    #[test]
    fn clones_share_pixels_and_identity_without_conflating_equal_images() {
        let bytes: Arc<[u8]> = Arc::from([1, 2, 3, 255]);
        let image = ImageResource::from_premultiplied_rgba8(
            Extent::new(1, 1),
            bytes.clone(),
            ImageLimits::default(),
        )
        .unwrap();
        assert_eq!(image.bytes().as_ptr(), bytes.as_ptr());
        let clone = image.clone();
        assert_eq!(clone, image);
        drop(image);
        assert_eq!(clone.bytes(), &*bytes);
        let other = ImageResource::from_premultiplied_rgba8(
            Extent::new(1, 1),
            bytes,
            ImageLimits::default(),
        )
        .unwrap();
        assert_ne!(clone, other);
    }
    #[test]
    fn limits_lengths_and_premultiplication_are_checked_before_publication() {
        let create = |extent, bytes: Vec<u8>, limits| {
            ImageResource::from_premultiplied_rgba8(extent, bytes.into(), limits)
        };
        let limits = ImageLimits {
            max_dimension: 2,
            max_bytes: 8,
        };
        assert_eq!(
            create(Extent::new(0, 1), vec![], limits),
            Err(ImageError::InvalidExtent)
        );
        assert_eq!(
            create(Extent::new(3, 1), vec![], limits),
            Err(ImageError::TooLarge)
        );
        assert_eq!(
            create(Extent::new(2, 2), vec![], limits),
            Err(ImageError::TooLarge)
        );
        for len in [0, 3, 5] {
            assert_eq!(
                create(Extent::new(1, 1), vec![0; len], limits),
                Err(ImageError::InvalidByteLength)
            );
        }
        assert_eq!(
            create(Extent::new(2, 1), vec![0, 0, 0, 0, 10, 0, 0, 9], limits),
            Err(ImageError::InvalidPremultipliedPixel(1))
        );
        assert!(create(Extent::new(2, 1), vec![0; 8], limits).is_ok());
        assert_eq!(
            create(
                Extent::new(u32::MAX, u32::MAX),
                vec![],
                ImageLimits {
                    max_dimension: u32::MAX,
                    max_bytes: usize::MAX
                }
            ),
            Err(ImageError::TooLarge)
        );
        assert_eq!(
            ImageResource::from_rgba8(Extent::new(1, 1), vec![0; 5], limits),
            Err(ImageError::InvalidByteLength)
        );
    }
}
