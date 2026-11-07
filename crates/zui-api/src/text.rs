//! Portable shaped text results shared by measurement and rendering.
use alloc::{string::String, sync::Arc, vec::Vec};
use core::{fmt, ops::Range};
use zui_paint::{GlyphRun, Painter};
use zui_types::{Color, Point, Rect, Size};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextAlign {
    #[default]
    Start,
    Center,
    End,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextStyle {
    /// Explicit registered family names, in fallback order.
    pub families: Vec<String>,
    pub font_size: f32,
    /// Line-height multiplier relative to the requested font size.
    pub line_height: f32,
    pub align: TextAlign,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TextWidth {
    Unbounded,
    /// Wrapping/alignment width; an indivisible cluster may still overflow.
    AtMost(f32),
    MinContent,
}
pub struct TextRequest<'a> {
    pub text: &'a str,
    pub style: &'a TextStyle,
    pub width: TextWidth,
}

#[derive(Clone, Debug)]
pub struct TextLine {
    /// UTF-8 byte range in the original text, not a glyph or character index.
    pub text_range: Range<usize>,
    pub baseline: f32,
    pub height: f32,
}

#[derive(Clone, Debug)]
pub struct TextLayout {
    pub size: Size,
    pub first_baseline: Option<f32>,
    pub lines: Vec<TextLine>,
    pub runs: Vec<GlyphRun>,
    pub font_epoch: u64,
}
impl TextLayout {
    /// Number of font placeholder glyphs, not the number of missing characters.
    /// Shaping can map several code points to one glyph or one cluster to several.
    pub fn missing_glyph_count(&self) -> usize {
        self.runs
            .iter()
            .flat_map(|run| &run.glyphs)
            .filter(|glyph| glyph.id == 0)
            .count()
    }
    /// Uses the already positioned glyphs; no independent shaping or line wrapping.
    pub fn paint(&self, painter: &mut Painter<'_>, color: Color) {
        for run in &self.runs {
            painter.glyphs(run.clone(), color);
        }
    }
}

/// Selects the preceding or following text side at a wrap/bidi boundary.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextAffinity {
    Upstream,
    #[default]
    Downstream,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextPosition {
    pub byte_index: usize,
    pub affinity: TextAffinity,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VisualDirection {
    Left,
    Right,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextGeometryError {
    InvalidPosition,
    InvalidPoint,
    InvalidCaretWidth,
    InvalidGeometry,
}
impl fmt::Display for TextGeometryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "text geometry error: {self:?}")
    }
}
impl core::error::Error for TextGeometryError {}

/// Immutable geometry of one exact text/layout snapshot. Coordinates are logical
/// and relative to its origin. Providers must return extended-grapheme boundaries.
pub trait TextGeometry: Send + Sync {
    fn cursor_at(&self, point: Point) -> Result<TextPosition, TextGeometryError>;
    fn caret(&self, position: TextPosition, width: f32) -> Result<Rect, TextGeometryError>;
    fn selection_rects(
        &self,
        anchor: TextPosition,
        focus: TextPosition,
    ) -> Result<Vec<Rect>, TextGeometryError>;
    fn move_visual(
        &self,
        position: TextPosition,
        direction: VisualDirection,
    ) -> Result<TextPosition, TextGeometryError>;
}
/// Interactive clients opt into retaining shaping data; ordinary labels need not.
#[derive(Clone)]
pub struct InteractiveTextLayout {
    pub layout: TextLayout,
    pub geometry: Arc<dyn TextGeometry>,
}
impl fmt::Debug for InteractiveTextLayout {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InteractiveTextLayout")
            .field("layout", &self.layout)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextError {
    Geometry(TextGeometryError),
    ServiceUnavailable,
    InteractionUnavailable,
    InvalidStyle,
    InvalidWidth,
    InvalidFont,
    NoFonts,
    UnknownFamily(String),
    MissingGlyph,
    BudgetExceeded,
    UnsupportedFontSynthesis,
}
impl fmt::Display for TextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "text error: {self:?}")
    }
}
impl core::error::Error for TextError {}

pub trait TextService {
    fn layout(&mut self, request: TextRequest<'_>) -> Result<TextLayout, TextError>;
    /// A changed epoch invalidates text measurement and paint caches owned by callers.
    fn font_epoch(&self) -> u64;
    fn layout_interactive(
        &mut self,
        _request: TextRequest<'_>,
    ) -> Result<InteractiveTextLayout, TextError> {
        Err(TextError::InteractionUnavailable)
    }
}
