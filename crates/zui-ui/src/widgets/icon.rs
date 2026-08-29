use crate::{
    event::{EventContext, EventResult, UiEvent},
    layout::Constraints,
    theme::Theme,
    widget::{build_render_node_with_commands, Widget, WidgetId},
};
use zui_core::{Color, Dip, Point, Rect, Size};
use zui_render::{IconPath, LineSegment};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IconName {
    Check,
    Close,
    Menu,
    Minus,
    Pause,
    Play,
    Plus,
    ArrowLeft,
    ArrowRight,
}

pub struct Icon {
    id: WidgetId,
    name: IconName,
    bounds: Rect,
    theme: Theme,
}

impl Icon {
    pub fn new(name: IconName) -> Self {
        Self {
            id: WidgetId::new(),
            name,
            bounds: Rect::default(),
            theme: Theme::default(),
        }
    }

    pub fn name(&self) -> IconName {
        self.name
    }

    fn build_render_commands(&self, ctx: &mut crate::PaintContext<'_>) {
        build_icon_commands(
            ctx,
            self.name,
            self.bounds,
            ctx.theme.icon.color,
            ctx.theme.icon.stroke_width,
        );
    }
}

impl Widget for Icon {
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
            width: self.theme.icon.size,
            height: self.theme.icon.size,
        });
        self.bounds.size = size;
        size
    }
    fn event(&mut self, _event: &UiEvent, _ctx: &mut EventContext) -> EventResult {
        EventResult::Ignored
    }
    fn set_theme(&mut self, theme: &Theme) {
        self.theme = theme.clone();
    }
    fn build_render_node(&self, theme: &Theme) -> zui_render::RenderNode {
        build_render_node_with_commands(self.id, self.bounds, theme, |ctx| {
            self.build_render_commands(ctx)
        })
    }
    fn build_render_node_incremental(
        &self,
        context: &mut crate::RenderBuildContext<'_>,
    ) -> zui_render::RenderNode {
        crate::widget::build_leaf_render_node_incremental(self.id, context, |theme| {
            self.build_render_node(theme)
        })
    }
}

pub(crate) fn build_icon_commands(
    ctx: &mut crate::PaintContext<'_>,
    name: IconName,
    bounds: Rect,
    color: Color,
    stroke: Dip,
) {
    let x = bounds.origin.x.0;
    let y = bounds.origin.y.0;
    let size = bounds.size.width.0.min(bounds.size.height.0);
    let point = |px: f32, py: f32| Point {
        x: Dip(x + px * size),
        y: Dip(y + py * size),
    };
    let mut segments = Vec::new();
    let mut line = |a: Point, b: Point| {
        segments.push(LineSegment { start: a, end: b });
    };
    match name {
        IconName::Check => {
            line(point(0.18, 0.52), point(0.42, 0.76));
            line(point(0.42, 0.76), point(0.84, 0.24));
        }
        IconName::Close => {
            line(point(0.22, 0.22), point(0.78, 0.78));
            line(point(0.78, 0.22), point(0.22, 0.78));
        }
        IconName::Menu => {
            line(point(0.18, 0.25), point(0.82, 0.25));
            line(point(0.18, 0.50), point(0.82, 0.50));
            line(point(0.18, 0.75), point(0.82, 0.75));
        }
        IconName::Minus => line(point(0.20, 0.50), point(0.80, 0.50)),
        IconName::Plus => {
            line(point(0.20, 0.50), point(0.80, 0.50));
            line(point(0.50, 0.20), point(0.50, 0.80));
        }
        IconName::ArrowLeft => {
            line(point(0.20, 0.50), point(0.78, 0.50));
            line(point(0.20, 0.50), point(0.45, 0.25));
            line(point(0.20, 0.50), point(0.45, 0.75));
        }
        IconName::ArrowRight => {
            line(point(0.22, 0.50), point(0.80, 0.50));
            line(point(0.80, 0.50), point(0.55, 0.25));
            line(point(0.80, 0.50), point(0.55, 0.75));
        }
        IconName::Play => {
            line(point(0.35, 0.22), point(0.72, 0.50));
            line(point(0.72, 0.50), point(0.35, 0.78));
            line(point(0.35, 0.78), point(0.35, 0.22));
        }
        IconName::Pause => {
            line(point(0.35, 0.22), point(0.35, 0.78));
            line(point(0.65, 0.22), point(0.65, 0.78));
        }
    }
    ctx.draw_icon(bounds, IconPath::new(segments), color, stroke);
}
