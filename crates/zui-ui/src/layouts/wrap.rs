use super::LayoutStrategy;
use crate::{layout::Constraints, widget::Widget};
use zui_core::{Dip, Point, Rect, Size};

#[derive(Clone, Copy, Debug, Default)]
pub struct WrapLayout {
    pub spacing: Dip,
    pub run_spacing: Dip,
}
impl WrapLayout {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn gap(mut self, value: Dip) -> Self {
        self.spacing = value;
        self.run_spacing = value;
        self
    }
}
impl LayoutStrategy for WrapLayout {
    fn measure(&self, children: &mut [Box<dyn Widget>], constraints: Constraints) -> Size {
        let max_width = constraints.max.width.0;
        let mut x: f32 = 0.0;
        let mut y: f32 = 0.0;
        let mut line_height: f32 = 0.0;
        let mut used_width: f32 = 0.0;
        for child in children {
            let size = child.measure(Constraints::loose(constraints.max));
            if x > 0.0 && x + size.width.0 > max_width {
                x = 0.0;
                y += line_height + self.run_spacing.0;
                line_height = 0.0;
            }
            x += size.width.0 + self.spacing.0;
            used_width = used_width.max((x - self.spacing.0).max(0.0));
            line_height = line_height.max(size.height.0);
        }
        constraints.constrain(Size {
            width: Dip(used_width),
            height: Dip(y + line_height),
        })
    }
    fn arrange(&self, children: &mut [Box<dyn Widget>], bounds: Rect) {
        let mut x: f32 = 0.0;
        let mut y: f32 = 0.0;
        let mut line_height: f32 = 0.0;
        for child in children {
            let size = child.bounds().size;
            if x > 0.0 && x + size.width.0 > bounds.size.width.0 {
                x = 0.0;
                y += line_height + self.run_spacing.0;
                line_height = 0.0;
            }
            child.arrange(Rect {
                origin: Point {
                    x: Dip(bounds.origin.x.0 + x),
                    y: Dip(bounds.origin.y.0 + y),
                },
                size,
            });
            x += size.width.0 + self.spacing.0;
            line_height = line_height.max(size.height.0);
        }
    }
}
