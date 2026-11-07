/// A point in logical coordinates unless explicitly documented otherwise.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub const ZERO: Self = Self::new(0.0, 0.0);

    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

impl Size {
    pub const ZERO: Self = Self::new(0.0, 0.0);

    pub const fn new(width: f64, height: f64) -> Self {
        Self { width, height }
    }

    pub fn is_valid(self) -> bool {
        self.width.is_finite() && self.height.is_finite() && self.width >= 0.0 && self.height >= 0.0
    }
}

/// Physical pixel dimensions; intentionally distinct from logical `Size`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Extent {
    pub width: u32,
    pub height: u32,
}

impl Extent {
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    pub fn rgba_len(self) -> Option<usize> {
        (self.width as usize)
            .checked_mul(self.height as usize)?
            .checked_mul(4)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub origin: Point,
    pub size: Size,
}

impl Rect {
    pub const fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            origin: Point::new(x, y),
            size: Size::new(width, height),
        }
    }

    pub fn is_valid(self) -> bool {
        self.origin.is_finite()
            && self.size.is_valid()
            && (self.origin.x + self.size.width).is_finite()
            && (self.origin.y + self.size.height).is_finite()
    }

    /// Half-open bounds prevent neighboring controls sharing an edge hit.
    pub fn contains(self, point: Point) -> bool {
        self.is_valid()
            && point.x >= self.origin.x
            && point.y >= self.origin.y
            && point.x < self.origin.x + self.size.width
            && point.y < self.origin.y + self.size.height
    }
}

/// Affine coefficients [a, b, c, d, e, f]: (ax + cy + e, bx + dy + f).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine(pub [f64; 6]);

impl Default for Affine {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Affine {
    pub const IDENTITY: Self = Self([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);

    pub const fn translate(x: f64, y: f64) -> Self {
        Self([1.0, 0.0, 0.0, 1.0, x, y])
    }

    pub const fn scale(x: f64, y: f64) -> Self {
        Self([x, 0.0, 0.0, y, 0.0, 0.0])
    }

    pub fn is_finite(self) -> bool {
        self.0.iter().all(|v| v.is_finite())
    }

    pub fn apply(self, p: Point) -> Point {
        let [a, b, c, d, e, f] = self.0;
        Point::new(a * p.x + c * p.y + e, b * p.x + d * p.y + f)
    }

    /// Returns `self * local`: local coordinates are transformed first.
    pub fn compose(self, local: Self) -> Self {
        let [a, b, c, d, e, f] = self.0;
        let [g, h, i, j, k, l] = local.0;
        Self([
            a * g + c * h,
            b * g + d * h,
            a * i + c * j,
            b * i + d * j,
            a * k + c * l + e,
            b * k + d * l + f,
        ])
    }

    pub fn inverse(self) -> Option<Self> {
        let [a, b, c, d, e, f] = self.0;
        let determinant = a * d - b * c;
        if !self.is_finite() || !determinant.is_finite() || determinant == 0.0 {
            return None;
        }
        let result = Self([
            d / determinant,
            -b / determinant,
            -c / determinant,
            a / determinant,
            (c * f - d * e) / determinant,
            (b * e - a * f) / determinant,
        ]);
        result.is_finite().then_some(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_transform_can_be_used_for_hit_testing() {
        let transform = Affine::scale(2.0, 2.0).compose(Affine::translate(10.0, 20.0));
        let local = Point::new(4.0, 8.0);
        assert_eq!(transform.apply(local), Point::new(28.0, 56.0));
        assert_eq!(
            transform.inverse().unwrap().apply(transform.apply(local)),
            local
        );
        assert!(Affine::scale(0.0, 1.0).inverse().is_none());
    }

    #[test]
    fn adjacent_rectangles_have_unambiguous_edges() {
        assert!(!Rect::new(0.0, 0.0, 10.0, 10.0).contains(Point::new(10.0, 5.0)));
        assert!(Rect::new(10.0, 0.0, 10.0, 10.0).contains(Point::new(10.0, 5.0)));
    }
}
