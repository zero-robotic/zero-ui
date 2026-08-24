use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use zui_core::{Color, Point, Rect, Size};
use zui_render::DisplayList;

use crate::{
    event::{EventContext, EventResult, UiEvent},
    layout::Constraints,
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
}
impl<'a> PaintContext<'a> {
    pub fn new(display_list: &'a mut DisplayList) -> Self {
        Self {
            display_list,
            now: Instant::now(),
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
    fn set_bounds(&mut self, bounds: Rect);
    fn layout(&mut self, constraints: Constraints) -> Size;
    fn event(&mut self, event: &UiEvent, ctx: &mut EventContext) -> EventResult;
    fn paint(&self, _ctx: &mut PaintContext<'_>) {}
}
