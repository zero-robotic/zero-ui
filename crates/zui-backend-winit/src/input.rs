use std::collections::HashMap;
use winit::{
    event::{
        ButtonSource, ElementState, FingerId, Ime, MouseButton, MouseScrollDelta, PointerKind,
        PointerSource, WindowEvent,
    },
    keyboard::{Key as WinitKey, ModifiersState, NamedKey},
};
use zui_platform::{
    InputEvent, Key, KeyInput, Modifiers, PointerButton, ScrollDelta, TouchPhase, WindowMetrics,
};
use zui_types::Point;

#[derive(Default)]
pub(crate) struct InputState {
    touches: HashMap<FingerId, Point>,
    modifiers: Modifiers,
}

impl InputState {
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn translate(&mut self, event: WindowEvent, metrics: WindowMetrics) -> Option<InputEvent> {
        Some(match event {
            WindowEvent::PointerEntered {
                kind: PointerKind::Touch(_),
                ..
            } => return None,
            WindowEvent::PointerLeft {
                kind: PointerKind::Touch(id),
                position,
                ..
            } => {
                let last = self.touches.remove(&id)?;
                InputEvent::Touch {
                    id: id.into_raw() as u64,
                    phase: TouchPhase::Cancelled,
                    position: metrics
                        .logical_point(position.map(|p| Point::new(p.x, p.y)).unwrap_or(last)),
                }
            }
            WindowEvent::PointerMoved {
                source: PointerSource::Touch { finger_id, .. },
                position,
                ..
            } => {
                let physical = Point::new(position.x, position.y);
                *self.touches.get_mut(&finger_id)? = physical;
                InputEvent::Touch {
                    id: finger_id.into_raw() as u64,
                    phase: TouchPhase::Moved,
                    position: metrics.logical_point(physical),
                }
            }
            WindowEvent::PointerButton {
                button: ButtonSource::Touch { finger_id, .. },
                state,
                position,
                ..
            } => {
                let physical = Point::new(position.x, position.y);
                let phase = if state == ElementState::Pressed {
                    self.touches.insert(finger_id, physical);
                    TouchPhase::Started
                } else {
                    self.touches.remove(&finger_id)?;
                    TouchPhase::Ended
                };
                InputEvent::Touch {
                    id: finger_id.into_raw() as u64,
                    phase,
                    position: metrics.logical_point(physical),
                }
            }
            WindowEvent::PointerEntered { primary: true, .. } => InputEvent::PointerEntered,
            WindowEvent::PointerLeft { primary: true, .. } => InputEvent::PointerLeft,
            WindowEvent::PointerMoved {
                primary: true,
                position,
                ..
            } => {
                InputEvent::PointerMoved(metrics.logical_point(Point::new(position.x, position.y)))
            }
            WindowEvent::PointerButton {
                primary: true,
                button,
                state,
                position,
                ..
            } => InputEvent::PointerButton {
                button: match button {
                    ButtonSource::Unknown(id) => PointerButton::Other(id),
                    button => match button.mouse_button()? {
                        MouseButton::Left => PointerButton::Primary,
                        MouseButton::Right => PointerButton::Secondary,
                        MouseButton::Middle => PointerButton::Middle,
                        MouseButton::Back => PointerButton::Back,
                        MouseButton::Forward => PointerButton::Forward,
                        other => PointerButton::Other(other as u16),
                    },
                },
                pressed: state == ElementState::Pressed,
                position: Some(metrics.logical_point(Point::new(position.x, position.y))),
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
                _ => return None,
            }),
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
                // Surrounding-text capability is not requested by this adapter.
                _ => return None,
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
        super_key: value.meta_key(),
    }
}

fn key(value: WinitKey) -> Key {
    match value {
        WinitKey::Character(value) if value == " " => Key::Space,
        WinitKey::Character(value) => Key::Character(value.to_string()),
        WinitKey::Named(value) => match value {
            NamedKey::Enter => Key::Enter,
            NamedKey::Escape => Key::Escape,
            NamedKey::Tab => Key::Tab,
            NamedKey::Backspace => Key::Backspace,
            NamedKey::Delete => Key::Delete,
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
    use winit::dpi::PhysicalPosition;
    use zui_types::Extent;

    fn metrics(scale: f64) -> WindowMetrics {
        WindowMetrics {
            extent: Extent::new(800, 600),
            scale,
            occluded: false,
        }
    }
    fn button(button: ButtonSource, state: ElementState, x: f64, y: f64) -> WindowEvent {
        WindowEvent::PointerButton {
            device_id: None,
            state,
            position: PhysicalPosition::new(x, y),
            primary: true,
            button,
            is_macos_activation_click: false,
        }
    }
    #[test]
    fn pointer_coordinates_and_clicks_follow_scale_changes() {
        let mut state = InputState::default();
        let moved = WindowEvent::PointerMoved {
            device_id: None,
            position: PhysicalPosition::new(120.0, 80.0),
            primary: true,
            source: PointerSource::Mouse,
        };
        assert_eq!(
            state.translate(moved, metrics(2.0)),
            Some(InputEvent::PointerMoved(Point::new(60.0, 40.0)))
        );
        // Button coordinates are authoritative even if no move was delivered first.
        let click = button(MouseButton::Left.into(), ElementState::Pressed, 150.0, 90.0);
        assert_eq!(
            state.translate(click, metrics(1.5)),
            Some(InputEvent::PointerButton {
                button: PointerButton::Primary,
                pressed: true,
                position: Some(Point::new(100.0, 60.0))
            })
        );
    }
    #[test]
    fn ime_events_are_preserved() {
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
        assert_eq!(
            state.translate(WindowEvent::Ime(Ime::Commit("中文".into())), metrics(2.0)),
            Some(InputEvent::ImeCommit("中文".into()))
        );
    }
    #[test]
    fn keyboard_mapping_preserves_space_and_command_shortcuts() {
        assert_eq!(key(WinitKey::Character(" ".into())), Key::Space);
        assert_eq!(
            key(WinitKey::Character("中".into())),
            Key::Character("中".into())
        );
        assert_eq!(key(WinitKey::Named(NamedKey::Enter)), Key::Enter);
        assert_eq!(key(WinitKey::Named(NamedKey::F12)), Key::Function(12));
        let value = modifiers(ModifiersState::META | ModifiersState::SHIFT);
        assert!(value.super_key && value.shift);
        assert!(!value.control && !value.alt);
    }

    #[test]
    fn scroll_units_remain_distinct_across_dpi_changes() {
        let mut state = InputState::default();
        let scroll = |delta| WindowEvent::MouseWheel {
            device_id: None,
            delta,
            phase: winit::event::TouchPhase::Moved,
            source: winit::event::ScrollSource::Unknown,
        };
        assert_eq!(
            state.translate(
                scroll(MouseScrollDelta::PixelDelta((30.0, 60.0).into())),
                metrics(1.5)
            ),
            Some(InputEvent::Scroll(ScrollDelta::Pixels { x: 20.0, y: 40.0 }))
        );
        assert_eq!(
            state.translate(scroll(MouseScrollDelta::LineDelta(1.0, 2.0)), metrics(2.0)),
            Some(InputEvent::Scroll(ScrollDelta::Lines { x: 1.0, y: 2.0 }))
        );
    }
    #[test]
    fn touch_lifecycle_does_not_generate_mouse_events_or_duplicate_end() {
        let mut state = InputState::default();
        let id = FingerId::from_raw(7);
        let touch = ButtonSource::Touch {
            finger_id: id,
            force: None,
        };
        let entered = WindowEvent::PointerEntered {
            device_id: None,
            position: (20.0, 40.0).into(),
            primary: true,
            kind: PointerKind::Touch(id),
        };
        assert_eq!(state.translate(entered, metrics(2.0)), None);
        assert_eq!(
            state.translate(
                button(touch.clone(), ElementState::Pressed, 20.0, 40.0),
                metrics(2.0)
            ),
            Some(InputEvent::Touch {
                id: 7,
                phase: TouchPhase::Started,
                position: Point::new(10.0, 20.0)
            })
        );
        let moved = WindowEvent::PointerMoved {
            device_id: None,
            position: (40.0, 60.0).into(),
            primary: true,
            source: PointerSource::Touch {
                finger_id: id,
                force: None,
            },
        };
        assert_eq!(
            state.translate(moved, metrics(2.0)),
            Some(InputEvent::Touch {
                id: 7,
                phase: TouchPhase::Moved,
                position: Point::new(20.0, 30.0)
            })
        );
        let left = WindowEvent::PointerLeft {
            device_id: None,
            position: None,
            primary: true,
            kind: PointerKind::Touch(id),
        };
        assert_eq!(
            state.translate(left.clone(), metrics(1.0)),
            Some(InputEvent::Touch {
                id: 7,
                phase: TouchPhase::Cancelled,
                position: Point::new(40.0, 60.0)
            })
        );
        state.translate(
            button(touch.clone(), ElementState::Pressed, 20.0, 40.0),
            metrics(2.0),
        );
        assert_eq!(
            state.translate(
                button(touch, ElementState::Released, 20.0, 40.0),
                metrics(2.0)
            ),
            Some(InputEvent::Touch {
                id: 7,
                phase: TouchPhase::Ended,
                position: Point::new(10.0, 20.0)
            })
        );
        assert_eq!(state.translate(left, metrics(2.0)), None);
    }
}
