//! Logical layout contracts. No platform, renderer or layout-library types escape here.
use alloc::{sync::Arc, vec::Vec};
use core::fmt;
use zui_types::{Rect, Size};

pub use zui_types::NodeId;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Dimension {
    #[default]
    Auto,
    Length(f32),
    /// Fraction of the containing block, e.g. 0.5 means 50%.
    Percent(f32),
    /// Intrinsic size with all available soft wrap opportunities taken.
    /// Supported for width/height/basis, not min/max bounds.
    MinContent,
    /// Intrinsic size without soft wrapping. Supported for width/height/basis.
    MaxContent,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Edges {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}
impl Edges {
    pub const fn all(value: f32) -> Self {
        Self {
            left: value,
            right: value,
            top: value,
            bottom: value,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LayoutKind {
    /// Intrinsic content supplied by `LayoutTree::measure`; may not have children.
    #[default]
    Leaf,
    Row,
    Column,
    Grid,
    /// All children share one content area; later children paint above earlier ones.
    Stack,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    Start,
    End,
    Center,
    #[default]
    Stretch,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Justify {
    #[default]
    Start,
    End,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

/// Layout overflow policy. Painting/hit clipping is supplied independently by
/// the view or widget viewport; no renderer behavior is implied here.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Overflow {
    #[default]
    Visible,
    /// Contains descendant overflow without changing automatic minimum size.
    Clip,
    /// Contains overflow and permits shrinking below the intrinsic content size.
    Scroll,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Track {
    Auto,
    Length(f32),
    Percent(f32),
    /// Flexible fraction, with an automatic intrinsic minimum.
    Fraction(f32),
}

/// Border-box sizes in logical units; percentages follow Flex/Grid containing-block rules.
/// A percentage with an indefinite basis is resolved by the algorithm, never by screen size.
#[derive(Clone, Debug, PartialEq)]
pub struct LayoutStyle {
    pub kind: LayoutKind,
    pub width: Dimension,
    pub height: Dimension,
    pub min_width: Dimension,
    pub min_height: Dimension,
    pub max_width: Dimension,
    pub max_height: Dimension,
    pub padding: Edges,
    /// Signed logical margins; auto margins are a later extension.
    pub margin: Edges,
    pub gap: Size,
    pub grow: f32,
    pub shrink: f32,
    pub basis: Dimension,
    pub wrap: bool,
    pub align: Align,
    /// Horizontal item alignment for Grid/Stack containers.
    pub justify_items: Align,
    /// Overrides parent cross-axis alignment (vertical in Grid/Stack).
    pub align_self: Option<Align>,
    /// Overrides parent horizontal item alignment in Grid/Stack.
    pub justify_self: Option<Align>,
    pub justify: Justify,
    pub columns: Vec<Track>,
    pub rows: Vec<Track>,
    pub overflow_x: Overflow,
    pub overflow_y: Overflow,
}

impl Default for LayoutStyle {
    fn default() -> Self {
        Self {
            kind: LayoutKind::Leaf,
            width: Dimension::Auto,
            height: Dimension::Auto,
            min_width: Dimension::Auto,
            min_height: Dimension::Auto,
            max_width: Dimension::Auto,
            max_height: Dimension::Auto,
            padding: Edges::default(),
            margin: Edges::default(),
            gap: Size::ZERO,
            grow: 0.0,
            shrink: 1.0,
            basis: Dimension::Auto,
            wrap: false,
            align: Align::Stretch,
            justify_items: Align::Stretch,
            align_self: None,
            justify_self: None,
            justify: Justify::Start,
            columns: Vec::new(),
            rows: Vec::new(),
            overflow_x: Overflow::Visible,
            overflow_y: Overflow::Visible,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AvailableSpace {
    Definite(f32),
    MinContent,
    MaxContent,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeasureInput {
    /// Content dimensions; layout adapters clamp exhausted/negative content space to zero.
    pub known_width: Option<f32>,
    pub known_height: Option<f32>,
    pub available_width: AvailableSpace,
    pub available_height: AvailableSpace,
}

/// Unrounded border box relative to the parent; root position is viewport-relative.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LayoutBox {
    pub bounds: Rect,
    /// Reachable content extent measured from the local border-box origin,
    /// including padding; at least bounds.size. Negative/start-side overflow is
    /// not reachable. Presentation translations do not affect this layout value.
    pub scroll_extent: Size,
}
impl LayoutBox {
    /// A box with no known scrollable overflow, useful for simple layout engines.
    pub fn new(bounds: Rect) -> Self {
        Self {
            scroll_extent: bounds.size,
            bounds,
        }
    }
}

pub struct LayoutNode<'a> {
    pub style: &'a LayoutStyle,
    pub children: &'a [NodeId],
}

/// Opaque identity for unchanged intrinsic measurement inputs. Keep a clone while
/// inputs are unchanged; replace it on content/resource changes. Identity, not
/// numeric node IDs, isolates trees and prevents reuse after an owner is dropped.
#[derive(Clone, Debug)]
pub struct LayoutCacheKey(Arc<()>);
impl Default for LayoutCacheKey {
    fn default() -> Self {
        Self(Arc::new(()))
    }
}
impl PartialEq for LayoutCacheKey {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for LayoutCacheKey {}

/// The owner guarantees stable topology/styles during a compute call.
/// Measurement can update private text/image caches, but cannot mutate the tree.
pub trait LayoutTree {
    /// Opt in to cross-call measurement caching. None conservatively disables
    /// reuse for this node. Styles, child order and viewport are checked by the
    /// engine separately. A key must never be shared by distinct node instances.
    fn cache_key(&self, _id: NodeId) -> Option<LayoutCacheKey> {
        None
    }
    /// Optional preferred layout box ratio. Concrete controls remain opaque.
    fn preferred_aspect_ratio(&self, _id: NodeId) -> Option<f32> {
        None
    }
    fn node(&self, id: NodeId) -> Option<LayoutNode<'_>>;
    fn measure(&mut self, id: NodeId, input: MeasureInput) -> Result<Size, LayoutError>;
    /// Called only after the entire result has been validated. Must not change topology.
    fn set_layout(&mut self, id: NodeId, layout: LayoutBox);
}

pub trait LayoutEngine {
    /// The viewport supplies available space; root style controls its own border-box size.
    fn compute(
        &mut self,
        tree: &mut dyn LayoutTree,
        root: NodeId,
        viewport: Size,
    ) -> Result<(), LayoutError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LayoutError {
    MissingNode(NodeId),
    RepeatedNode(NodeId),
    InvalidStyle(NodeId),
    InvalidMeasurement(NodeId),
    InvalidResult(NodeId),
    InvalidViewport,
    BudgetExceeded,
    MeasurementFailed(NodeId),
    Text(NodeId, crate::text::TextError),
}
impl fmt::Display for LayoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "layout error: {self:?}")
    }
}
impl core::error::Error for LayoutError {}
