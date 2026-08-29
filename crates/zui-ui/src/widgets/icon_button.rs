use super::icon::{build_icon_commands, IconName};
use crate::{
    event::{is_left_press, ActionKind, EventContext, EventResult, UiEvent},
    layout::Constraints,
    theme::Theme,
    widget::{build_render_node_with_commands, Widget, WidgetId},
};
use zui_core::{Dip, Point, Rect, Size};
use zui_platform::InputEvent;

pub struct IconButton {
    id: WidgetId,
    icon: IconName,
    bounds: Rect,
    hovered: bool,
    theme: Theme,
}

impl IconButton {
    fn build_render_commands(&self, ctx: &mut crate::PaintContext<'_>) {
        let style = &ctx.theme.icon_button;
        ctx.fill_rounded_rect(
            self.bounds,
            style.radius,
            if self.hovered {
                style.hover_background
            } else {
                style.background
            },
        );
        let icon_rect =
            Rect {
                origin: Point {
                    x: Dip(self.bounds.origin.x.0
                        + (self.bounds.size.width.0 - style.icon_size.0) / 2.0),
                    y: Dip(self.bounds.origin.y.0
                        + (self.bounds.size.height.0 - style.icon_size.0) / 2.0),
                },
                size: Size {
                    width: style.icon_size,
                    height: style.icon_size,
                },
            };
        build_icon_commands(
            ctx,
            self.icon,
            icon_rect,
            style.foreground,
            style.stroke_width,
        );
    }
}

impl IconButton {
    pub fn new(icon: IconName) -> Self {
        Self {
            id: WidgetId::new(),
            icon,
            bounds: Rect::default(),
            hovered: false,
            theme: Theme::default(),
        }
    }

    pub fn icon(&self) -> IconName {
        self.icon
    }
}

impl Widget for IconButton {
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
            width: self.theme.icon_button.size,
            height: self.theme.icon_button.size,
        });
        self.bounds.size = size;
        size
    }
    fn event(&mut self, event: &UiEvent, ctx: &mut EventContext) -> EventResult {
        if let (InputEvent::CursorMoved { .. }, Some(point)) = (&event.input, event.position) {
            let hovered = self.bounds.contains(point);
            if hovered != self.hovered {
                self.hovered = hovered;
                self.invalidate(ctx, self.bounds);
                return EventResult::RequestRedraw;
            }
        }
        if is_left_press(event)
            && event
                .position
                .is_some_and(|point| self.bounds.contains(point))
        {
            ctx.emit(self.id, ActionKind::Clicked);
            self.invalidate(ctx, self.bounds);
            EventResult::RequestRedraw
        } else {
            EventResult::Ignored
        }
    }
    fn set_theme(&mut self, theme: &Theme) {
        self.theme = theme.clone();
    }
    fn build_render_node(&self, theme: &Theme) -> zui_render::RenderNode {
        build_render_node_with_commands(self.id, self.bounds, theme, |ctx| {
            self.build_render_commands(ctx)
        })
    }
}
