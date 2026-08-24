//! Platform-independent widgets, layout, events, focus and semantics.

pub mod event;
pub mod focus;
pub mod layout;
pub mod semantics;
pub mod theme;
pub mod tree;
pub mod widget;
pub mod widgets;

pub use event::{Action, ActionKind, EventContext, EventResult, UiEvent};
pub use focus::FocusManager;
pub use layout::{Constraints, LayoutBox};
pub use semantics::{SemanticRole, SemanticsNode};
pub use theme::{ButtonStyle, TextInputStyle, TextStyle, Theme, ThemeToken};
pub use tree::WidgetTree;
pub use widget::{PaintContext, Widget, WidgetId};
pub use widgets::{Button, Column, Padding, Row, Text, TextInput};

#[cfg(test)]
mod tests {
    use super::*;
    use zui_core::{Dip, Point, Rect, Size};
    use zui_platform::{InputEvent, KeyState, MouseButton};

    #[test]
    fn row_lays_out_children_with_spacing() {
        let mut row = Row::new(vec![Box::new(Text::new("A")), Box::new(Text::new("BB"))])
            .with_spacing(Dip(4.0));
        let size = row.layout(Constraints::loose(Size {
            width: Dip(200.0),
            height: Dip(100.0),
        }));
        assert_eq!(size.width, Dip(28.0));
        assert_eq!(row.children()[1].bounds().origin.x, Dip(12.0));
    }

    #[test]
    fn button_emits_clicked_action_for_left_pointer_press() {
        let mut button = Button::new("OK");
        button.set_bounds(Rect {
            origin: Point {
                x: Dip(10.0),
                y: Dip(10.0),
            },
            size: Size {
                width: Dip(60.0),
                height: Dip(30.0),
            },
        });
        let event = UiEvent::pointer(
            None,
            Point {
                x: Dip(20.0),
                y: Dip(20.0),
            },
            InputEvent::MouseInput {
                button: MouseButton::Left,
                state: KeyState::Pressed,
            },
        );
        let mut context = EventContext::new();
        assert_eq!(
            button.event(&event, &mut context),
            EventResult::RequestRedraw
        );
        assert_eq!(context.actions()[0].kind, ActionKind::Clicked);
    }

    #[test]
    fn text_input_appends_text_when_focused() {
        let mut input = TextInput::new();
        input.set_bounds(Rect {
            origin: Point {
                x: Dip(0.0),
                y: Dip(0.0),
            },
            size: Size {
                width: Dip(200.0),
                height: Dip(32.0),
            },
        });
        let focus = UiEvent::pointer(
            None,
            Point {
                x: Dip(2.0),
                y: Dip(2.0),
            },
            InputEvent::MouseInput {
                button: MouseButton::Left,
                state: KeyState::Pressed,
            },
        );
        let text = UiEvent::input(InputEvent::Text("hi".into()));
        let mut context = EventContext::new();
        input.event(&focus, &mut context);
        input.event(&text, &mut context);
        assert_eq!(input.text(), "hi");
    }

    #[test]
    fn text_input_accepts_character_key_events() {
        let mut input = TextInput::new();
        input.set_bounds(Rect {
            origin: Point {
                x: Dip(0.0),
                y: Dip(0.0),
            },
            size: Size {
                width: Dip(200.0),
                height: Dip(32.0),
            },
        });
        let focus = UiEvent::pointer(
            None,
            Point {
                x: Dip(2.0),
                y: Dip(2.0),
            },
            InputEvent::MouseInput {
                button: MouseButton::Left,
                state: KeyState::Pressed,
            },
        );
        let key = UiEvent::input(InputEvent::Keyboard {
            key: zui_platform::KeyCode::Character('a'),
            state: KeyState::Pressed,
            modifiers: Default::default(),
        });
        let mut context = EventContext::new();
        input.event(&focus, &mut context);
        input.event(&key, &mut context);
        assert_eq!(input.text(), "a");
    }

    #[test]
    fn padding_offsets_child_before_layout() {
        let mut padding = Padding::new(Text::new("x"), Dip(16.0));
        padding.layout(Constraints::loose(Size {
            width: Dip(200.0),
            height: Dip(100.0),
        }));
        assert_eq!(
            padding.child().bounds().origin,
            Point {
                x: Dip(16.0),
                y: Dip(16.0)
            }
        );
    }
}
