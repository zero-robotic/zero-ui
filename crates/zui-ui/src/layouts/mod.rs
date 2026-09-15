use crate::{
    event::{EventContext, EventResult, UiEvent},
    layout::Constraints,
    theme::Theme,
    widget::{RenderBuildContext, Widget, WidgetId},
};
use zui_core::{Rect, Size};
use zui_render::{RenderNode, Transform};

mod align;
mod column;
mod context;
mod gap;
mod grid;
mod padding;
mod row;
mod sized_box;
mod spacer;
mod stack;
mod wrap;

pub use align::{Align, Alignment};
pub use column::ColumnLayout;
pub use context::LayoutContext;
pub use gap::Gap;
pub use grid::GridLayout;
pub use padding::Padding;
pub use row::RowLayout;
pub use sized_box::SizedBox;
pub use spacer::Spacer;
pub use stack::StackLayout;
pub use wrap::WrapLayout;

pub trait LayoutStrategy {
    fn measure(&self, context: &mut LayoutContext<'_>) -> Size;
    fn arrange(&self, context: &mut LayoutContext<'_>, bounds: Rect);
}

pub struct Layout {
    id: WidgetId,
    strategy: Box<dyn LayoutStrategy>,
    children: Vec<Box<dyn Widget>>,
    flex: Option<f32>,
    bounds: Rect,
}
impl Layout {
    pub fn new(strategy: impl LayoutStrategy + 'static) -> Self {
        Self {
            id: WidgetId::new(),
            strategy: Box::new(strategy),
            children: Vec::new(),
            flex: None,
            bounds: Rect::default(),
        }
    }
    /// Lets a row or column allocate this layout a share of its remaining
    /// main-axis space. Without this opt-in, the layout keeps its intrinsic
    /// size when the window grows.
    pub fn flex(mut self, factor: f32) -> Self {
        self.flex = (factor.is_finite() && factor > 0.0).then_some(factor);
        self
    }
    pub fn child(mut self, child: impl Widget + 'static) -> Self {
        self.children.push(Box::new(child));
        self
    }
    pub fn child_box(mut self, child: Box<dyn Widget>) -> Self {
        self.children.push(child);
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
        let mut context = LayoutContext::new(&mut self.children, constraints);
        let size = self.strategy.measure(&mut context);
        self.bounds.size = size;
        size
    }
    fn arrange(&mut self, bounds: Rect) {
        self.bounds = bounds;
        let mut context = LayoutContext::new(&mut self.children, Constraints::tight(bounds.size));
        self.strategy.arrange(&mut context, bounds);
    }
    fn flex_factor(&self) -> Option<f32> {
        self.flex
    }
    fn next_redraw(&self) -> Option<std::time::Instant> {
        self.children
            .iter()
            .filter_map(|child| child.next_redraw())
            .min()
    }
    fn event(&mut self, event: &UiEvent, ctx: &mut EventContext) -> EventResult {
        for child in self.children.iter_mut().rev() {
            match child.event(event, ctx) {
                EventResult::Ignored => {}
                result => return result,
            }
        }
        EventResult::Ignored
    }
    fn build_render_node(&self, theme: &Theme) -> RenderNode {
        let mut node = RenderNode::for_widget(self.bounds);
        node.set_id(self.id.0);
        for child in &self.children {
            node.add_child(child.build_render_node(theme));
        }
        node
    }
    fn semantics(&self) -> crate::SemanticsNode {
        let mut node =
            crate::SemanticsNode::new(self.id, crate::SemanticRole::Group, "").bounds(self.bounds);
        node.children = self
            .children
            .iter()
            .map(|child| child.semantics())
            .collect();
        node
    }
    fn build_render_node_incremental(&self, context: &mut RenderBuildContext<'_>) -> RenderNode {
        if !context.subtree_is_dirty(self.id) {
            if let Some(previous) = context.previous() {
                return previous.clone();
            }
        }
        let mut node = context.localize(RenderNode::for_widget(self.bounds));
        node.set_id(self.id.0);
        let child_world_transform =
            Transform::translate(self.bounds.origin.x, self.bounds.origin.y);
        for (index, child) in self.children.iter().enumerate() {
            let mut child_context = context.child(index, child_world_transform);
            node.add_child(child.build_render_node_incremental(&mut child_context));
        }
        node
    }
    fn set_theme(&mut self, theme: &Theme) {
        for child in &mut self.children {
            child.set_theme(theme);
        }
    }
}
