use alloc::vec::Vec;
use zui_types::{Point, Rect};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PathElement {
    MoveTo(Point),
    LineTo(Point),
    QuadTo(Point, Point),
    CubicTo(Point, Point, Point),
    Close,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Path {
    elements: Vec<PathElement>,
}

impl Path {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn elements(&self) -> &[PathElement] {
        &self.elements
    }
    pub(crate) fn storage_bytes(&self) -> usize {
        self.elements
            .capacity()
            .saturating_mul(core::mem::size_of::<PathElement>())
    }
    pub fn move_to(&mut self, point: Point) {
        self.elements.push(PathElement::MoveTo(point));
    }
    pub fn line_to(&mut self, point: Point) {
        self.elements.push(PathElement::LineTo(point));
    }
    pub fn quad_to(&mut self, control: Point, end: Point) {
        self.elements.push(PathElement::QuadTo(control, end));
    }
    pub fn cubic_to(&mut self, first: Point, second: Point, end: Point) {
        self.elements.push(PathElement::CubicTo(first, second, end));
    }
    pub fn close(&mut self) {
        self.elements.push(PathElement::Close);
    }

    pub fn rectangle(rect: Rect) -> Self {
        let Point { x, y } = rect.origin;
        let mut path = Self::new();
        path.move_to(Point::new(x, y));
        path.line_to(Point::new(x + rect.size.width, y));
        path.line_to(Point::new(x + rect.size.width, y + rect.size.height));
        path.line_to(Point::new(x, y + rect.size.height));
        path.close();
        path
    }

    pub fn is_valid(&self) -> bool {
        let mut has_start = false;
        for element in &self.elements {
            let valid = match *element {
                PathElement::MoveTo(p) => {
                    has_start = true;
                    p.is_finite()
                }
                PathElement::LineTo(p) => has_start && p.is_finite(),
                PathElement::QuadTo(c, p) => has_start && c.is_finite() && p.is_finite(),
                PathElement::CubicTo(a, b, p) => {
                    has_start && a.is_finite() && b.is_finite() && p.is_finite()
                }
                PathElement::Close => has_start,
            };
            if !valid {
                return false;
            }
        }
        true
    }
}
