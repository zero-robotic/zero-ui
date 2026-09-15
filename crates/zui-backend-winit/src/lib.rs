//! Ordinary application-window backend built on winit.

use std::{sync::Arc, time::Instant};
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, Ime, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use winit::window::{Window, WindowAttributes};
use zui_core::{Dip, Id, Point, ScaleFactor, Size, WindowId};
use zui_platform::spi;
use zui_platform::{
    AppLoop, Backend, Host, InputEvent, KeyCode, KeyState, LoopControl, Modifiers as UiModifiers,
    PlatformError, PlatformEvent, SurfaceTarget, WindowOptions,
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
    window: Arc<Window>,
    size: Size,
    scale_factor: ScaleFactor,
}

fn native_surface_target<T>(source: Arc<T>) -> SurfaceTarget
where
    T: HasWindowHandle + HasDisplayHandle + Send + Sync + 'static,
{
    spi::native_surface_target(source)
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
    fn surface_target(&self) -> SurfaceTarget {
        native_surface_target(Arc::clone(&self.window))
    }
    fn request_redraw(&self) -> Result<(), PlatformError> {
        self.window.request_redraw();
        Ok(())
    }
}

struct Runner<'a> {
    options: Option<WindowOptions>,
    host: Option<WinitHost>,
    app: &'a mut dyn AppLoop<WinitHost>,
    redraw_at: Option<Instant>,
    ime_composing: bool,
    ime_cursor_position: Option<PhysicalPosition<f64>>,
    modifiers: UiModifiers,
    error: Option<PlatformError>,
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
        let window = match event_loop.create_window(attrs) {
            Ok(window) => window,
            Err(error) => {
                self.error = Some(PlatformError::Backend(error.to_string()));
                event_loop.exit();
                return;
            }
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
            window: Arc::new(window),
            size,
            scale_factor,
        });
        let created = self.app.event(
            self.host.as_ref().expect("host was just installed"),
            PlatformEvent::WindowCreated(id),
        );
        self.apply_control(event_loop, created);
        if matches!(created, LoopControl::Exit) {
            return;
        }
        let ready = self
            .app
            .host_ready(self.host.as_ref().expect("host was just installed"));
        self.apply_control(event_loop, ready);
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
        let control = match event {
            WindowEvent::CloseRequested => {
                let id = self.host.as_ref().expect("host exists").id;
                let _ = self.app.event(
                    self.host.as_ref().expect("host exists"),
                    PlatformEvent::CloseRequested(id),
                );
                LoopControl::Exit
            }
            WindowEvent::RedrawRequested => {
                let id = self.host.as_ref().expect("host exists").id;
                // Any previous deadline has now produced (or been superseded
                // by) a frame. The application can return a fresh deadline.
                self.redraw_at = None;
                self.app.event(
                    self.host.as_ref().expect("host exists"),
                    PlatformEvent::RedrawRequested(id),
                )
            }
            WindowEvent::Resized(size) => {
                let host = self.host.as_mut().expect("host exists");
                host.size = Size {
                    width: Dip(size.width as f32 / host.scale_factor.0 as f32),
                    height: Dip(size.height as f32 / host.scale_factor.0 as f32),
                };
                self.app.event(
                    host,
                    PlatformEvent::WindowResized {
                        window: host.id,
                        size: host.size,
                        scale_factor: host.scale_factor,
                    },
                )
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                let host = self.host.as_mut().expect("host exists");
                host.scale_factor = ScaleFactor(scale_factor);
                let physical = host.window.inner_size();
                host.size = Size {
                    width: Dip(physical.width as f32 / scale_factor as f32),
                    height: Dip(physical.height as f32 / scale_factor as f32),
                };
                self.app.event(
                    host,
                    PlatformEvent::WindowResized {
                        window: host.id,
                        size: host.size,
                        scale_factor: host.scale_factor,
                    },
                )
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
                self.app.event(
                    self.host.as_ref().expect("host exists"),
                    PlatformEvent::Input {
                        window: id,
                        event: InputEvent::CursorMoved {
                            position: Point {
                                x: Dip(position.x as f32 / scale_factor as f32),
                                y: Dip(position.y as f32 / scale_factor as f32),
                            },
                        },
                    },
                )
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                let state = modifiers.state();
                self.modifiers = UiModifiers {
                    shift: state.shift_key(),
                    control: state.control_key(),
                    alt: state.alt_key(),
                    logo: state.super_key(),
                };
                LoopControl::Continue
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
                self.app.event(
                    self.host.as_ref().expect("host exists"),
                    PlatformEvent::Input {
                        window: id,
                        event: InputEvent::MouseInput {
                            button: map_button(button),
                            state: map_state(state),
                        },
                    },
                )
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
                self.app.event(
                    self.host.as_ref().expect("host exists"),
                    PlatformEvent::Input {
                        window: id,
                        event: InputEvent::MouseWheel { delta_x, delta_y },
                    },
                )
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if self.ime_composing {
                    LoopControl::Continue
                } else {
                    let key = map_key(&event.logical_key);
                    let id = self.host.as_ref().expect("host exists").id;
                    self.app.event(
                        self.host.as_ref().expect("host exists"),
                        PlatformEvent::Input {
                            window: id,
                            event: InputEvent::Keyboard {
                                key,
                                state: map_state(event.state),
                                modifiers: self.modifiers,
                            },
                        },
                    )
                }
            }
            WindowEvent::Ime(ime) => match ime {
                Ime::Enabled | Ime::Disabled => {
                    self.ime_composing = false;
                    LoopControl::Continue
                }
                Ime::Preedit(text, _) => {
                    self.ime_composing = !text.is_empty();
                    LoopControl::Continue
                }
                Ime::Commit(text) if !text.is_empty() => {
                    self.ime_composing = false;
                    let id = self.host.as_ref().expect("host exists").id;
                    self.app.event(
                        self.host.as_ref().expect("host exists"),
                        PlatformEvent::Input {
                            window: id,
                            event: InputEvent::Text(text),
                        },
                    )
                }
                Ime::Commit(_) => {
                    self.ime_composing = false;
                    LoopControl::Continue
                }
            },
            WindowEvent::Occluded(occluded) => {
                let host = self.host.as_ref().expect("host exists");
                self.app.event(
                    host,
                    PlatformEvent::WindowOccluded {
                        window: host.id,
                        occluded,
                    },
                )
            }
            _ => LoopControl::Continue,
        };
        self.apply_control(event_loop, control);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let Some(host) = self.host.as_ref() else {
            event_loop.set_control_flow(ControlFlow::Wait);
            return;
        };
        let control = self.app.event(host, PlatformEvent::AboutToWait);
        self.apply_control(event_loop, control);
        if matches!(control, LoopControl::Exit) {
            return;
        }
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
    fn apply_control(&mut self, event_loop: &ActiveEventLoop, control: LoopControl) {
        match control {
            LoopControl::Continue => {}
            LoopControl::RequestRedraw => {
                if let Some(host) = self.host.as_ref() {
                    host.window.request_redraw();
                }
            }
            LoopControl::WaitUntil(deadline) if deadline <= Instant::now() => {
                if let Some(host) = self.host.as_ref() {
                    host.window.request_redraw();
                }
            }
            LoopControl::WaitUntil(deadline) => {
                self.redraw_at = Some(deadline);
            }
            LoopControl::Exit => event_loop.exit(),
        }
    }
}

