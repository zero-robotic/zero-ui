use crate::RenderError;
use alloc::{sync::Arc, vec::Vec};
use core::{
    fmt,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT_FONT_ID: AtomicUsize = AtomicUsize::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FontId(usize);

struct FontData {
    id: FontId,
    bytes: Arc<[u8]>,
    index: u32,
}

/// Immutable, shared font bytes. Scenes keep the resource alive independently
/// of the text service. IDs never reuse an earlier resource identity.
#[derive(Clone)]
pub struct FontResource(Arc<FontData>);
impl FontResource {
    /// Format/face validity is checked by the text service or rendering backend.
    pub fn new(bytes: Arc<[u8]>, index: u32) -> Result<Self, RenderError> {
        if bytes.is_empty() {
            return Err(RenderError::InvalidFont);
        }
        let id = NEXT_FONT_ID
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map_err(|_| RenderError::InvalidFont)?;
        Ok(Self(Arc::new(FontData {
            id: FontId(id),
            bytes,
            index,
        })))
    }
    pub fn id(&self) -> FontId {
        self.0.id
    }
    pub fn index(&self) -> u32 {
        self.0.index
    }
    pub fn bytes(&self) -> &[u8] {
        &self.0.bytes
    }
}
impl AsRef<[u8]> for FontResource {
    fn as_ref(&self) -> &[u8] {
        self.bytes()
    }
}
impl PartialEq for FontResource {
    fn eq(&self, other: &Self) -> bool {
        self.id() == other.id()
    }
}
impl Eq for FontResource {}
impl fmt::Debug for FontResource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FontResource")
            .field("id", &self.id())
            .field("index", &self.index())
            .field("byte_len", &self.bytes().len())
            .finish()
    }
}

/// Positioned glyph in logical coordinates; y is its baseline, increasing downwards.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glyph {
    pub id: u32,
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GlyphRun {
    pub font: FontResource,
    pub font_size: f32,
    pub glyphs: Vec<Glyph>,
    /// OpenType normalized variation coordinates, signed 2.14 fixed point.
    pub normalized_coords: Vec<i16>,
}
