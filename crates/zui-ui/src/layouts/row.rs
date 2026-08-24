use super::LayoutStrategy;
use crate::{layout::Constraints, widget::Widget};
use zui_core::{Dip, Point, Rect, Size};

#[derive(Clone, Copy, Debug, Default)]
pub struct RowLayout {
    pub spacing: Dip,
}
impl RowLayout {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn spacing(mut self, value: Dip) -> Self {
        self.spacing = value;
        self
    }
}
impl LayoutStrategy for RowLayout {
    fn measure(&self, children: &mut [Box<dyn Widget>], constraints: Constraints) -> Size {
        let mut width = 0.0;
        let mut height: f32 = 0.0;
        for (i, child) in children.iter_mut().enumerate() {
            let size = child.measure(Constraints::loose(constraints.max));
            if i > 0 {
                width += self.spacing.0;
            }
            width += size.width.0;
            height = height.max(size.height.0);
        }
        constraints.constrain(Size {
            width: Dip(width),
            height: Dip(height),
        })
    }
    fn arrange(&self, children: &mut [Box<dyn Widget>], bounds: Rect) {
        let mut x = bounds.origin.x.0;
        for (i, child) in children.iter_mut().enumerate() {
            if i > 0 {
                x += self.spacing.0;
            }
            let size = child.bounds().size;
            child.arrange(Rect {
                origin: Point {
                    x: Dip(x),
                    y: bounds.origin.y,
                },
                size,
            });
            x += size.width.0;
        }
    }
}
