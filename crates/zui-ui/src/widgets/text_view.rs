use crate::{
    event::{EventContext, EventResult, UiEvent},
    layout::Constraints,
    theme::Theme,
    ScrollArea, Text, TextWrap, Widget, WidgetId,
};
use zui_core::{Rect, Size};
/// Read-only long-form text with a clipped, scrollable viewport.
pub struct TextView {
    id: WidgetId,
    scroll: ScrollArea,
}
impl TextView {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            id: WidgetId::new(),
            scroll: ScrollArea::vertical(Text::new(text).wrap(TextWrap::Word)),
        }
    }
    pub fn height(mut self, value: f32) -> Self {
        self.scroll = self.scroll.height(value);
        self
    }
}
impl Widget for TextView {
    fn id(&self) -> WidgetId {
        self.id
    }
    fn bounds(&self) -> Rect {
        self.scroll.bounds()
    }
    fn measure(&mut self, c: Constraints) -> Size {
        self.scroll.measure(c)
    }
    fn arrange(&mut self, b: Rect) {
        self.scroll.arrange(b)
    }
    fn event(&mut self, e: &UiEvent, c: &mut EventContext) -> EventResult {
        self.scroll.event(e, c)
    }
    fn set_theme(&mut self, t: &Theme) {
        self.scroll.set_theme(t)
    }
    fn build_render_node(&self, t: &Theme) -> zui_render::RenderNode {
        self.scroll.build_render_node(t)
    }
}
