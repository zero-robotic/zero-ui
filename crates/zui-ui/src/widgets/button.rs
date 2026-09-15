use crate::{
    event::{is_left_press, ActionKind, EventContext, EventResult, UiEvent},
    layout::Constraints,
    theme::Theme,
    widget::{build_render_node_with_commands, RenderBuildContext, Widget, WidgetId},
    Text,
};
use zui_core::{Dip, Point, Rect, Size};
use zui_platform::InputEvent;
use zui_render::{RenderNode, Transform};

pub struct Button {
    id: WidgetId,
    label: Text,
    bounds: Rect,
    theme: Theme,
}
impl Button {
    pub fn new(label: impl Into<String>) -> Self {
        let theme = Theme::default();
        let mut text = Text::new(label);
        text.set_font_size(theme.button.font_size);
        text.set_color(theme.button.foreground);
        Self {
            id: WidgetId::new(),
            label: text,
            bounds: Rect::default(),
            theme,
        }
    }
    pub fn label(&self) -> &str {
        self.label.text()
    }
    fn place_label(&mut self) {
        let style = &self.theme.button;
        let size = self.label.bounds().size;
        self.label.arrange(Rect {
            origin: Point {
                x: Dip(self.bounds.origin.x.0 + style.padding_x.0),
                y: Dip(self.bounds.origin.y.0 + (self.bounds.size.height.0 - size.height.0) / 2.0),
            },
            size,
        });
    }
    fn background(&self, ctx: &mut crate::PaintContext<'_>, hovered: bool) {
        let style = &ctx.theme.button;
        ctx.fill_rounded_rect(
            self.bounds,
            style.radius,
            if hovered {
                style.hover_background
            } else {
                style.background
            },
        );
    }
}
impl Widget for Button {
    fn id(&self) -> WidgetId {
        self.id
    }
    fn bounds(&self) -> Rect {
        self.bounds
    }
    fn arrange(&mut self, bounds: Rect) {
        self.bounds = bounds;
        self.place_label();
    }
    fn measure(&mut self, constraints: Constraints) -> Size {
        let label = self.label.measure(Constraints::loose(constraints.max));
        let size = constraints.constrain(Size {
            width: Dip(label.width.0 + self.theme.button.padding_x.0 * 2.0),
            height: self.theme.button.height,
        });
        self.bounds.size = size;
        size
    }
    fn next_redraw(&self) -> Option<std::time::Instant> {
        self.label.next_redraw()
    }
    fn event(&mut self, event: &UiEvent, ctx: &mut EventContext) -> EventResult {
        if let (InputEvent::CursorMoved { .. }, Some(point)) = (&event.input, event.position) {
            if ctx.set_hovered(self.id, self.bounds.contains(point)) {
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
        self.label.set_font_size(theme.button.font_size);
        self.label.set_color(theme.button.foreground);
        self.label.set_theme(theme);
    }
    fn build_render_node(&self, theme: &Theme) -> RenderNode {
        let mut node = build_render_node_with_commands(self.id, self.bounds, theme, |ctx| {
            self.background(ctx, false)
        });
        node.add_child(self.label.build_render_node(theme));
        node
    }
    fn semantics(&self) -> crate::SemanticsNode {
        crate::SemanticsNode::new(self.id, crate::SemanticRole::Button, self.label())
            .bounds(self.bounds)
    }
    fn build_render_node_incremental(&self, context: &mut RenderBuildContext<'_>) -> RenderNode {
        if !context.subtree_is_dirty(self.id) {
            if let Some(previous) = context.previous() {
                return previous.clone();
            }
        }
        let hovered = context.runtime().is_hovered(self.id);
        let mut node = context.localize(build_render_node_with_commands(
            self.id,
            self.bounds,
            context.theme(),
            |ctx| self.background(ctx, hovered),
        ));
        let mut label_context = context.child(
            0,
            Transform::translate(self.bounds.origin.x, self.bounds.origin.y),
        );
        node.add_child(self.label.build_render_node_incremental(&mut label_context));
        node
    }
}
