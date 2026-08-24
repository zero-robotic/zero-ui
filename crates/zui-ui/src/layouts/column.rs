use super::{LayoutContext, LayoutStrategy};
use crate::layout::Constraints;
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
    fn measure(&self, context: &mut LayoutContext<'_>) -> Size {
        let children = &mut *context.children;
        let constraints = context.constraints;
        let mut width: f32 = 0.0;
        let mut height: f32 = self.spacing.0 * children.len().saturating_sub(1) as f32;
        let mut flex_total = 0.0;
        for child in children.iter_mut() {
            if let Some(factor) = child.flex_factor() {
                flex_total += factor;
            } else {
                let size = child.measure(Constraints::loose(constraints.max));
                width = width.max(size.width.0);
                height += size.height.0;
            }
        }
        let remaining = (constraints.max.height.0 - height).max(0.0);
        if flex_total > 0.0 {
            for child in children.iter_mut() {
                if let Some(factor) = child.flex_factor() {
                    let allocated = Dip(remaining * factor / flex_total);
                    let size = child.measure(Constraints {
                        min: Size {
                            width: Dip::ZERO,
                            height: allocated,
                        },
                        max: Size {
                            width: constraints.max.width,
                            height: allocated,
                        },
                    });
                    width = width.max(size.width.0);
                    height += size.height.0;
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
