use std::cell::RefCell;
use std::time::Instant;

use zui_backend_winit::WinitBackend;
use zui_core::{Dip, PhysicalSize, Point, Rect, Size};
use zui_platform::{Host, InputEvent, PlatformEvent, WindowOptions};
use zui_render::RenderError;
use zui_render_runtime::{ActiveRenderer, RendererError};
use zui_ui::{
    ActionKind, Button, Constraints, EventContext, EventResult, Padding, RenderBuildContext, Text,
    Theme, UiEvent, Widget, WidgetId, WidgetTree,
};

/// A small application-level composition demonstrating Button -> Text data
/// flow. The Button emits `Clicked`; the composition updates its state and
/// Text, then invalidates the retained subtree.
struct CounterPanel {
    id: WidgetId,
    button: Button,
    value: Text,
    count: u32,
    bounds: Rect,
}

impl CounterPanel {
    fn new() -> Self {
        Self {
            id: WidgetId::new(),
            button: Button::new("增加计数"),
            value: Text::new("计数：0"),
            count: 0,
            bounds: Rect::default(),
        }
    }
}

impl Widget for CounterPanel {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn measure(&mut self, constraints: Constraints) -> Size {
        let button_size = self.button.measure(constraints);
        let value_size = self.value.measure(constraints);
        let size = constraints.constrain(Size {
            width: Dip(button_size.width.0.max(value_size.width.0)),
            height: Dip(button_size.height.0 + 8.0 + value_size.height.0),
        });
        self.bounds.size = size;
        size
    }

    fn arrange(&mut self, bounds: Rect) {
        self.bounds = bounds;
        let button_size = self.button.bounds().size;
        self.button.arrange(Rect {
            origin: bounds.origin,
            size: button_size,
        });
        self.value.arrange(Rect {
            origin: Point {
                x: bounds.origin.x,
                y: Dip(bounds.origin.y.0 + button_size.height.0 + 8.0),
            },
            size: self.value.bounds().size,
        });
    }

    fn event(&mut self, event: &UiEvent, ctx: &mut EventContext) -> EventResult {
        let result = self.button.event(event, ctx);
        let clicked = ctx
            .actions()
            .iter()
            .any(|action| action.source == self.button.id() && action.kind == ActionKind::Clicked);
        if clicked {
            self.count += 1;
            self.value.set_text(format!("计数：{}", self.count));
            self.value.measure(Constraints::loose(self.bounds.size));
            self.arrange(self.bounds);
            // The button is already invalidated by Button::event(). Mark the
            // changed leaf and its composition parent so incremental
            // building does not reuse the old Text RenderNode.
            self.invalidate(ctx, self.bounds);
            self.value.invalidate(ctx, self.value.bounds());
            EventResult::RequestRedraw
        } else {
            result
        }
    }

    fn set_theme(&mut self, theme: &Theme) {
        self.button.set_theme(theme);
        self.value.set_theme(theme);
    }

    fn build_render_node(&self, theme: &Theme) -> zui_render::RenderNode {
        let mut node = zui_render::RenderNode::for_widget(self.bounds);
        node.set_id(self.id.value());
        node.add_child(self.button.build_render_node(theme));
        node.add_child(self.value.build_render_node(theme));
        node
    }

