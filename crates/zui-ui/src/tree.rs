use crate::{
    event::{Action, EventContext, EventResult, UiEvent},
    layout::Constraints,
    theme::Theme,
    widget::{PaintContext, Widget},
};
use zui_render::DisplayList;

pub struct WidgetTree {
    root: Box<dyn Widget>,
    theme: Theme,
}

impl WidgetTree {
    pub fn new(root: impl Widget + 'static) -> Self {
        Self {
            root: Box::new(root),
            theme: Theme::default(),
        }
    }
    pub fn measure(&mut self, constraints: Constraints) -> zui_core::Size {
        self.root.measure(constraints)
    }
    pub fn arrange(&mut self, bounds: zui_core::Rect) {
        self.root.arrange(bounds);
    }
    pub fn layout(&mut self, constraints: Constraints) -> zui_core::Size {
        let size = self.measure(constraints);
        let origin = self.root.bounds().origin;
        self.arrange(zui_core::Rect { origin, size });
        size
    }
    pub fn event(&mut self, event: &UiEvent) -> (EventResult, Vec<Action>) {
        let mut ctx = EventContext::new();
        let result = self.root.event(event, &mut ctx);
        (result, ctx.take_actions())
    }
    pub fn paint(&self, display_list: &mut DisplayList) {
        self.root
            .paint(&mut PaintContext::new(display_list, &self.theme));
    }
    pub fn theme(&self) -> &Theme {
        &self.theme
    }
    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
        self.root.set_theme(&self.theme);
    }
    pub fn root(&self) -> &dyn Widget {
        &*self.root
    }
    pub fn root_mut(&mut self) -> &mut dyn Widget {
        &mut *self.root
    }
}
