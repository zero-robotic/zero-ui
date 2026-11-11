//! Shared press/cancel behavior; controls own values, actions and visuals.
use zui_api::{event::*, widget::Invalidation};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Interaction {
    pub(crate) hovered: bool,
    pub(crate) focused: bool,
    pub(crate) focus_visible: bool,
    pub(crate) mouse: bool,
    pub(crate) space: bool,
}
impl Interaction {
    pub(crate) fn cancel(&mut self) {
        self.mouse = false;
        self.space = false;
    }
    pub(crate) fn event(
        &mut self,
        context: EventContext,
        event: &WidgetEvent,
        enabled: bool,
        enter_activates: bool,
    ) -> (EventResponse, bool) {
        let before = *self;
        let mut response = EventResponse::default();
        let mut activated = false;
        match event {
            WidgetEvent::FocusChanged(focused) => {
                self.focused = *focused;
                self.focus_visible = *focused;
                if !focused {
                    self.mouse = false;
                    self.space = false;
                }
            }
            WidgetEvent::PointerCaptureLost => self.mouse = false,
            WidgetEvent::FocusVisibleChanged(visible) => self.focus_visible = *visible,
            WidgetEvent::Input(InputEvent::PointerEntered) => self.hovered = true,
            WidgetEvent::Input(InputEvent::PointerLeft) => self.hovered = false,
            _ => {}
        }
        if context.phase == EventPhase::Target {
            match event {
                WidgetEvent::Input(InputEvent::PointerButton {
                    button: PointerButton::Primary,
                    pressed,
                    ..
                }) => {
                    response.handled = true;
                    if enabled {
                        if *pressed && self.hovered {
                            self.mouse = true;
                            self.space = false;
                            response.focus = FocusRequest::Focus;
                            response.capture = CaptureRequest::Capture;
                        } else if !pressed {
                            if self.mouse && self.hovered {
                                activated = true;
                            }
                            self.mouse = false;
                            response.capture = CaptureRequest::Release;
                        }
                    }
                }
                WidgetEvent::Input(InputEvent::Key(key)) if enabled && self.focused => {
                    let modified =
                        key.modifiers.control || key.modifiers.alt || key.modifiers.super_key;
                    match key.key {
                        Key::Space if !modified || self.space => {
                            response.handled = true;
                            if key.pressed {
                                if !key.repeat && !modified && !self.mouse {
                                    self.space = true;
                                }
                            } else {
                                if self.space && !modified {
                                    activated = true;
                                }
                                self.space = false;
                            }
                        }
                        Key::Enter if enter_activates && !modified => {
                            response.handled = true;
                            if key.pressed && !key.repeat && !self.space && !self.mouse {
                                activated = true;
                            }
                        }
                        Key::Escape if self.space || self.mouse => {
                            response.handled = true;
                            self.space = false;
                            self.mouse = false;
                            response.capture = CaptureRequest::Release;
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        if before != *self {
            response.invalidation = Invalidation::Paint;
        }
        (response, activated)
    }
}
