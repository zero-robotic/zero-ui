use crate::{
    event::{EventContext, EventResult, UiEvent},
    layout::Constraints,
    theme::Theme,
    widget::{PaintContext, Widget, WidgetId},
};
use zui_core::{Dip, Rect, Size};

pub struct SizedBox {
    id: WidgetId,
    child: Box<dyn Widget>,
    width: Option<Dip>,
    height: Option<Dip>,
    bounds: Rect,
}
impl SizedBox {
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            id: WidgetId::new(),
            child: Box::new(child),
            width: None,
            height: None,
            bounds: Rect::default(),
        }
    }
    pub fn width(mut self, value: f32) -> Self {
        self.width = Some(Dip(value));
        self
    }
    pub fn height(mut self, value: f32) -> Self {
        self.height = Some(Dip(value));
        self
    }
}
impl Widget for SizedBox {
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
        let desired = Size {
            width: self.width.unwrap_or(constraints.max.width),
            height: self.height.unwrap_or(constraints.max.height),
        };
        let size = constraints.constrain(desired);
        self.child.measure(Constraints::tight(size));
        self.child.arrange(Rect {
            origin: self.bounds.origin,
            size,
        });
        self.bounds.size = size;
        size
    }
    fn event(&mut self, event: &UiEvent, ctx: &mut EventContext) -> EventResult {
        self.child.event(event, ctx)
    }
    fn paint(&self, ctx: &mut PaintContext<'_>) {
        self.child.paint(ctx);
    }
    fn set_theme(&mut self, theme: &Theme) {
        self.child.set_theme(theme);
    }
}
