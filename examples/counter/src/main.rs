use zui_app::Application;
use zui_ui::{button, column, text, Component, ComponentCtx, Theme, View};

struct Counter {
    count: u32,
}

#[derive(Clone)]
enum CounterMessage {
    Increment,
}

impl Component for Counter {
    type Message = CounterMessage;

    fn update(&mut self, message: Self::Message, _cx: &mut ComponentCtx) {
        match message {
            CounterMessage::Increment => self.count += 1,
        }
    }

    fn view(&self) -> Box<dyn View<Self::Message>> {
        Box::new(
            column(vec![
                Box::new(button("增加计数").on_click(CounterMessage::Increment)),
                Box::new(text(format!("计数：{}", self.count))),
            ])
            .spacing(8.0),
        )
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let theme = Theme::from_toml_str(include_str!("../../widgets/theme.toml"))?;
    Application::new()
        .title("zero-ui counter example")
        .theme(theme)
        .run_component(Counter { count: 0 })
}

#[cfg(test)]
mod tests {
    use super::*;
    use zui_core::{Dip, Point, Size};
    use zui_platform::{InputEvent, KeyState, MouseButton};
    use zui_ui::{ComponentRoot, Constraints, EventResult, UiEvent, WidgetTree};

    fn contains_text(node: &zui_render::RenderNode, expected: &str) -> bool {
        node.commands.iter().any(|command| {
            matches!(command, zui_render::PaintCommand::Text { text, .. } if text == expected)
        }) || node.children.iter().any(|child| contains_text(child, expected))
    }

    #[test]
    fn click_rebuilds_the_component_view() {
        let mut tree = WidgetTree::new(ComponentRoot::new(Counter { count: 0 }));
        tree.layout(Constraints::loose(Size {
            width: Dip(300.0),
            height: Dip(160.0),
        }));
        tree.render_node_cached();
        tree.mark_clean();

        let (result, _) = tree.event(&UiEvent::pointer(
            None,
            Point {
                x: Dip(8.0),
                y: Dip(8.0),
            },
            InputEvent::MouseInput {
                button: MouseButton::Left,
                state: KeyState::Pressed,
            },
        ));

        assert_eq!(result, EventResult::RequestRedraw);
        assert!(contains_text(tree.render_node_cached(), "计数：1"));
    }
}
