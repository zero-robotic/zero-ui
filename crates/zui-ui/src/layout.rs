use zui_core::{Dip, Point, Rect, Size};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Constraints {
    pub min: Size,
    pub max: Size,
}

impl Constraints {
    pub fn loose(max: Size) -> Self {
        Self {
            min: Size::ZERO,
            max,
        }
    }
    pub fn tight(size: Size) -> Self {
        Self {
            min: size,
            max: size,
        }
    }
    pub fn constrain(self, size: Size) -> Size {
        Size {
            width: Dip(size.width.0.clamp(self.min.width.0, self.max.width.0)),
            height: Dip(size.height.0.clamp(self.min.height.0, self.max.height.0)),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LayoutBox {
    pub rect: Rect,
}

impl LayoutBox {
    pub fn at(origin: Point, size: Size) -> Self {
        Self {
            rect: Rect { origin, size },
        }
    }
}
