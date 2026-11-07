//! Backend-independent cubic approximations of common shapes.
use crate::{Path, RenderError};
use zui_types::{Point, Rect};

const KAPPA: f64 = 0.552_284_749_830_793_6;

impl Path {
    /// Approximates a circle with four cubic arcs, clockwise in screen coordinates.
    /// Rejects non-finite/negative radii and unrepresentable bounds; zero is empty.
    pub fn circle(center: Point, radius: f64) -> Result<Self, RenderError> {
        if !center.is_finite() || !radius.is_finite() || radius < 0.0 {
            return Err(RenderError::InvalidGeometry);
        }
        Self::ellipse(Rect::new(
            center.x - radius,
            center.y - radius,
            radius * 2.0,
            radius * 2.0,
        ))
    }

    /// Approximates an ellipse inscribed in `rect` with four cubic arcs.
    /// Invalid or overflowing bounds are rejected; zero-area bounds are empty.
    pub fn ellipse(rect: Rect) -> Result<Self, RenderError> {
        if !rect.is_valid() {
            return Err(RenderError::InvalidGeometry);
        }
        if rect.size.width == 0.0 || rect.size.height == 0.0 {
            return Ok(Self::new());
        }
        let (left, top) = (rect.origin.x, rect.origin.y);
        let (right, bottom) = (left + rect.size.width, top + rect.size.height);
        let (rx, ry) = (rect.size.width / 2.0, rect.size.height / 2.0);
        let (cx, cy) = (left + rx, top + ry);
        let (kx, ky) = (rx * KAPPA, ry * KAPPA);
        let mut path = Self::new();
        path.move_to(Point::new(right, cy));
        path.cubic_to(
            Point::new(right, cy + ky),
            Point::new(cx + kx, bottom),
            Point::new(cx, bottom),
        );
        path.cubic_to(
            Point::new(cx - kx, bottom),
            Point::new(left, cy + ky),
            Point::new(left, cy),
        );
        path.cubic_to(
            Point::new(left, cy - ky),
            Point::new(cx - kx, top),
            Point::new(cx, top),
        );
        path.cubic_to(
            Point::new(cx + kx, top),
            Point::new(right, cy - ky),
            Point::new(right, cy),
        );
        path.close();
        Ok(path)
    }

    /// Rectangle with a uniform circular corner radius, approximated by cubic arcs.
    /// Radius is clamped to half the shorter side; zero radius is a rectangle.
    /// Invalid/overflowing bounds or non-finite/negative radii return an error.
    /// Zero-area bounds produce an empty path.
    pub fn rounded_rectangle(rect: Rect, radius: f64) -> Result<Self, RenderError> {
        if !rect.is_valid() || !radius.is_finite() || radius < 0.0 {
            return Err(RenderError::InvalidGeometry);
        }
        if rect.size.width == 0.0 || rect.size.height == 0.0 {
            return Ok(Self::new());
        }
        let r = radius
            .min(rect.size.width / 2.0)
            .min(rect.size.height / 2.0);
        if r == 0.0 {
            return Ok(Self::rectangle(rect));
        }
        let (left, top) = (rect.origin.x, rect.origin.y);
        let (right, bottom) = (left + rect.size.width, top + rect.size.height);
        let k = r * (1.0 - KAPPA);
        let mut path = Self::new();
        path.move_to(Point::new(left + r, top));
        path.line_to(Point::new(right - r, top));
        path.cubic_to(
            Point::new(right - k, top),
            Point::new(right, top + k),
            Point::new(right, top + r),
        );
        path.line_to(Point::new(right, bottom - r));
        path.cubic_to(
            Point::new(right, bottom - k),
            Point::new(right - k, bottom),
            Point::new(right - r, bottom),
        );
        path.line_to(Point::new(left + r, bottom));
        path.cubic_to(
            Point::new(left + k, bottom),
            Point::new(left, bottom - k),
            Point::new(left, bottom - r),
        );
        path.line_to(Point::new(left, top + r));
        path.cubic_to(
            Point::new(left, top + k),
            Point::new(left + k, top),
            Point::new(left + r, top),
        );
        path.close();
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PathElement;

    #[test]
    fn rejects_invalid_and_overflowing_geometry() {
        for radius in [-1.0, f64::NAN, f64::INFINITY] {
            assert!(Path::circle(Point::ZERO, radius).is_err());
            assert!(Path::rounded_rectangle(Rect::new(0.0, 0.0, 20.0, 10.0), radius).is_err());
        }
        for rect in [
            Rect::new(0.0, 0.0, -1.0, 2.0),
            Rect::new(f64::NAN, 0.0, 2.0, 2.0),
            Rect::new(f64::MAX, 0.0, f64::MAX, 2.0),
        ] {
            assert!(Path::ellipse(rect).is_err());
            assert!(Path::rounded_rectangle(rect, 1.0).is_err());
        }
        assert!(Path::circle(Point::ZERO, f64::MAX).is_err());
        assert!(Path::circle(Point::new(f64::INFINITY, 0.0), 0.0).is_err());
    }

    #[test]
    fn degenerate_shapes_and_clamping_are_explicit() {
        assert!(
            Path::circle(Point::ZERO, 0.0)
                .unwrap()
                .elements()
                .is_empty()
        );
        for rect in [
            Rect::new(2.0, 3.0, 0.0, 10.0),
            Rect::new(2.0, 3.0, 10.0, 0.0),
        ] {
            assert!(Path::ellipse(rect).unwrap().elements().is_empty());
            assert!(
                Path::rounded_rectangle(rect, 5.0)
                    .unwrap()
                    .elements()
                    .is_empty()
            );
        }
        let rect = Rect::new(-5.0, 2.0, 20.0, 10.0);
        assert_eq!(
            Path::rounded_rectangle(rect, 0.0).unwrap(),
            Path::rectangle(rect)
        );
        assert_eq!(
            Path::rounded_rectangle(rect, f64::MAX).unwrap(),
            Path::rounded_rectangle(rect, 5.0).unwrap()
        );
    }

    #[test]
    fn arcs_are_closed_and_stay_inside_bounds() {
        let rect = Rect::new(-10.0, 20.0, 40.0, 20.0);
        for path in [
            Path::ellipse(rect).unwrap(),
            Path::rounded_rectangle(rect, 8.0).unwrap(),
        ] {
            assert!(path.is_valid());
            assert_eq!(path.elements().last(), Some(&PathElement::Close));
            let mut arcs = 0;
            for element in path.elements() {
                if let PathElement::CubicTo(a, b, end) = element {
                    arcs += 1;
                    for p in [a, b, end] {
                        assert!((-10.0..=30.0).contains(&p.x));
                        assert!((20.0..=40.0).contains(&p.y));
                    }
                }
            }
            assert_eq!(arcs, 4);
        }
        assert_eq!(
            Path::circle(Point::new(10.0, 30.0), 10.0).unwrap(),
            Path::ellipse(Rect::new(0.0, 20.0, 20.0, 20.0)).unwrap()
        );
    }
}
