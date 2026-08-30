use crate::{
    event::{is_left_press, ActionKind, EventContext, EventResult, UiEvent},
    layout::Constraints,
    theme::Theme,
    widget::{build_render_node_with_commands, RenderBuildContext, Widget, WidgetId},
    Text,
};
use zui_core::{Dip, Point, Rect, Size};
use zui_render::{RenderNode, Transform};

/// A single radio option.
///
/// Selecting an already selected radio does not clear it. Applications that
/// need group exclusivity can listen for `ActionKind::SelectionChanged` and
/// update the other radios in the group through their retained widget state.
pub struct Radio {
    id: WidgetId,
    label: Text,
    selected: bool,
    bounds: Rect,
    theme: Theme,
}

impl Radio {
    pub fn new(label: impl Into<String>) -> Self {
        let theme = Theme::default();
        let mut label = Text::new(label);
        label.set_font_size(theme.radio.font_size);
        label.set_color(theme.text.color);
        Self {
            id: WidgetId::new(),
            label,
            selected: false,
            bounds: Rect::default(),
            theme,
        }
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn is_selected(&self) -> bool {
        self.selected
    }

    pub fn set_selected(&mut self, selected: bool) {
        self.selected = selected;
    }

    pub fn label(&self) -> &str {
        self.label.text()
    }

    fn build_render_commands(&self, ctx: &mut crate::PaintContext<'_>) {
        let style = &ctx.theme.radio;
        let indicator = Rect {
            origin: Point {
                x: self.bounds.origin.x,
                y: Dip(self.bounds.origin.y.0 + (self.bounds.size.height.0 - style.size.0) / 2.0),
            },
            size: Size {
                width: style.size,
                height: style.size,
            },
        };
        ctx.fill_rounded_rect(
            indicator,
            Dip(style.size.0 / 2.0),
            if self.selected {
                style.selected_background
            } else {
                style.background
            },
        );
        if self.selected {
            let dot_size = Dip((style.size.0 * 0.42).max(2.0));
            ctx.fill_rounded_rect(
                Rect {
                    origin: Point {
                        x: Dip(indicator.origin.x.0 + (style.size.0 - dot_size.0) / 2.0),
                        y: Dip(indicator.origin.y.0 + (style.size.0 - dot_size.0) / 2.0),
                    },
                    size: Size {
                        width: dot_size,
                        height: dot_size,
                    },
                },
                Dip(dot_size.0 / 2.0),
                style.dot,
            );
        }
    }
}

impl Widget for Radio {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn arrange(&mut self, bounds: Rect) {
        self.bounds = bounds;
        let size = self.label.bounds().size;
        let style = &self.theme.radio;
        self.label.arrange(Rect {
            origin: Point {
                x: Dip(bounds.origin.x.0 + style.size.0 + style.gap.0),
                y: Dip(bounds.origin.y.0 + (bounds.size.height.0 - size.height.0) / 2.0),
            },
            size,
        });
    }

    fn measure(&mut self, constraints: Constraints) -> Size {
        let style = &self.theme.radio;
        let label = self.label.measure(Constraints::loose(constraints.max));
        let size = constraints.constrain(Size {
            width: Dip(style.size.0 + style.gap.0 + label.width.0),
            height: Dip(style
                .size
                .0
                .max(zui_render::text_metrics(style.font_size).line_height.0)),
        });
        self.bounds.size = size;
        size
    }

    fn event(&mut self, event: &UiEvent, ctx: &mut EventContext) -> EventResult {
        if is_left_press(event)
            && event
                .position
                .is_some_and(|point| self.bounds.contains(point))
        {
            if self.selected {
                return EventResult::Handled;
            }
            self.selected = true;
            ctx.emit(self.id, ActionKind::SelectionChanged);
            self.invalidate(ctx, self.bounds);
            EventResult::RequestRedraw
        } else {
            EventResult::Ignored
        }
    }

    fn set_theme(&mut self, theme: &Theme) {
        self.theme = theme.clone();
        self.label.set_font_size(theme.radio.font_size);
        self.label.set_color(theme.text.color);
        self.label.set_theme(theme);
    }

    fn build_render_node(&self, theme: &Theme) -> RenderNode {
        let mut node = build_render_node_with_commands(self.id, self.bounds, theme, |ctx| {
            self.build_render_commands(ctx)
        });
        node.add_child(self.label.build_render_node(theme));
        node
    }
    fn build_render_node_incremental(&self, context: &mut RenderBuildContext<'_>) -> RenderNode {
        if !context.subtree_is_dirty(self.id) {
            if let Some(previous) = context.previous() {
                return previous.clone();
            }
        }
        let mut node = context.localize(build_render_node_with_commands(
            self.id,
            self.bounds,
            context.theme(),
            |ctx| self.build_render_commands(ctx),
        ));
        let mut label_context = context.child(
            0,
            Transform::translate(self.bounds.origin.x, self.bounds.origin.y),
        );
        node.add_child(self.label.build_render_node_incremental(&mut label_context));
        node
    }
}
