use crate::{
    event::{Action, EventContext, EventResult, UiEvent},
    layout::Constraints,
    widget::{PaintContext, Widget},
};
use zui_render::DisplayList;

pub struct WidgetTree {
    root: Box<dyn Widget>,
}

impl WidgetTree {
    pub fn new(root: impl Widget + 'static) -> Self {
        Self {
            root: Box::new(root),
        }
    }
    pub fn layout(&mut self, constraints: Constraints) -> zui_core::Size {
        self.root.layout(constraints)
    }
    pub fn event(&mut self, event: &UiEvent) -> (EventResult, Vec<Action>) {
        let mut ctx = EventContext::new();
        let result = self.root.event(event, &mut ctx);
        (result, ctx.take_actions())
    }
    pub fn paint(&self, display_list: &mut DisplayList) {
        self.root.paint(&mut PaintContext::new(display_list));
    }
    pub fn root(&self) -> &dyn Widget {
        &*self.root
    }
    pub fn root_mut(&mut self) -> &mut dyn Widget {
        &mut *self.root
    }
}
