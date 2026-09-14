//! Ordinary application-window backend built on winit.

use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, Ime, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::raw_window_handle::{
    HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle,
};
use winit::window::{Window, WindowAttributes};
use zui_core::{Dip, Id, Point, ScaleFactor, Size, WindowId};
use zui_platform::spi::RawWindowHandleProvider;
use zui_platform::{
    Backend, Host, InputEvent, KeyCode, KeyState, Modifiers as UiModifiers, PlatformError,
    PlatformEvent, WindowOptions,
};

pub struct WinitBackend {
    event_loop: Option<EventLoop<()>>,
}

impl WinitBackend {
    pub fn new() -> Result<Self, PlatformError> {
        EventLoop::new()
            .map(|event_loop| Self {
                event_loop: Some(event_loop),
            })
            .map_err(|e| PlatformError::Backend(e.to_string()))
    }
}

pub struct WinitHost {
    id: WindowId,
    window: Window,
    size: Size,
    scale_factor: ScaleFactor,
}

impl Host for WinitHost {
    fn id(&self) -> WindowId {
        self.id
    }
    fn size(&self) -> Size {
        self.size
    }
    fn scale_factor(&self) -> ScaleFactor {
        self.scale_factor
    }
    fn request_redraw(&self) -> Result<(), PlatformError> {
        self.window.request_redraw();
        Ok(())
    }
}

impl RawWindowHandleProvider for WinitHost {
    fn raw_window_handle(&self) -> Result<RawWindowHandle, winit::raw_window_handle::HandleError> {
        Ok(self.window.window_handle()?.as_raw())
    }

    fn raw_display_handle(
        &self,
    ) -> Result<RawDisplayHandle, winit::raw_window_handle::HandleError> {
        Ok(self.window.display_handle()?.as_raw())
    }
}

struct Runner<'a> {
    options: Option<WindowOptions>,
    host: Option<WinitHost>,
    on_window: &'a mut dyn FnMut(&WinitHost),
    handler: &'a mut dyn FnMut(PlatformEvent) -> Option<Instant>,
    redraw_at: Option<Instant>,
    ime_composing: bool,
    ime_cursor_position: Option<PhysicalPosition<f64>>,
    modifiers: UiModifiers,
}

