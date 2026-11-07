use winit::{
    event::{ElementState, Ime, MouseButton, MouseScrollDelta, WindowEvent},
    keyboard::{Key as WinitKey, ModifiersState, NamedKey},
};
use zui_platform::{
    InputEvent, Key, KeyInput, Modifiers, PointerButton, ScrollDelta, TouchPhase, WindowMetrics,
};
use zui_types::Point;

#[derive(Default)]
pub(crate) struct InputState {
    physical_cursor: Option<Point>,
    modifiers: Modifiers,
}

impl InputState {
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn translate(&mut self, event: WindowEvent, metrics: WindowMetrics) -> Option<InputEvent> {
        Some(match event {
            WindowEvent::CursorEntered { .. } => InputEvent::PointerEntered,
            WindowEvent::CursorLeft { .. } => {
                self.physical_cursor = None;
                InputEvent::PointerLeft
            }
            WindowEvent::CursorMoved { position, .. } => {
                let physical = Point::new(position.x, position.y);
                self.physical_cursor = Some(physical);
                InputEvent::PointerMoved(metrics.logical_point(physical))
            }
            WindowEvent::MouseInput { button, state, .. } => InputEvent::PointerButton {
                button: match button {
                    MouseButton::Left => PointerButton::Primary,
                    MouseButton::Right => PointerButton::Secondary,
                    MouseButton::Middle => PointerButton::Middle,
                    MouseButton::Back => PointerButton::Back,
                    MouseButton::Forward => PointerButton::Forward,
                    MouseButton::Other(id) => PointerButton::Other(id),
                },
                pressed: state == ElementState::Pressed,
                position: self
                    .physical_cursor
                    .map(|point| metrics.logical_point(point)),
            },
            WindowEvent::MouseWheel { delta, .. } => InputEvent::Scroll(match delta {
                MouseScrollDelta::LineDelta(x, y) => ScrollDelta::Lines {
                    x: x.into(),
                    y: y.into(),
                },
                MouseScrollDelta::PixelDelta(delta) => ScrollDelta::Pixels {
                    x: delta.x / metrics.scale,
                    y: delta.y / metrics.scale,
                },
            }),
            WindowEvent::Touch(touch) => InputEvent::Touch {
                id: touch.id,
                phase: match touch.phase {
                    winit::event::TouchPhase::Started => TouchPhase::Started,
                    winit::event::TouchPhase::Moved => TouchPhase::Moved,
                    winit::event::TouchPhase::Ended => TouchPhase::Ended,
                    winit::event::TouchPhase::Cancelled => TouchPhase::Cancelled,
                },
                position: metrics.logical_point(Point::new(touch.location.x, touch.location.y)),
            },
            WindowEvent::KeyboardInput {
                event,
                is_synthetic: false,
                ..
            } => InputEvent::Key(KeyInput {
                key: key(event.logical_key),
                pressed: event.state == ElementState::Pressed,
                repeat: event.repeat,
                text: event.text.map(|text| text.to_string()),
                modifiers: self.modifiers,
            }),
            WindowEvent::ModifiersChanged(value) => {
                self.modifiers = modifiers(value.state());
                InputEvent::ModifiersChanged(self.modifiers)
            }
            WindowEvent::Ime(ime) => match ime {
                Ime::Enabled => InputEvent::ImeEnabled,
                Ime::Preedit(text, cursor) => InputEvent::ImePreedit { text, cursor },
                Ime::Commit(text) => InputEvent::ImeCommit(text),
                Ime::Disabled => InputEvent::ImeDisabled,
            },
            _ => return None,
        })
    }
}

fn modifiers(value: ModifiersState) -> Modifiers {
    Modifiers {
        shift: value.shift_key(),
        control: value.control_key(),
        alt: value.alt_key(),
        super_key: value.super_key(),
    }
}

fn key(value: WinitKey) -> Key {
    match value {
        WinitKey::Character(value) => Key::Character(value.to_string()),
        WinitKey::Named(value) => match value {
            NamedKey::Enter => Key::Enter,
            NamedKey::Escape => Key::Escape,
            NamedKey::Tab => Key::Tab,
            NamedKey::Backspace => Key::Backspace,
            NamedKey::Delete => Key::Delete,
            NamedKey::Space => Key::Space,
            NamedKey::ArrowLeft => Key::ArrowLeft,
            NamedKey::ArrowRight => Key::ArrowRight,
            NamedKey::ArrowUp => Key::ArrowUp,
            NamedKey::ArrowDown => Key::ArrowDown,
            NamedKey::Home => Key::Home,
            NamedKey::End => Key::End,
            NamedKey::PageUp => Key::PageUp,
            NamedKey::PageDown => Key::PageDown,
            NamedKey::Insert => Key::Insert,
            NamedKey::F1 => Key::Function(1),
            NamedKey::F2 => Key::Function(2),
            NamedKey::F3 => Key::Function(3),
            NamedKey::F4 => Key::Function(4),
            NamedKey::F5 => Key::Function(5),
            NamedKey::F6 => Key::Function(6),
            NamedKey::F7 => Key::Function(7),
            NamedKey::F8 => Key::Function(8),
            NamedKey::F9 => Key::Function(9),
            NamedKey::F10 => Key::Function(10),
            NamedKey::F11 => Key::Function(11),
            NamedKey::F12 => Key::Function(12),
            _ => Key::Unidentified,
        },
        _ => Key::Unidentified,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use winit::{
        dpi::PhysicalPosition,
        event::{DeviceId, Touch},
    };
    use zui_types::Extent;

    fn metrics(scale: f64) -> WindowMetrics {
        WindowMetrics {
            extent: Extent::new(800, 600),
            scale,
            occluded: false,
        }
    }

    #[test]
    fn pointer_coordinates_and_clicks_follow_scale_changes() {
        let mut state = InputState::default();
        let id = DeviceId::dummy();
        let moved = WindowEvent::CursorMoved {
            device_id: id,
            position: PhysicalPosition::new(120.0, 80.0),
        };
        assert_eq!(
            state.translate(moved, metrics(2.0)),
            Some(InputEvent::PointerMoved(Point::new(60.0, 40.0)))
        );
        let click = WindowEvent::MouseInput {
            device_id: id,
            state: ElementState::Pressed,
            button: MouseButton::Left,
        };
        assert_eq!(
            state.translate(click, metrics(1.0)),
            Some(InputEvent::PointerButton {
                button: PointerButton::Primary,
                pressed: true,
                position: Some(Point::new(120.0, 80.0))
            })
        );
    }

    #[test]
    fn ime_and_touch_are_preserved_without_keyboard_conversion() {
        let mut state = InputState::default();
        assert_eq!(
            state.translate(
                WindowEvent::Ime(Ime::Preedit("中文".into(), Some((3, 6)))),
                metrics(2.0)
            ),
            Some(InputEvent::ImePreedit {
                text: "中文".into(),
                cursor: Some((3, 6))
            })
        );
        let touch = Touch {
            device_id: DeviceId::dummy(),
            phase: winit::event::TouchPhase::Moved,
            location: PhysicalPosition::new(20.0, 40.0),
            force: None,
            id: 7,
        };
        assert_eq!(
            state.translate(WindowEvent::Touch(touch), metrics(2.0)),
            Some(InputEvent::Touch {
                id: 7,
                phase: TouchPhase::Moved,
                position: Point::new(10.0, 20.0)
            })
        );
    }
}
