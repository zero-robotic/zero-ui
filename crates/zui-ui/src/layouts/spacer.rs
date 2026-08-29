use crate::{
    event::{EventContext, EventResult, UiEvent},
    layout::Constraints,
    theme::Theme,
    widget::{build_render_node_with_commands, Widget, WidgetId},
};
use zui_core::{Dip, Rect, Size};

pub struct Spacer {
    id: WidgetId,
    width: Dip,
    height: Dip,
    flex: Option<f32>,
    bounds: Rect,
}
impl Spacer {
    #[deprecated(note = "use Gap::horizontal/vertical or Spacer::flex instead")]
    pub fn new(extent: f32) -> Self {
        Self {
            id: WidgetId::new(),
            width: Dip(extent),
            height: Dip(extent),
            flex: None,
            bounds: Rect::default(),
        }
    }
    pub fn flex(factor: f32) -> Self {
        Self {
            id: WidgetId::new(),
            width: Dip::ZERO,
            height: Dip::ZERO,
            flex: Some(factor.max(0.0)),
            bounds: Rect::default(),
        }
    }
    pub fn width(mut self, value: f32) -> Self {
        self.width = Dip(value);
        self
    }
    pub fn height(mut self, value: f32) -> Self {
        self.height = Dip(value);
        self
    }
}
impl Widget for Spacer {
    fn id(&self) -> WidgetId {
        self.id
    }
    fn bounds(&self) -> Rect {
        self.bounds
    }
    fn arrange(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }
    fn measure(&mut self, constraints: Constraints) -> Size {
        let size = constraints.constrain(Size {
            width: self.width,
            height: self.height,
        });
        self.bounds.size = size;
        size
    }
    fn flex_factor(&self) -> Option<f32> {
        self.flex
    }
    fn event(&mut self, _event: &UiEvent, _ctx: &mut EventContext) -> EventResult {
        EventResult::Ignored
    }
    fn build_render_node(&self, theme: &Theme) -> zui_render::RenderNode {
        build_render_node_with_commands(self.id, self.bounds, theme, |_ctx| {})
    }
}