impl ApplicationHandler for Runner<'_> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.host.is_some() {
            return;
        }
        let options = self.options.take().unwrap_or_default();
        let attrs = WindowAttributes::default()
            .with_title(options.title)
            .with_inner_size(winit::dpi::LogicalSize::new(
                options.size.width.0,
                options.size.height.0,
            ))
            .with_resizable(options.resizable);
        let Ok(window) = event_loop.create_window(attrs) else {
            return;
        };
        window.set_ime_allowed(true);
        let id = WindowId(Id::new(1));
        let scale_factor = ScaleFactor(window.scale_factor());
        let physical = window.inner_size();
        let size = Size {
            width: Dip(physical.width as f32 / scale_factor.0 as f32),
            height: Dip(physical.height as f32 / scale_factor.0 as f32),
        };
        self.host = Some(WinitHost {
            id,
            window,
            size,
            scale_factor,
        });
        let host = self.host.as_ref().expect("host was just installed");
        (self.handler)(PlatformEvent::WindowCreated(id));
        (self.on_window)(host);
        // The application callback may attach a renderer and request the
        // first frame. Keep a backend-owned request as well: on some window
        // systems a redraw requested while the window is being resumed can be
        // coalesced before the callback returns, leaving the window blank
        // until the first input event.
        host.window.request_redraw();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        if self.host.is_none() {
            return;
        }
        let redraw_at = match event {
            WindowEvent::CloseRequested => {
                let id = self.host.as_ref().expect("host exists").id;
                (self.handler)(PlatformEvent::CloseRequested(id));
                event_loop.exit();
                None
            }
            WindowEvent::RedrawRequested => {
                let id = self.host.as_ref().expect("host exists").id;
                (self.handler)(PlatformEvent::RedrawRequested(id));
                None
            }
            WindowEvent::Resized(size) => {
                let host = self.host.as_mut().expect("host exists");
                host.size = Size {
                    width: Dip(size.width as f32 / host.scale_factor.0 as f32),
                    height: Dip(size.height as f32 / host.scale_factor.0 as f32),
                };
                (self.handler)(PlatformEvent::WindowResized {
                    window: host.id,
                    size: host.size,
                    scale_factor: host.scale_factor,
                })
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                let host = self.host.as_mut().expect("host exists");
                host.scale_factor = ScaleFactor(scale_factor);
                let physical = host.window.inner_size();
                host.size = Size {
                    width: Dip(physical.width as f32 / scale_factor as f32),
                    height: Dip(physical.height as f32 / scale_factor as f32),
                };
                (self.handler)(PlatformEvent::WindowResized {
                    window: host.id,
                    size: host.size,
                    scale_factor: host.scale_factor,
                })
            }
            WindowEvent::CursorMoved { position, .. } => {
                let id = self.host.as_ref().expect("host exists").id;
                self.ime_cursor_position = Some(position);
                if !self.ime_composing {
                    if let Some(host) = self.host.as_ref() {
                        host.window.set_ime_cursor_area(
                            PhysicalPosition::new(
                                position.x.round() as i32,
                                position.y.round() as i32,
                            ),
                            PhysicalSize::new(
                                1,
                                (host.scale_factor.0 * 26.0).round().max(1.0) as u32,
                            ),
                        );
                    }
                }
                let scale_factor = self.host.as_ref().expect("host exists").scale_factor.0;
                (self.handler)(PlatformEvent::Input {
                    window: id,
                    event: InputEvent::CursorMoved {
                        position: Point {
                            x: Dip(position.x as f32 / scale_factor as f32),
                            y: Dip(position.y as f32 / scale_factor as f32),
                        },
                    },
                })
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                let state = modifiers.state();
                self.modifiers = UiModifiers {
                    shift: state.shift_key(),
                    control: state.control_key(),
                    alt: state.alt_key(),
                    logo: state.super_key(),
                };
                None
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let id = self.host.as_ref().expect("host exists").id;
                if matches!(state, ElementState::Pressed) {
                    if let (Some(host), Some(position)) =
                        (self.host.as_ref(), self.ime_cursor_position)
                    {
                        host.window.set_ime_cursor_area(
                            PhysicalPosition::new(
                                position.x.round() as i32,
                                position.y.round() as i32,
                            ),
                            PhysicalSize::new(
                                1,
                                (host.scale_factor.0 * 26.0).round().max(1.0) as u32,
                            ),
                        );
                    }
                }
                (self.handler)(PlatformEvent::Input {
                    window: id,
                    event: InputEvent::MouseInput {
                        button: map_button(button),
                        state: map_state(state),
                    },
                })
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let id = self.host.as_ref().expect("host exists").id;
                let scale = self.host.as_ref().expect("host exists").scale_factor.0 as f32;
                let (delta_x, delta_y) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (Dip(x * 24.0), Dip(y * 24.0)),
                    MouseScrollDelta::PixelDelta(position) => (
                        Dip(position.x as f32 / scale),
                        Dip(position.y as f32 / scale),
                    ),
                };
                (self.handler)(PlatformEvent::Input {
                    window: id,
                    event: InputEvent::MouseWheel { delta_x, delta_y },
                })
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if self.ime_composing {
                    None
                } else {
                    let key = map_key(&event.logical_key);
                    let id = self.host.as_ref().expect("host exists").id;
                    (self.handler)(PlatformEvent::Input {
                        window: id,
                        event: InputEvent::Keyboard {
                            key,
                            state: map_state(event.state),
                            modifiers: self.modifiers,
                        },
                    })
                }
            }
            WindowEvent::Ime(ime) => match ime {
                Ime::Enabled | Ime::Disabled => {
                    self.ime_composing = false;
                    None
                }
                Ime::Preedit(text, _) => {
                    self.ime_composing = !text.is_empty();
                    None
                }
                Ime::Commit(text) if !text.is_empty() => {
                    self.ime_composing = false;
                    let id = self.host.as_ref().expect("host exists").id;
                    (self.handler)(PlatformEvent::Input {
                        window: id,
                        event: InputEvent::Text(text),
                    })
                }
                Ime::Commit(_) => {
                    self.ime_composing = false;
                    None
                }
            },
            _ => None,
        };
        self.schedule_redraw(redraw_at);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        (self.handler)(PlatformEvent::AboutToWait);
        if let Some(deadline) = self.redraw_at {
            if deadline <= Instant::now() {
                self.redraw_at = None;
                if let Some(host) = self.host.as_ref() {
                    host.window.request_redraw();
                }
            } else {
                event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
            }
        } else {
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }
}

