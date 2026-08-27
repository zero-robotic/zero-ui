use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use zui_core::{Color, Point, Rect, Size};
use zui_render::{
    ClipShape, IconPath, ImageId, PaintCommand, RenderNode, RenderNodeBuilder, Transform,
};

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
    pub commands: &'a mut Vec<PaintCommand>,
    pub now: Instant,
    pub theme: &'a Theme,
    origin: Point,
}

pub struct RenderBuildContext<'a> {
    previous: Option<&'a RenderNode>,
    dirty_paths: Vec<Vec<usize>>,
    force_rebuild: bool,
    theme: &'a Theme,
}

impl<'a> RenderBuildContext<'a> {
    pub fn new(
        previous: Option<&'a RenderNode>,
        dirty_paths: &[Vec<usize>],
        force_rebuild: bool,
        theme: &'a Theme,
    ) -> Self {
        Self {
            previous,
            dirty_paths: dirty_paths.to_vec(),
            force_rebuild,
            theme,
        }
    }

    pub fn theme(&self) -> &'a Theme {
        self.theme
    }

    pub fn previous(&self) -> Option<&'a RenderNode> {
        self.previous
    }

    pub fn child(&self, index: usize) -> Self {
        Self {
            previous: self.previous.and_then(|node| node.children.get(index)),
            dirty_paths: self
                .dirty_paths
                .iter()
                .filter_map(|path| {
                    (path.first().copied() == Some(index)).then(|| path[1..].to_vec())
                })
                .collect(),
            force_rebuild: self.force_rebuild,
            theme: self.theme,
        }
    }

    pub fn subtree_is_dirty(&self, _id: WidgetId) -> bool {
        self.force_rebuild || !self.dirty_paths.is_empty()
    }
}

pub(crate) fn build_render_node_with_commands(
    id: WidgetId,
    bounds: Rect,
    theme: &Theme,
    build_commands: impl FnOnce(&mut PaintContext<'_>),
) -> RenderNode {
    let mut builder = RenderNodeBuilder::for_widget(bounds);
    builder.source_id(id.0);
    build_commands(&mut PaintContext::new_at(
        builder.commands_mut(),
        theme,
        bounds.origin,
    ));
    builder.finish()
}
impl<'a> PaintContext<'a> {
    pub fn new(commands: &'a mut Vec<PaintCommand>, theme: &'a Theme) -> Self {
        Self {
            commands,
            now: Instant::now(),
            theme,
            origin: Point::default(),
        }
    }
    pub fn new_at(commands: &'a mut Vec<PaintCommand>, theme: &'a Theme, origin: Point) -> Self {
        Self {
            commands,
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
        self.commands.push(PaintCommand::Rect {
            rect: self.local_rect(rect),
            color,
        });
    }
    pub fn fill_rounded_rect(&mut self, rect: Rect, radius: zui_core::Dip, color: Color) {
        self.commands.push(PaintCommand::RoundedRect {
            rect: self.local_rect(rect),
            radius,
            color,
        });
    }
    pub fn draw_line(&mut self, start: Point, end: Point, width: zui_core::Dip, color: Color) {
        self.commands.push(PaintCommand::Line {
            start: self.local_point(start),
            end: self.local_point(end),
            width,
            color,
        });
    }
    pub fn draw_text(&mut self, text: impl Into<String>, origin: Point, color: Color, scale: u32) {
        self.commands.push(PaintCommand::Text {
            text: text.into(),
            origin: self.local_point(origin),
            color,
            scale: scale.max(1),
        });
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
        self.commands.push(PaintCommand::Icon {
            rect: self.local_rect(rect),
            path,
            color,
            stroke,
        });
    }
    pub fn draw_image(&mut self, rect: Rect, image: ImageId, opacity: f32) {
        self.commands.push(PaintCommand::Image {
            rect: self.local_rect(rect),
            image,
            opacity: opacity.clamp(0.0, 1.0),
        });
    }
    pub fn push_clip(&mut self, rect: Rect) {
        self.commands
            .push(PaintCommand::PushClip(ClipShape::Rect(self.local_rect(rect))));
    }
    pub fn push_rounded_clip(&mut self, rect: Rect, radius: zui_core::Dip) {
        self.commands.push(PaintCommand::PushClip(ClipShape::RoundedRect {
                rect: self.local_rect(rect),
                radius,
            }));
    }
    pub fn push_path_clip(&mut self, path: IconPath) {
        let path = path.transformed(Transform::translate(
            zui_core::Dip(-self.origin.x.0),
            zui_core::Dip(-self.origin.y.0),
        ));
        self.commands
            .push(PaintCommand::PushClip(ClipShape::Path { path }));
    }
    pub fn pop_clip(&mut self) {
        self.commands.push(PaintCommand::PopClip);
    }
    pub fn push_transform(&mut self, transform: Transform) {
        self.commands.push(PaintCommand::Transform(transform));
    }
    pub fn push_opacity(&mut self, opacity: f32) {
        self.commands
            .push(PaintCommand::Opacity(opacity.clamp(0.0, 1.0)));
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
    /// Records a paint invalidation owned by this widget. The WidgetTree uses
    /// the source id to update only the affected RenderNode subtree.
    fn invalidate(&self, ctx: &mut EventContext, region: Rect) {
        ctx.invalidate_widget(self.id(), region);
    }
    fn set_theme(&mut self, _theme: &Theme) {}
    fn build_render_node(&self, theme: &Theme) -> RenderNode;
    fn build_render_node_incremental(&self, context: &mut RenderBuildContext<'_>) -> RenderNode {
        if !context.subtree_is_dirty(self.id()) {
            if let Some(previous) = context.previous() {
                return previous.clone();
            }
        }
        self.build_render_node(context.theme())
    }
}
