use crate::{
    event::{is_left_press, ActionKind, EventContext, EventResult, UiEvent},
    layout::Constraints,
    theme::Theme,
    widget::{RenderBuildContext, Widget, WidgetId},
    Text,
};
use zui_core::{Rect, Size};
use zui_render::{RenderNode, Transform};

/// Text that names another control.
///
/// A label delegates text layout and painting to its internal [`Text`]. When
/// associated with a target through [`Label::for_widget`], clicking it moves
/// keyboard focus to that target.
pub struct Label {
    id: WidgetId,
    text: Text,
    target: Option<WidgetId>,
    bounds: Rect,
}

impl Label {
    pub fn new(text: impl Into<String>) -> Self {
        Self::with_text(Text::new(text))
    }

    /// Creates a label from a fully configured text widget.
    pub fn with_text(text: Text) -> Self {
        Self {
            id: WidgetId::new(),
            text,
            target: None,
            bounds: Rect::default(),
        }
    }

    /// Associates this label with the control that it names.
    pub fn for_widget(mut self, target: WidgetId) -> Self {
        self.target = Some(target);
        self
    }

    pub fn target(&self) -> Option<WidgetId> {
        self.target
    }

    pub fn text(&self) -> &Text {
        &self.text
    }
}

impl Widget for Label {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn measure(&mut self, constraints: Constraints) -> Size {
        let size = self.text.measure(constraints);
        self.bounds.size = size;
        size
    }

    fn arrange(&mut self, bounds: Rect) {
        self.bounds = bounds;
        self.text.arrange(bounds);
    }

    fn next_redraw(&self) -> Option<std::time::Instant> {
        self.text.next_redraw()
    }

    fn event(&mut self, event: &UiEvent, ctx: &mut EventContext) -> EventResult {
        if is_left_press(event)
            && event
                .position
                .is_some_and(|point| self.bounds.contains(point))
        {
            if let Some(target) = self.target {
                if ctx.request_focus(target) {
                    ctx.emit(target, ActionKind::FocusRequested);
                }
                self.invalidate(ctx, self.bounds);
                return EventResult::RequestRedraw;
            }
        }
        EventResult::Ignored
    }

    fn set_theme(&mut self, theme: &Theme) {
        self.text.set_theme(theme);
    }

    fn build_render_node(&self, theme: &Theme) -> RenderNode {
        let mut node = RenderNode::for_widget(self.bounds);
        node.set_id(self.id.0);
        node.add_child(self.text.build_render_node(theme));
        node
    }

    fn build_render_node_incremental(&self, context: &mut RenderBuildContext<'_>) -> RenderNode {
        if !context.subtree_is_dirty(self.id) {
            if let Some(previous) = context.previous() {
                return previous.clone();
            }
        }
        let mut node = context.localize(RenderNode::for_widget(self.bounds));
        node.set_id(self.id.0);
        let mut text_context = context.child(
            0,
            Transform::translate(self.bounds.origin.x, self.bounds.origin.y),
        );
        node.add_child(self.text.build_render_node_incremental(&mut text_context));
        node
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WidgetRuntimeTable;
    use zui_core::{Dip, Point};
    use zui_platform::{InputEvent, KeyState, MouseButton};

    #[test]
    fn clicking_an_associated_label_focuses_its_target() {
        let target = WidgetId::new();
        let mut label = Label::new("Email").for_widget(target);
        label.arrange(Rect {
            origin: Point::default(),
            size: Size {
                width: Dip(80.0),
                height: Dip(24.0),
            },
        });
        let event = UiEvent::pointer(
            None,
            Point {
                x: Dip(10.0),
                y: Dip(10.0),
            },
            InputEvent::MouseInput {
                button: MouseButton::Left,
                state: KeyState::Pressed,
            },
        );
        let mut runtime = WidgetRuntimeTable::default();
        let mut context = EventContext::with_runtime(&mut runtime);

        assert_eq!(
            label.event(&event, &mut context),
            EventResult::RequestRedraw
        );
        assert!(context.is_focused(target));
        assert_eq!(context.actions()[0].source, target);
        assert_eq!(context.actions()[0].kind, ActionKind::FocusRequested);
    }
}
