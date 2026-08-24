use super::{LayoutContext, LayoutStrategy};
use crate::layout::Constraints;
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
    fn measure(&self, context: &mut LayoutContext<'_>) -> Size {
        let children = &mut *context.children;
        let constraints = context.constraints;
        let mut width = self.spacing.0 * children.len().saturating_sub(1) as f32;
        let mut height: f32 = 0.0;
        let mut flex_total = 0.0;
        for child in children.iter_mut() {
            if let Some(factor) = child.flex_factor() {
                flex_total += factor;
            } else {
                let size = child.measure(Constraints::loose(constraints.max));
                width += size.width.0;
                height = height.max(size.height.0);
            }
        }
        let remaining = (constraints.max.width.0 - width).max(0.0);
        if flex_total > 0.0 {
            for child in children.iter_mut() {
                if let Some(factor) = child.flex_factor() {
                    let allocated = Dip(remaining * factor / flex_total);
                    let size = child.measure(Constraints {
                        min: Size {
                            width: allocated,
                            height: Dip::ZERO,
                        },
                        max: Size {
                            width: allocated,
                            height: constraints.max.height,
                        },
                    });
                    width += size.width.0;
                    height = height.max(size.height.0);
                }
            }
        }
        constraints.constrain(Size {
            width: Dip(width),
            height: Dip(height),
        })
    }
    fn arrange(&self, context: &mut LayoutContext<'_>, bounds: Rect) {
        let children = &mut *context.children;
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
