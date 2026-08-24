use super::LayoutStrategy;
use crate::{layout::Constraints, widget::Widget};
use zui_core::{Dip, Point, Rect, Size};

#[derive(Clone, Copy, Debug, Default)]
pub struct ColumnLayout {
    pub spacing: Dip,
}
impl ColumnLayout {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn spacing(mut self, value: Dip) -> Self {
        self.spacing = value;
        self
    }
}
impl LayoutStrategy for ColumnLayout {
    fn measure(&self, children: &mut [Box<dyn Widget>], constraints: Constraints) -> Size {
        let mut width: f32 = 0.0;
        let mut height: f32 = 0.0;
        for (i, child) in children.iter_mut().enumerate() {
            let size = child.measure(Constraints::loose(constraints.max));
            if i > 0 {
                height += self.spacing.0;
            }
            width = width.max(size.width.0);
            height += size.height.0;
        }
        constraints.constrain(Size {
            width: Dip(width),
            height: Dip(height),
        })
    }
    fn arrange(&self, children: &mut [Box<dyn Widget>], bounds: Rect) {
        let mut y = bounds.origin.y.0;
        for (i, child) in children.iter_mut().enumerate() {
            if i > 0 {
                y += self.spacing.0;
            }
            let size = child.bounds().size;
            child.arrange(Rect {
                origin: Point {
                    x: bounds.origin.x,
                    y: Dip(y),
                },
                size,
            });
            y += size.height.0;
        }
    }
}
