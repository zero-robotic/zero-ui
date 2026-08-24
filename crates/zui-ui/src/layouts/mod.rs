use crate::{
    event::{EventContext, EventResult, UiEvent},
    layout::Constraints,
    theme::Theme,
    widget::{PaintContext, Widget, WidgetId},
};
use zui_core::{Rect, Size};

mod column;
mod grid;
mod row;
mod stack;
mod wrap;

pub use column::ColumnLayout;
pub use grid::GridLayout;
pub use row::RowLayout;
pub use stack::StackLayout;
pub use wrap::WrapLayout;

pub trait LayoutStrategy {
    fn measure(&self, children: &mut [Box<dyn Widget>], constraints: Constraints) -> Size;
    fn arrange(&self, children: &mut [Box<dyn Widget>], bounds: Rect);
}

pub struct Layout {
    id: WidgetId,
    strategy: Box<dyn LayoutStrategy>,
    children: Vec<Box<dyn Widget>>,
    bounds: Rect,
}
impl Layout {
    pub fn new(strategy: impl LayoutStrategy + 'static) -> Self {
        Self {
            id: WidgetId::new(),
            strategy: Box::new(strategy),
            children: Vec::new(),
            bounds: Rect::default(),
        }
    }
    pub fn child(mut self, child: impl Widget + 'static) -> Self {
        self.children.push(Box::new(child));
        self
    }
    pub fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }
}
impl Widget for Layout {
    fn id(&self) -> WidgetId {
        self.id
    }
    fn bounds(&self) -> Rect {
        self.bounds
    }
    fn measure(&mut self, constraints: Constraints) -> Size {
        let size = self.strategy.measure(&mut self.children, constraints);
        self.bounds.size = size;
        size
    }
    fn arrange(&mut self, bounds: Rect) {
        self.bounds = bounds;
        self.strategy.arrange(&mut self.children, bounds);
    }
    fn event(&mut self, event: &UiEvent, ctx: &mut EventContext) -> EventResult {
        for child in self.children.iter_mut().rev() {
            if child.event(event, ctx) != EventResult::Ignored {
                return EventResult::Handled;
            }
        }
        EventResult::Ignored
    }
    fn paint(&self, ctx: &mut PaintContext<'_>) {
        for child in &self.children {
            child.paint(ctx);
        }
    }
    fn set_theme(&mut self, theme: &Theme) {
        for child in &mut self.children {
            child.set_theme(theme);
        }
    }
}
