//! Declarative components layered on top of the retained widget runtime.
//!
//! Views are intentionally lightweight descriptions. A component owns
//! application-local state; the runtime owns the retained widgets created by
//! the current view. This first layer keeps rebuilding explicit and provides
//! stable output routing without exposing renderer details to applications.

use std::collections::HashMap;

use crate::{
    ActionKind, Button, Constraints, EventContext, EventResult, RenderBuildContext, Theme, UiEvent,
    Widget, WidgetId,
};
use zui_core::{Rect, Size};
use zui_render::{RenderNode, Transform};

/// Application-local state which can produce a declarative [`View`].
pub trait Component: 'static {
    type Message: Clone + 'static;

    fn update(&mut self, message: Self::Message, cx: &mut ComponentCtx);
    fn view(&self) -> Box<dyn View<Self::Message>>;
}

/// Context reserved for component-side requests such as a future command or
/// task scheduler. Keeping it explicit makes `update` extensible without
/// widening the `Component` trait later.
#[derive(Default)]
pub struct ComponentCtx {
    rebuild_requested: bool,
}

impl ComponentCtx {
    pub fn request_rebuild(&mut self) {
        self.rebuild_requested = true;
    }
}

/// A lightweight description that creates retained widgets and registers
/// their outputs with the owning component.
pub trait View<M>: 'static {
    fn build(&self, cx: &mut ViewCtx<M>) -> Box<dyn Widget>;
}

/// Build context used only while materializing a view.
pub struct ViewCtx<M> {
    click_messages: HashMap<WidgetId, M>,
}

impl<M> Default for ViewCtx<M> {
    fn default() -> Self {
        Self {
            click_messages: HashMap::new(),
        }
    }
}

impl<M> ViewCtx<M> {
    pub fn on_click(&mut self, id: WidgetId, message: M) {
        self.click_messages.insert(id, message);
    }
}

/// A declarative button. Its output is routed as a typed component message.
pub struct ButtonView<M> {
    label: String,
    on_click: Option<M>,
}

pub fn button<M>(label: impl Into<String>) -> ButtonView<M> {
    ButtonView {
        label: label.into(),
        on_click: None,
    }
}

impl<M> ButtonView<M> {
    pub fn on_click(mut self, message: M) -> Self {
        self.on_click = Some(message);
        self
    }
}

impl<M: Clone + 'static> View<M> for ButtonView<M> {
    fn build(&self, cx: &mut ViewCtx<M>) -> Box<dyn Widget> {
        let button = Button::new(self.label.clone());
        if let Some(message) = &self.on_click {
            cx.on_click(button.id(), message.clone());
        }
        Box::new(button)
    }
}

/// A declarative text node.
pub struct TextView {
    value: String,
}

pub fn text(value: impl Into<String>) -> TextView {
    TextView {
        value: value.into(),
    }
}

impl<M: 'static> View<M> for TextView {
    fn build(&self, _cx: &mut ViewCtx<M>) -> Box<dyn Widget> {
        Box::new(crate::Text::new(self.value.clone()))
    }
}

/// A vertical view composition. Child order is retained in the resulting
/// layout, which is also the identity order used by the retained tree.
pub struct ColumnView<M> {
    children: Vec<Box<dyn View<M>>>,
    spacing: f32,
}

pub fn column<M: 'static>(children: Vec<Box<dyn View<M>>>) -> ColumnView<M> {
    ColumnView {
        children,
        spacing: 8.0,
    }
}

impl<M> ColumnView<M> {
    pub fn spacing(mut self, spacing: f32) -> Self {
        self.spacing = spacing;
        self
    }
}

impl<M: 'static> View<M> for ColumnView<M> {
    fn build(&self, cx: &mut ViewCtx<M>) -> Box<dyn Widget> {
        let mut layout =
            crate::Layout::new(crate::ColumnLayout::new().spacing(zui_core::Dip(self.spacing)));
        for child in &self.children {
            layout = layout.child_box(child.build(cx));
        }
        Box::new(layout)
    }
}

/// Retained adapter that owns a component and the widgets built from its
/// latest view. Applications put this widget directly into a [`WidgetTree`].
pub struct ComponentRoot<C: Component> {
    id: WidgetId,
    component: C,
    child: Box<dyn Widget>,
    click_messages: HashMap<WidgetId, C::Message>,
    bounds: Rect,
    theme: Theme,
}

impl<C: Component> ComponentRoot<C> {
    pub fn new(component: C) -> Self {
        let mut root = Self {
            id: WidgetId::new(),
            component,
            child: Box::new(crate::Gap::vertical(0.0)),
            click_messages: HashMap::new(),
            bounds: Rect::default(),
            theme: Theme::default(),
        };
        root.rebuild();
        root
    }

    pub fn component(&self) -> &C {
        &self.component
    }

    pub fn component_mut(&mut self) -> &mut C {
        &mut self.component
    }

    fn rebuild(&mut self) {
        let mut cx = ViewCtx::default();
        self.child = self.component.view().build(&mut cx);
        self.child.set_theme(&self.theme);
        self.click_messages = cx.click_messages;
    }
}

impl<C: Component> Widget for ComponentRoot<C> {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn measure(&mut self, constraints: Constraints) -> Size {
        let size = self.child.measure(constraints);
        self.bounds.size = size;
        size
    }

    fn arrange(&mut self, bounds: Rect) {
        self.bounds = bounds;
        self.child.arrange(bounds);
    }

    fn next_redraw(&self) -> Option<std::time::Instant> {
        self.child.next_redraw()
    }

    fn event(&mut self, event: &UiEvent, ctx: &mut EventContext) -> EventResult {
        let first_new_action = ctx.actions().len();
        let result = self.child.event(event, ctx);
        let messages = ctx.actions()[first_new_action..]
            .iter()
            .filter(|action| action.kind == ActionKind::Clicked)
            .filter_map(|action| self.click_messages.get(&action.source).cloned())
            .collect::<Vec<_>>();

        if messages.is_empty() {
            return result;
        }

        let mut component_cx = ComponentCtx::default();
        for message in messages {
            self.component.update(message, &mut component_cx);
        }
        self.rebuild();
        self.child.measure(Constraints::loose(self.bounds.size));
        self.child.arrange(self.bounds);
        self.invalidate(ctx, self.bounds);
        EventResult::RequestRedraw
    }

    fn set_theme(&mut self, theme: &Theme) {
        self.theme = theme.clone();
        self.child.set_theme(theme);
    }

    fn build_render_node(&self, theme: &Theme) -> RenderNode {
        let mut node = RenderNode::for_widget(self.bounds);
        node.set_id(self.id.value());
        node.add_child(self.child.build_render_node(theme));
        node
    }

    fn build_render_node_incremental(&self, context: &mut RenderBuildContext<'_>) -> RenderNode {
        if !context.subtree_is_dirty(self.id) {
            if let Some(previous) = context.previous() {
                return previous.clone();
            }
        }
        let mut node = context.localize(RenderNode::for_widget(self.bounds));
        node.set_id(self.id.value());
        let child_context = context.child(
            0,
            Transform::translate(self.bounds.origin.x, self.bounds.origin.y),
        );
        let mut child_context = child_context.force_rebuild_subtree();
        node.add_child(self.child.build_render_node_incremental(&mut child_context));
        node
    }
}