impl Runner<'_> {
    fn schedule_redraw(&mut self, deadline: Option<Instant>) {
        let Some(deadline) = deadline else {
            return;
        };
        if deadline <= Instant::now() {
            if let Some(host) = self.host.as_ref() {
                host.window.request_redraw();
            }
        } else {
            // A later deferred request supersedes an earlier one. This is
            // essential for resize debouncing: every native resize event must
            // extend the quiet period instead of preserving the first event's
            // deadline.
            self.redraw_at = Some(deadline);
        }
    }
}

impl Backend for WinitBackend {
    type Host = WinitHost;
    fn create_window(&mut self, _options: WindowOptions) -> Result<Self::Host, PlatformError> {
        Err(PlatformError::Backend(
            "winit creates windows during the event loop; use run".into(),
        ))
    }
    fn run(self, handler: &mut dyn FnMut(PlatformEvent)) -> Result<(), PlatformError> {
        let mut on_window = |host: &WinitHost| {
            let _ = host.request_redraw();
        };
        self.run_with_options(WindowOptions::default(), &mut on_window, handler)
    }
}

impl WinitBackend {
    /// Run the event loop and expose the created host to the composition root.
    /// The host is only valid for the duration of the callback and event loop.
    pub fn run_with_options(
        self,
        options: WindowOptions,
        on_window: &mut dyn FnMut(&WinitHost),
        handler: &mut dyn FnMut(PlatformEvent),
    ) -> Result<(), PlatformError> {
        let mut handler_with_redraw = |event| {
            let request_redraw = matches!(
                event,
                PlatformEvent::Input { .. } | PlatformEvent::WindowResized { .. }
            );
            handler(event);
            request_redraw.then(Instant::now)
        };
        self.run_with_options_and_schedule(options, on_window, &mut handler_with_redraw)
    }

    pub fn run_with_options_and_redraw(
        self,
        options: WindowOptions,
        on_window: &mut dyn FnMut(&WinitHost),
        handler: &mut dyn FnMut(PlatformEvent) -> bool,
    ) -> Result<(), PlatformError> {
        let mut handler_with_schedule = |event| handler(event).then(Instant::now);
        self.run_with_options_and_schedule(options, on_window, &mut handler_with_schedule)
    }

    pub fn run_with_options_and_schedule(
        mut self,
        options: WindowOptions,
        on_window: &mut dyn FnMut(&WinitHost),
        handler: &mut dyn FnMut(PlatformEvent) -> Option<Instant>,
    ) -> Result<(), PlatformError> {
        let event_loop = self
            .event_loop
            .take()
            .ok_or_else(|| PlatformError::Backend("event loop already consumed".into()))?;
        let mut runner = Runner {
            options: Some(options),
            host: None,
            on_window,
            handler,
            redraw_at: None,
            ime_composing: false,
            ime_cursor_position: None,
            modifiers: UiModifiers::default(),
        };
        event_loop
            .run_app(&mut runner)
            .map_err(|e| PlatformError::Backend(e.to_string()))
    }
}

fn map_state(state: ElementState) -> KeyState {
    if state == ElementState::Pressed {
        KeyState::Pressed
    } else {
        KeyState::Released
    }
}
fn map_button(button: winit::event::MouseButton) -> zui_platform::MouseButton {
    match button {
        winit::event::MouseButton::Left => zui_platform::MouseButton::Left,
        winit::event::MouseButton::Right => zui_platform::MouseButton::Right,
        winit::event::MouseButton::Middle => zui_platform::MouseButton::Middle,
        winit::event::MouseButton::Other(v) => zui_platform::MouseButton::Other(v),
        _ => zui_platform::MouseButton::Other(0),
    }
}
fn map_key(key: &Key) -> KeyCode {
    match key {
        Key::Named(NamedKey::Escape) => KeyCode::Escape,
        Key::Named(NamedKey::Enter) => KeyCode::Enter,
        Key::Named(NamedKey::Tab) => KeyCode::Tab,
        Key::Named(NamedKey::Backspace) => KeyCode::Backspace,
        Key::Named(NamedKey::ArrowUp) => KeyCode::ArrowUp,
        Key::Named(NamedKey::ArrowDown) => KeyCode::ArrowDown,
        Key::Named(NamedKey::ArrowLeft) => KeyCode::ArrowLeft,
        Key::Named(NamedKey::ArrowRight) => KeyCode::ArrowRight,
        Key::Named(NamedKey::Home) => KeyCode::Home,
        Key::Named(NamedKey::End) => KeyCode::End,
        Key::Character(text) => text
            .chars()
            .next()
            .map(KeyCode::Character)
            .unwrap_or(KeyCode::Unknown),
        _ => KeyCode::Unknown,
    }
}
