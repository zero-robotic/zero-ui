//! Backend-independent values shared across ZUI layers.
#![no_std]

mod color;
mod geometry;

pub use color::Color;
pub use geometry::{Affine, Extent, Point, Rect, Size};

/// Opaque identity allocated by a node owner. Reused storage must get a new ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub u64);
