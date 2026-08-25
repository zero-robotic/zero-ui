use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use zui_core::{Color, Point, Rect, Size};
use zui_render::DisplayList;

use crate::{
    event::{EventContext, EventResult, UiEvent},
    layout::Constraints,
    theme::Theme,
};

static NEXT_WIDGET_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WidgetId(pub(crate) u64);

impl WidgetId {
    pub fn new() -> Self {
        Self(NEXT_WIDGET_ID.fetch_add(1, Ordering::Relaxed))
    }
}

pub struct PaintContext<'a> {
    pub display_list: &'a mut DisplayList,
    pub now: Instant,
    pub theme: &'a Theme,
}
impl<'a> PaintContext<'a> {
    pub fn new(display_list: &'a mut DisplayList, theme: &'a Theme) -> Self {
        Self {
            display_list,
            now: Instant::now(),
            theme,
        }
    }
    pub fn fill_rect(&mut self, rect: Rect, color: Color) {
        self.display_list.fill_rect(rect, color);
    }
    pub fn fill_rounded_rect(&mut self, rect: Rect, radius: zui_core::Dip, color: Color) {
        self.display_list.fill_rounded_rect(rect, radius, color);
    }
    pub fn draw_text(&mut self, text: impl Into<String>, origin: Point, color: Color, scale: u32) {
        self.display_list.text(text, origin, color, scale);
    }
}

pub trait Widget {
    fn id(&self) -> WidgetId;
    fn bounds(&self) -> Rect;
    fn measure(&mut self, constraints: Constraints) -> Size;
    fn arrange(&mut self, bounds: Rect);
    fn flex_factor(&self) -> Option<f32> {
        None
    }
    fn next_redraw(&self) -> Option<Instant> {
        None
    }
    fn set_bounds(&mut self, bounds: Rect) {
        self.arrange(bounds);
    }
    fn layout(&mut self, constraints: Constraints) -> Size {
        let size = self.measure(constraints);
        let origin = self.bounds().origin;
        self.arrange(Rect { origin, size });
        size
    }
    fn event(&mut self, event: &UiEvent, ctx: &mut EventContext) -> EventResult;
    fn set_theme(&mut self, _theme: &Theme) {}
    fn paint(&self, _ctx: &mut PaintContext<'_>) {}
}
