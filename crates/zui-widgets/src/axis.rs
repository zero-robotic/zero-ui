use zui_types::{Point, Rect, Size};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollAxes {
    Vertical,
    Horizontal,
    Both,
}
impl ScrollAxes {
    pub(super) fn contains(self, axis: Axis) -> bool {
        matches!(
            (self, axis),
            (Self::Both, _)
                | (Self::Vertical, Axis::Vertical)
                | (Self::Horizontal, Axis::Horizontal)
        )
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Axis {
    Horizontal,
    Vertical,
}
impl Axis {
    pub const ALL: [Self; 2] = [Self::Horizontal, Self::Vertical];
    pub fn index(self) -> usize {
        match self {
            Self::Horizontal => 0,
            Self::Vertical => 1,
        }
    }
    pub fn other(self) -> Self {
        match self {
            Self::Horizontal => Self::Vertical,
            Self::Vertical => Self::Horizontal,
        }
    }
    pub fn value(self, point: Point) -> f64 {
        match self {
            Self::Horizontal => point.x,
            Self::Vertical => point.y,
        }
    }
    pub fn length(self, size: Size) -> f64 {
        match self {
            Self::Horizontal => size.width,
            Self::Vertical => size.height,
        }
    }
    pub fn set(self, point: &mut Point, value: f64) {
        match self {
            Self::Horizontal => point.x = value,
            Self::Vertical => point.y = value,
        }
    }
    pub fn rect(self, start: f64, cross: f64, length: f64, width: f64) -> Rect {
        match self {
            Self::Horizontal => Rect::new(start, cross, length, width),
            Self::Vertical => Rect::new(cross, start, width, length),
        }
    }
    pub fn reveal_delta(self, viewport: Rect, area: Rect) -> f64 {
        let start = self.value(area.origin) - self.value(viewport.origin);
        let end = start + self.length(area.size) - self.length(viewport.size);
        if self.length(area.size) <= self.length(viewport.size) {
            if start < 0.0 { start } else { end.max(0.0) }
        } else if start > 0.0 {
            start
        } else {
            end.min(0.0)
        }
    }
}
