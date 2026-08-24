use super::{LayoutContext, LayoutStrategy};
use crate::layout::Constraints;
use zui_core::{Dip, Point, Rect, Size};

#[derive(Clone, Copy, Debug)]
pub struct GridLayout {
    pub columns: usize,
    pub gap: Dip,
}
impl GridLayout {
    pub fn new(columns: usize) -> Self {
        Self {
            columns: columns.max(1),
            gap: Dip(0.0),
        }
    }
    pub fn gap(mut self, value: Dip) -> Self {
        self.gap = value;
        self
    }
}
impl LayoutStrategy for GridLayout {
    fn measure(&self, context: &mut LayoutContext<'_>) -> Size {
        let children = &mut *context.children;
        let constraints = context.constraints;
        let cols = self.columns as f32;
        let cell_width = ((constraints.max.width.0 - self.gap.0 * (cols - 1.0)) / cols).max(0.0);
        let mut rows: Vec<f32> = Vec::new();
        for (i, child) in children.iter_mut().enumerate() {
            let size = child.measure(Constraints::loose(Size {
                width: Dip(cell_width),
                height: constraints.max.height,
            }));
            let row = i / self.columns;
            if rows.len() <= row {
                rows.push(0.0);
            }
            rows[row] = rows[row].max(size.height.0);
        }
        let height = rows.iter().sum::<f32>() + self.gap.0 * rows.len().saturating_sub(1) as f32;
        constraints.constrain(Size {
            width: constraints.max.width,
            height: Dip(height),
        })
    }
    fn arrange(&self, context: &mut LayoutContext<'_>, bounds: Rect) {
        let children = &mut *context.children;
        let cell_width = ((bounds.size.width.0 - self.gap.0 * (self.columns as f32 - 1.0))
            / self.columns as f32)
            .max(0.0);
        let mut rows: Vec<f32> = Vec::new();
        for (i, child) in children.iter().enumerate() {
            let row = i / self.columns;
            if rows.len() <= row {
                rows.push(0.0);
            }
            rows[row] = rows[row].max(child.bounds().size.height.0);
        }
        let mut y = bounds.origin.y.0;
        for (row, row_height) in rows.iter().enumerate() {
            for column in 0..self.columns {
                if let Some(child) = children.get_mut(row * self.columns + column) {
                    let size = child.bounds().size;
                    child.arrange(Rect {
                        origin: Point {
                            x: Dip(bounds.origin.x.0 + column as f32 * (cell_width + self.gap.0)),
                            y: Dip(y),
                        },
                        size,
                    });
                }
            }
            y += *row_height + self.gap.0;
        }
    }
}