    fn build_render_node_incremental(
        &self,
        context: &mut RenderBuildContext<'_>,
    ) -> zui_render::RenderNode {
        if !context.subtree_is_dirty(self.id) {
            if let Some(previous) = context.previous() {
                return previous.clone();
            }
        }

        let mut node = context.localize(zui_render::RenderNode::for_widget(self.bounds));
        node.set_id(self.id.value());
        let child_world_transform =
            zui_render::Transform::translate(self.bounds.origin.x, self.bounds.origin.y);
        let mut button_context = context.child(0, child_world_transform);
        let mut value_context = context.child(1, child_world_transform);
        node.add_child(
            self.button
                .build_render_node_incremental(&mut button_context),
        );
        node.add_child(self.value.build_render_node_incremental(&mut value_context));
        node
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let renderer = RefCell::new(ActiveRenderer::new_blocking()?);
    let mut tree = WidgetTree::new(Padding::new(CounterPanel::new(), Dip(24.0)));
    let theme = Theme::from_toml_str(include_str!("../../empty_window/theme.toml"))?;
    let background = theme.background;
    tree.set_theme(theme);
    let ui = RefCell::new(tree);
    let pointer_position = RefCell::new(Point {
        x: Dip(0.0),
        y: Dip(0.0),
    });

    let mut on_window = |host: &zui_backend_winit::WinitHost| {
        let size = host.scale_factor().to_physical(host.size());
        let mut tree = ui.borrow_mut();
        tree.layout(Constraints::loose(host.size()));
        tree.request_paint(None);
        drop(tree);
        renderer
            .borrow_mut()
            .attach_surface(host.id(), host, size, host.scale_factor())
            .expect("failed to attach render surface");
        host.request_redraw()
            .expect("failed to request first redraw");
    };

    let mut handler = |event: PlatformEvent| match event {
        PlatformEvent::RedrawRequested(window) => {
            if !ui.borrow().needs_redraw() {
                if ui.borrow().next_redraw().is_some() {
                    ui.borrow_mut().request_paint(None);
                } else {
                    return None;
                }
            }
            let (scene_update, node, render_index) = {
                let mut tree = ui.borrow_mut();
                let scene_update = tree.scene_update().clone();
                let node = tree.render_node_cached().clone();
                let render_index = tree.render_index().clone();
                (scene_update, node, render_index)
            };
            if let Err(error) = renderer.borrow_mut().render_scene(
                window,
                &node,
                background,
                &render_index,
                &scene_update,
            ) {
                match error {
                    RendererError::HardwareGpu(RenderError::SurfaceLost) => {
                        eprintln!("render surface lost; waiting for resize")
                    }
                    error => eprintln!("failed to render frame: {error}"),
                }
            } else {
                ui.borrow_mut().mark_clean();
            }
            ui.borrow().next_redraw()
        }
        PlatformEvent::WindowResized {
            window,
            size,
            scale_factor,
        } => {
            let physical = scale_factor.to_physical(size);
            if physical != PhysicalSize::default() {
                ui.borrow_mut().layout(Constraints::loose(size));
                renderer
                    .borrow_mut()
                    .resize(window, physical, scale_factor)
                    .expect("failed to resize render surface");
            }
            Some(Instant::now())
        }
        PlatformEvent::Input { window, event } => {
            if let InputEvent::CursorMoved { position } = &event {
                *pointer_position.borrow_mut() = *position;
            }
            let ui_event = match event {
                InputEvent::CursorMoved { position } => {
                    UiEvent::pointer(Some(window), position, event)
                }
                InputEvent::MouseInput { .. } => {
                    UiEvent::pointer(Some(window), *pointer_position.borrow(), event)
                }
                _ => UiEvent::input(event),
            };
            let (result, actions) = ui.borrow_mut().event(&ui_event);
            for action in actions {
                println!("UI action: {:?}", action);
            }
            (result == EventResult::RequestRedraw).then(Instant::now)
        }
        PlatformEvent::CloseRequested(window) => {
            renderer.borrow_mut().detach_surface(window);
            None
        }
        _ => None,
    };

    WinitBackend::new()?.run_with_options_and_schedule(
        WindowOptions {
            title: "zero-ui counter example".into(),
            ..WindowOptions::default()
        },
        &mut on_window,
        &mut handler,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use zui_platform::{KeyState, MouseButton};

    fn contains_text(node: &zui_render::RenderNode, expected: &str) -> bool {
        node.commands.iter().any(|command| {
            matches!(command, zui_render::PaintCommand::Text { text, .. } if text == expected)
        }) || node.children.iter().any(|child| contains_text(child, expected))
    }

    #[test]
    fn click_updates_the_retained_text_node() {
        let mut tree = WidgetTree::new(Padding::new(CounterPanel::new(), Dip(24.0)));
        tree.layout(Constraints::loose(Size {
            width: Dip(300.0),
            height: Dip(160.0),
        }));
        tree.render_node_cached();
        tree.mark_clean();

        let (result, actions) = tree.event(&UiEvent::pointer(
            None,
            Point {
                x: Dip(30.0),
                y: Dip(30.0),
            },
            InputEvent::MouseInput {
                button: MouseButton::Left,
                state: KeyState::Pressed,
            },
        ));

        assert_eq!(result, EventResult::RequestRedraw);
        assert!(actions
            .iter()
            .any(|action| action.kind == ActionKind::Clicked));
        assert!(contains_text(tree.render_node_cached(), "计数：1"));
    }
}
