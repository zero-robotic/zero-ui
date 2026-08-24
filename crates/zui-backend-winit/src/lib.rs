//! Ordinary application-window backend built on winit.

use winit::application::ApplicationHandler;
use winit::event::{ElementState, Ime, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::raw_window_handle::{
    HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle,
};
use winit::window::{Window, WindowAttributes};
use zui_core::{Dip, Id, Point, ScaleFactor, Size, WindowId};
use zui_platform::spi::RawWindowHandleProvider;
use zui_platform::{
    Backend, Host, InputEvent, KeyCode, KeyState, PlatformError, PlatformEvent, WindowOptions,
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
    handler: &'a mut dyn FnMut(PlatformEvent),
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
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let Some(host) = self.host.as_mut() else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => {
                (self.handler)(PlatformEvent::CloseRequested(host.id));
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => (self.handler)(PlatformEvent::RedrawRequested(host.id)),
            WindowEvent::Resized(size) => {
                host.size = Size {
                    width: Dip(size.width as f32 / host.scale_factor.0 as f32),
                    height: Dip(size.height as f32 / host.scale_factor.0 as f32),
                };
                (self.handler)(PlatformEvent::WindowResized {
                    window: host.id,
                    size: host.size,
                    scale_factor: host.scale_factor,
                });
            }
            WindowEvent::CursorMoved { position, .. } => (self.handler)(PlatformEvent::Input {
                window: host.id,
                event: InputEvent::CursorMoved {
                    position: Point {
                        x: Dip(position.x as f32),
                        y: Dip(position.y as f32),
                    },
                },
            }),
            WindowEvent::MouseInput { state, button, .. } => (self.handler)(PlatformEvent::Input {
                window: host.id,
                event: InputEvent::MouseInput {
                    button: map_button(button),
                    state: map_state(state),
                },
            }),
            WindowEvent::KeyboardInput { event, .. } => {
                let key = map_key(&event.logical_key);
                (self.handler)(PlatformEvent::Input {
                    window: host.id,
                    event: InputEvent::Keyboard {
                        key,
                        state: map_state(event.state),
                        modifiers: Default::default(),
                    },
                });
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                if !text.is_empty() {
                    (self.handler)(PlatformEvent::Input {
                        window: host.id,
                        event: InputEvent::Text(text),
                    });
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        (self.handler)(PlatformEvent::AboutToWait);
        if let Some(host) = self.host.as_ref() {
            host.window.request_redraw();
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
        let mut on_window = |_host: &WinitHost| {};
        self.run_with_options(WindowOptions::default(), &mut on_window, handler)
    }
}

impl WinitBackend {
    /// Run the event loop and expose the created host to the composition root.
    /// The host is only valid for the duration of the callback and event loop.
    pub fn run_with_options(
        mut self,
        options: WindowOptions,
        on_window: &mut dyn FnMut(&WinitHost),
        handler: &mut dyn FnMut(PlatformEvent),
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
        Key::Character(text) => text
            .chars()
            .next()
            .map(KeyCode::Character)
            .unwrap_or(KeyCode::Unknown),
        _ => KeyCode::Unknown,
    }
}
