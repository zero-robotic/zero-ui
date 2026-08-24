use crate::layout::Constraints;
use crate::widget::Widget;
use zui_core::{Rect, Size};

/// Shared state and helpers available to every layout strategy.
pub struct LayoutContext<'a> {
    pub children: &'a mut [Box<dyn Widget>],
    pub constraints: Constraints,
}

impl<'a> LayoutContext<'a> {
    pub fn new(children: &'a mut [Box<dyn Widget>], constraints: Constraints) -> Self {
        Self {
            children,
            constraints,
        }
    }
    pub fn measure_child(&mut self, index: usize, constraints: Constraints) -> Size {
        self.children[index].measure(constraints)
    }
    pub fn child_size(&self, index: usize) -> Size {
        self.children[index].bounds().size
    }
    pub fn arrange_child(&mut self, index: usize, bounds: Rect) {
        self.children[index].arrange(bounds);
    }
}