impl Backend for WinitBackend {
    type Host = WinitHost;
    fn run(
        mut self,
        options: WindowOptions,
        app: &mut dyn AppLoop<Self::Host>,
    ) -> Result<(), PlatformError> {
        let event_loop = self
            .event_loop
            .take()
            .ok_or_else(|| PlatformError::Backend("event loop already consumed".into()))?;
        let mut runner = Runner {
            options: Some(options),
            host: None,
            app,
            redraw_at: None,
            ime_composing: false,
            ime_cursor_position: None,
            modifiers: UiModifiers::default(),
            error: None,
        };
        event_loop
            .run_app(&mut runner)
            .map_err(|e| PlatformError::Backend(e.to_string()))?;
        if let Some(error) = runner.error {
            return Err(error);
        }
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use winit::raw_window_handle::{
        DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WebWindowHandle,
        WindowHandle,
    };
    use zui_platform::SurfaceTargetKind;

    struct NativeHandleStub;

    impl HasWindowHandle for NativeHandleStub {
        fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
            // Web IDs contain no pointers, which makes this a portable test
            // handle on every host target supported by raw-window-handle.
            Ok(unsafe { WindowHandle::borrow_raw(WebWindowHandle::new(1).into()) })
        }
    }

    impl HasDisplayHandle for NativeHandleStub {
        fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
            Ok(DisplayHandle::web())
        }
    }

    #[test]
    fn winit_target_runs_the_shared_surface_lifecycle_contract() {
        let source = Arc::new(NativeHandleStub);
        let retained_source = Arc::downgrade(&source);
        let target = native_surface_target(Arc::clone(&source));
        drop(source);

        assert_eq!(target.kind(), SurfaceTargetKind::Native);
        assert!(target.native_source().is_some());
        assert!(retained_source.upgrade().is_some());
        zui_render::test_support::assert_surface_lifecycle_contract();

        drop(target);
        assert!(retained_source.upgrade().is_none());
    }
}
