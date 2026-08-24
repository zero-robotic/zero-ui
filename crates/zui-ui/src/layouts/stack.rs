use super::{LayoutContext, LayoutStrategy};
use crate::layout::Constraints;
use zui_core::{Rect, Size};

#[derive(Clone, Copy, Debug, Default)]
pub struct StackLayout;
impl StackLayout {
    pub fn new() -> Self {
        Self
    }
}
impl LayoutStrategy for StackLayout {
    fn measure(&self, context: &mut LayoutContext<'_>) -> Size {
        let children = &mut *context.children;
        let constraints = context.constraints;
        let mut size = Size::ZERO;
        for child in children {
            let child_size = child.measure(Constraints::loose(constraints.max));
            size.width.0 = size.width.0.max(child_size.width.0);
            size.height.0 = size.height.0.max(child_size.height.0);
        }
        constraints.constrain(size)
    }
    fn arrange(&self, context: &mut LayoutContext<'_>, bounds: Rect) {
        let children = &mut *context.children;
        for child in children {
            child.arrange(Rect {
                origin: bounds.origin,
                size: child.bounds().size,
            });
        }
    }
}
