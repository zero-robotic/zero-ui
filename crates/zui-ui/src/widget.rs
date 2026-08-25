use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use zui_core::{Color, Point, Rect, Size};
use zui_render::{DisplayList, IconPath, ImageId, RenderNode, RenderNodeBuilder, Transform};

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
    origin: Point,
}

pub(crate) fn build_render_node_from_paint(
    id: WidgetId,
    bounds: Rect,
    theme: &Theme,
    paint: impl FnOnce(&mut PaintContext<'_>),
) -> RenderNode {
    let mut builder = RenderNodeBuilder::for_widget(bounds);
    builder.source_id(id.0);
    paint(&mut PaintContext::new_at(
        builder.commands_mut(),
        theme,
        bounds.origin,
    ));
    builder.finish()
}
impl<'a> PaintContext<'a> {
    pub fn new(display_list: &'a mut DisplayList, theme: &'a Theme) -> Self {
        Self {
            display_list,
            now: Instant::now(),
            theme,
            origin: Point::default(),
        }
    }
    pub fn new_at(display_list: &'a mut DisplayList, theme: &'a Theme, origin: Point) -> Self {
        Self {
            display_list,
            now: Instant::now(),
            theme,
            origin,
        }
    }
    fn local_point(&self, point: Point) -> Point {
        Point {
            x: zui_core::Dip(point.x.0 - self.origin.x.0),
            y: zui_core::Dip(point.y.0 - self.origin.y.0),
        }
    }
    fn local_rect(&self, rect: Rect) -> Rect {
        Rect {
            origin: self.local_point(rect.origin),
            size: rect.size,
        }
    }
    pub fn fill_rect(&mut self, rect: Rect, color: Color) {
        self.display_list.fill_rect(self.local_rect(rect), color);
    }
    pub fn fill_rounded_rect(&mut self, rect: Rect, radius: zui_core::Dip, color: Color) {
        self.display_list
            .fill_rounded_rect(self.local_rect(rect), radius, color);
    }
    pub fn draw_line(&mut self, start: Point, end: Point, width: zui_core::Dip, color: Color) {
        self.display_list
            .line(self.local_point(start), self.local_point(end), width, color);
    }
    pub fn draw_text(&mut self, text: impl Into<String>, origin: Point, color: Color, scale: u32) {
        self.display_list
            .text(text, self.local_point(origin), color, scale);
    }
    pub fn draw_icon(&mut self, rect: Rect, path: IconPath, color: Color, stroke: zui_core::Dip) {
        let path = zui_render::IconPath::new(
            path.segments
                .into_iter()
                .map(|segment| zui_render::LineSegment {
                    start: self.local_point(segment.start),
                    end: self.local_point(segment.end),
                })
                .collect::<Vec<_>>(),
        );
        self.display_list
            .icon(self.local_rect(rect), path, color, stroke);
    }
    pub fn draw_image(&mut self, rect: Rect, image: ImageId, opacity: f32) {
        self.display_list
            .image(self.local_rect(rect), image, opacity);
    }
    pub fn push_clip(&mut self, rect: Rect) {
        self.display_list.clip(self.local_rect(rect));
    }
    pub fn push_transform(&mut self, transform: Transform) {
        self.display_list.transform(transform);
    }
    pub fn push_opacity(&mut self, opacity: f32) {
        self.display_list.opacity(opacity);
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

    fn build_render_node(&self, theme: &Theme) -> RenderNode {
        build_render_node_from_paint(self.id(), self.bounds(), theme, |ctx| self.paint(ctx))
    }

    fn build_render_node_with_cache(
        &self,
        previous: Option<&RenderNode>,
        dirty_region: Option<Rect>,
        theme: &Theme,
    ) -> RenderNode {
        if let (Some(previous), Some(dirty_region)) = (previous, dirty_region) {
            // A standalone node does not know its parent's world transform.
            // Do not make a local-vs-world intersection decision for an
            // already-normalized cached node; rebuilding it is conservative
            // and avoids stale rendering in nested layouts.
            if previous.coordinates_normalized {
                return self.build_render_node(theme);
            }
            if !rect_intersects(previous.world_bounds(), dirty_region) {
                return previous.clone();
            }
        }
        self.build_render_node(theme)
    }

    fn build_render_node_with_dirty_widgets(
        &self,
        previous: Option<&RenderNode>,
        dirty_region: Option<Rect>,
        dirty_widgets: &HashSet<WidgetId>,
        theme: &Theme,
    ) -> RenderNode {
        if dirty_widgets.contains(&self.id()) {
            self.build_render_node(theme)
        } else {
            self.build_render_node_with_cache(previous, dirty_region, theme)
        }
    }
}

fn rect_intersects(a: Rect, b: Rect) -> bool {
    a.origin.x.0 < b.origin.x.0 + b.size.width.0
        && a.origin.x.0 + a.size.width.0 > b.origin.x.0
        && a.origin.y.0 < b.origin.y.0 + b.size.height.0
        && a.origin.y.0 + a.size.height.0 > b.origin.y.0
}
