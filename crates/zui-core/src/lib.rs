//! Platform-independent primitives shared by the toolkit.

use std::fmt;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const BLACK: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };
    pub const WHITE: Self = Self {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Dip(pub f32);

impl Dip {
    pub const ZERO: Self = Self(0.0);
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: Dip,
    pub y: Dip,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size {
    pub width: Dip,
    pub height: Dip,
}

impl Size {
    pub const ZERO: Self = Self {
        width: Dip::ZERO,
        height: Dip::ZERO,
    };
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub origin: Point,
    pub size: Size,
}

impl Rect {
    pub fn contains(self, point: Point) -> bool {
        point.x.0 >= self.origin.x.0
            && point.y.0 >= self.origin.y.0
            && point.x.0 <= self.origin.x.0 + self.size.width.0
            && point.y.0 <= self.origin.y.0 + self.size.height.0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PhysicalSize {
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScaleFactor(pub f64);

impl Default for ScaleFactor {
    fn default() -> Self {
        Self(1.0)
    }
}

impl ScaleFactor {
    pub fn to_physical(self, size: Size) -> PhysicalSize {
        PhysicalSize {
            width: (size.width.0 as f64 * self.0).round().max(0.0) as u32,
            height: (size.height.0 as f64 * self.0).round().max(0.0) as u32,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Id(u64);

impl Id {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }
    pub const fn value(self) -> u64 {
        self.0
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WindowId(pub Id);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct OutputId(pub Id);
