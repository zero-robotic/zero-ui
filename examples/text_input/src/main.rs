use zui_app::Application;
use zui_backend_winit::WinitBackend;
use zui_core::Dip;
use zui_render_runtime::ActiveRenderer;
use zui_ui::{ColumnLayout, Layout, Padding, Text, TextInput};

fn text_input_demo() -> Padding {
    let content = Layout::new(ColumnLayout::new().spacing(Dip(12.0)))
        .child(Text::new("TextInput / IME 验收"))
        .child(Text::new(
            "点击输入框后输入文字；组合文本应在 commit 前显示，取消后消失。",
        ))
        .child(TextInput::new())
        .child(Text::new("第二输入框用于验证点击切换焦点。"))
        .child(TextInput::with_text("已有文本"));
    Padding::new(content, Dip(24.0))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let backend = WinitBackend::new()?;
    let renderer = ActiveRenderer::new_blocking()?;
    Application::new()
        .title("zero-ui TextInput / IME")
        .run_with(backend, renderer, text_input_demo())
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use zui_core::{Point, Rect, Size};
    use zui_platform::{ImeEvent, InputEvent, KeyCode, KeyState, Modifiers, MouseButton};
    use zui_ui::{EventContext, EventResult, UiEvent, Widget};

    use super::*;

    #[test]
    fn public_events_cover_the_manual_ime_acceptance_path() {
        let mut input = TextInput::new();
        input.set_bounds(Rect {
            origin: Point::default(),
            size: Size {
                width: Dip(280.0),
                height: Dip(40.0),
            },
        });
        let mut context = EventContext::new();

        assert_eq!(
            input.event(
                &UiEvent::pointer(
                    None,
                    Point {
                        x: Dip(10.0),
                        y: Dip(10.0),
                    },
                    InputEvent::MouseInput {
                        button: MouseButton::Left,
                        state: KeyState::Pressed,
                    },
                ),
                &mut context,
            ),
            EventResult::RequestRedraw
        );
        input.event(
            &UiEvent::input(InputEvent::Keyboard {
                key: KeyCode::Character('a'),
                state: KeyState::Pressed,
                modifiers: Modifiers::default(),
            }),
            &mut context,
        );
        assert_eq!(input.text(), "a");

        input.event(
            &UiEvent::input(InputEvent::Ime(ImeEvent::Preedit {
                text: "ni hao".into(),
                selection: None,
            })),
            &mut context,
        );
        assert_eq!(input.preedit(), Some(("ni hao", None)));
        assert_eq!(input.text(), "a");

        input.event(
            &UiEvent::input(InputEvent::Ime(ImeEvent::Commit("你好".into()))),
            &mut context,
        );
        assert_eq!(input.preedit(), None);
        assert_eq!(input.text(), "a你好");

        input.event(
            &UiEvent::input(InputEvent::Ime(ImeEvent::Preedit {
                text: "qu xiao".into(),
                selection: Some((0, 2)),
            })),
            &mut context,
        );
        input.event(
            &UiEvent::input(InputEvent::Ime(ImeEvent::Cancelled)),
            &mut context,
        );
        assert_eq!(input.preedit(), None);
        assert_eq!(input.text(), "a你好");
    }
}
