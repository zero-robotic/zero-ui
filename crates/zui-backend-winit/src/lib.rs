//! Ordinary multi-window application backend built on winit.

use std::{
    collections::{HashMap, VecDeque},
    env,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Instant,
};

use winit::application::ApplicationHandler;
use winit::event::{ElementState, Ime as WinitImeEvent, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, NamedKey};
use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use winit::window::{Window, WindowAttributes};
use zui_core::{Dip, Id, OutputId, Point, Rect, ScaleFactor, Size, WindowId};
use zui_platform::spi;
use zui_platform::{
    AppContext, AppLoop, Backend, Capabilities, DragDropEvent, Host, Ime, ImeEvent, InputEvent,
    KeyCode, KeyState, LoopControl, Modifiers as UiModifiers, Output, Paths, PlatformError,
    PlatformEvent, SurfaceTarget, UiTask, UiTaskPoster, WindowOptions,
};

enum UserEvent {
    Run(UiTask),
}

pub struct WinitBackend {
    event_loop: Option<EventLoop<UserEvent>>,
    paths: Result<Paths, PlatformError>,
    capabilities: Capabilities,
    ime_windows: Arc<Mutex<HashMap<WindowId, Arc<Window>>>>,
}

impl WinitBackend {
    pub fn new() -> Result<Self, PlatformError> {
        let event_loop = EventLoop::<UserEvent>::with_user_event()
            .build()
            .map_err(|error| PlatformError::Backend(error.to_string()))?;
        let ime_windows = Arc::new(Mutex::new(HashMap::new()));
        let capabilities = Capabilities::new().with_ime(WinitIme {
            windows: Arc::clone(&ime_windows),
        });
        Ok(Self {
            event_loop: Some(event_loop),
            paths: resolve_paths(),
            capabilities,
            ime_windows,
        })
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

struct WinitIme {
    windows: Arc<Mutex<HashMap<WindowId, Arc<Window>>>>,
}

impl Ime for WinitIme {
    fn set_enabled(&mut self, window: WindowId, enabled: bool) -> Result<(), PlatformError> {
        let windows = self.windows.lock().map_err(|_| PlatformError::Capability {
            name: "ime",
            reason: "window registry was poisoned".into(),
        })?;
        let native = windows
            .get(&window)
            .ok_or(PlatformError::UnknownWindow(window))?;
        native.set_ime_allowed(enabled);
        Ok(())
    }

    fn set_cursor_area(&mut self, window: WindowId, area: Rect) -> Result<(), PlatformError> {
        let windows = self.windows.lock().map_err(|_| PlatformError::Capability {
            name: "ime",
            reason: "window registry was poisoned".into(),
        })?;
        let native = windows
            .get(&window)
            .ok_or(PlatformError::UnknownWindow(window))?;
        let scale = native.scale_factor();
        native.set_ime_cursor_area(
            winit::dpi::PhysicalPosition::new(
                (area.origin.x.0 as f64 * scale).round() as i32,
                (area.origin.y.0 as f64 * scale).round() as i32,
            ),
            winit::dpi::PhysicalSize::new(
                (area.size.width.0 as f64 * scale).round().max(1.0) as u32,
                (area.size.height.0 as f64 * scale).round().max(1.0) as u32,
            ),
        );
        Ok(())
    }
}

struct Runner<'a> {
    options: Option<WindowOptions>,
    hosts: HashMap<WindowId, WinitHost>,
    native_ids: HashMap<winit::window::WindowId, WindowId>,
    next_id: u64,
    app: &'a mut dyn AppLoop,
    pending_events: VecDeque<PlatformEvent>,
    redraw_at: HashMap<WindowId, Instant>,
    ime_composing: HashMap<WindowId, bool>,
    pointer_positions: HashMap<WindowId, Point>,
    modifiers: UiModifiers,
    paths: Result<Paths, PlatformError>,
    outputs: Vec<Output>,
    capabilities: Capabilities,
    poster: UiTaskPoster,
    ime_windows: Arc<Mutex<HashMap<WindowId, Arc<Window>>>>,
    error: Option<PlatformError>,
    exiting: bool,
}

struct WinitContext<'a> {
    event_loop: &'a ActiveEventLoop,
    hosts: &'a mut HashMap<WindowId, WinitHost>,
    native_ids: &'a mut HashMap<winit::window::WindowId, WindowId>,
    next_id: &'a mut u64,
    pending_events: &'a mut VecDeque<PlatformEvent>,
    paths: &'a Result<Paths, PlatformError>,
    outputs: &'a [Output],
    capabilities: &'a mut Capabilities,
    poster: UiTaskPoster,
    ime_windows: Arc<Mutex<HashMap<WindowId, Arc<Window>>>>,
}

impl AppContext for WinitContext<'_> {
    fn host(&self, window: WindowId) -> Option<&dyn Host> {
        self.hosts.get(&window).map(|host| host as &dyn Host)
    }

    fn window_ids(&self) -> Vec<WindowId> {
        let mut ids = self.hosts.keys().copied().collect::<Vec<_>>();
        ids.sort_by_key(|id| id.0.value());
        ids
    }

    fn create_window(&mut self, options: WindowOptions) -> Result<WindowId, PlatformError> {
        let attrs = WindowAttributes::default()
            .with_title(options.title)
            .with_inner_size(winit::dpi::LogicalSize::new(
                options.size.width.0,
                options.size.height.0,
            ))
            .with_resizable(options.resizable);
        let window = Arc::new(
            self.event_loop
                .create_window(attrs)
                .map_err(|error| PlatformError::Backend(error.to_string()))?,
        );
        let id = WindowId(Id::new(*self.next_id));
        *self.next_id = self.next_id.saturating_add(1);
        let scale_factor = ScaleFactor(window.scale_factor());
        let physical = window.inner_size();
        let size = logical_size(physical, scale_factor);
        self.native_ids.insert(window.id(), id);
        self.ime_windows
            .lock()
            .map_err(|_| PlatformError::Backend("IME window registry was poisoned".into()))?
            .insert(id, Arc::clone(&window));
        self.hosts.insert(
            id,
            WinitHost {
                id,
                window,
                size,
                scale_factor,
            },
        );
        self.pending_events
            .push_back(PlatformEvent::WindowCreated(id));
        Ok(id)
    }

    fn destroy_window(&mut self, window: WindowId) -> Result<(), PlatformError> {
        let host = self
            .hosts
            .remove(&window)
            .ok_or(PlatformError::UnknownWindow(window))?;
        self.native_ids.remove(&host.window.id());
        self.ime_windows
            .lock()
            .map_err(|_| PlatformError::Backend("IME window registry was poisoned".into()))?
            .remove(&window);
        self.pending_events
            .push_back(PlatformEvent::WindowDestroyed(window));
        Ok(())
    }

    fn paths(&self) -> Result<&Paths, PlatformError> {
        self.paths.as_ref().map_err(Clone::clone)
    }
    fn outputs(&self) -> &[Output] {
        self.outputs
    }
    fn capabilities(&mut self) -> &mut Capabilities {
        self.capabilities
    }
    fn ui_task_poster(&self) -> UiTaskPoster {
        self.poster.clone()
    }
}

impl ApplicationHandler<UserEvent> for Runner<'_> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let Some(options) = self.options.take() else {
            return;
        };
        self.outputs = output_snapshot(event_loop);
        self.pending_events
            .push_back(PlatformEvent::OutputsChanged(self.outputs.clone()));
        if let Err(error) = self.with_context(event_loop, |context| context.create_window(options))
        {
            self.fail(event_loop, error);
            return;
        }
        self.flush_events(event_loop);
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Run(task) => task.run(),
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        native_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let Some(window) = self.native_ids.get(&native_id).copied() else {
            return;
        };
        let control = match event {
            WindowEvent::CloseRequested => {
                self.dispatch(event_loop, PlatformEvent::CloseRequested(window));
                if !self.exiting && self.hosts.contains_key(&window) {
                    if let Err(error) =
                        self.with_context(event_loop, |context| context.destroy_window(window))
                    {
                        self.fail(event_loop, error);
                    }
                }
                self.flush_events(event_loop);
                if self.hosts.is_empty() {
                    self.exiting = true;
                    event_loop.exit();
                }
                return;
            }
            WindowEvent::RedrawRequested => {
                self.redraw_at.remove(&window);
                Some(PlatformEvent::RedrawRequested(window))
            }
            WindowEvent::Resized(physical) => {
                let Some(host) = self.hosts.get_mut(&window) else {
                    return;
                };
                host.size = logical_size(physical, host.scale_factor);
                Some(PlatformEvent::WindowResized {
                    window,
                    size: host.size,
                })
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                let Some(host) = self.hosts.get_mut(&window) else {
                    return;
                };
                host.scale_factor = ScaleFactor(scale_factor);
                host.size = logical_size(host.window.inner_size(), host.scale_factor);
                Some(PlatformEvent::ScaleFactorChanged {
                    window,
                    size: host.size,
                    scale_factor: host.scale_factor,
                })
            }
            WindowEvent::CursorMoved { position, .. } => {
                let Some(host) = self.hosts.get(&window) else {
                    return;
                };
                let position = Point {
                    x: Dip(position.x as f32 / host.scale_factor.0 as f32),
                    y: Dip(position.y as f32 / host.scale_factor.0 as f32),
                };
                self.pointer_positions.insert(window, position);
                Some(PlatformEvent::Input {
                    window,
                    event: InputEvent::CursorMoved { position },
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
            WindowEvent::MouseInput { state, button, .. } => Some(PlatformEvent::Input {
                window,
                event: InputEvent::MouseInput {
                    button: map_button(button),
                    state: map_state(state),
                },
            }),
            WindowEvent::MouseWheel { delta, .. } => {
                let scale = self
                    .hosts
                    .get(&window)
                    .map_or(1.0, |host| host.scale_factor.0 as f32);
                let (delta_x, delta_y) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (Dip(x * 24.0), Dip(y * 24.0)),
                    MouseScrollDelta::PixelDelta(position) => (
                        Dip(position.x as f32 / scale),
                        Dip(position.y as f32 / scale),
                    ),
                };
                Some(PlatformEvent::Input {
                    window,
                    event: InputEvent::MouseWheel { delta_x, delta_y },
                })
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if self.ime_composing.get(&window).copied().unwrap_or(false) {
                    None
                } else {
                    Some(PlatformEvent::Input {
                        window,
                        event: InputEvent::Keyboard {
                            key: map_key(&event.logical_key),
                            state: map_state(event.state),
                            modifiers: self.modifiers,
                        },
                    })
                }
            }
            WindowEvent::Ime(ime) => {
                self.handle_ime(event_loop, window, ime);
                return;
            }
            WindowEvent::Occluded(occluded) => {
                Some(PlatformEvent::WindowOccluded { window, occluded })
            }
            WindowEvent::HoveredFile(path) => {
                Some(PlatformEvent::DragDrop(DragDropEvent::Entered {
                    window,
                    paths: vec![path],
                }))
            }
            WindowEvent::DroppedFile(path) => {
                Some(PlatformEvent::DragDrop(DragDropEvent::Dropped {
                    window,
                    paths: vec![path],
                }))
            }
            WindowEvent::HoveredFileCancelled => {
                Some(PlatformEvent::DragDrop(DragDropEvent::Cancelled { window }))
            }
            _ => None,
        };
        if let Some(event) = control {
            self.dispatch(event_loop, event);
        }
        self.flush_events(event_loop);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let outputs = output_snapshot(event_loop);
        if outputs != self.outputs {
            self.outputs = outputs.clone();
            self.dispatch(event_loop, PlatformEvent::OutputsChanged(outputs));
        }
        self.dispatch(event_loop, PlatformEvent::AboutToWait);
        self.flush_events(event_loop);
        if self.exiting {
            return;
        }

        let now = Instant::now();
        let due = self
            .redraw_at
            .iter()
            .filter_map(|(window, deadline)| (*deadline <= now).then_some(*window))
            .collect::<Vec<_>>();
        for window in due {
            self.redraw_at.remove(&window);
            if let Some(host) = self.hosts.get(&window) {
                host.window.request_redraw();
            }
        }
        if let Some(deadline) = self.redraw_at.values().min().copied() {
            event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
        } else {
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }
}

impl Runner<'_> {
    fn with_context<T>(
        &mut self,
        event_loop: &ActiveEventLoop,
        callback: impl FnOnce(&mut WinitContext<'_>) -> Result<T, PlatformError>,
    ) -> Result<T, PlatformError> {
        let mut context = WinitContext {
            event_loop,
            hosts: &mut self.hosts,
            native_ids: &mut self.native_ids,
            next_id: &mut self.next_id,
            pending_events: &mut self.pending_events,
            paths: &self.paths,
            outputs: &self.outputs,
            capabilities: &mut self.capabilities,
            poster: self.poster.clone(),
            ime_windows: Arc::clone(&self.ime_windows),
        };
        callback(&mut context)
    }

    fn dispatch(&mut self, event_loop: &ActiveEventLoop, event: PlatformEvent) {
        if self.exiting {
            return;
        }
        let control = {
            let app = &mut self.app;
            let mut context = WinitContext {
                event_loop,
                hosts: &mut self.hosts,
                native_ids: &mut self.native_ids,
                next_id: &mut self.next_id,
                pending_events: &mut self.pending_events,
                paths: &self.paths,
                outputs: &self.outputs,
                capabilities: &mut self.capabilities,
                poster: self.poster.clone(),
                ime_windows: Arc::clone(&self.ime_windows),
            };
            app.event(&mut context, event)
        };
        self.apply_control(event_loop, control);
    }

    fn flush_events(&mut self, event_loop: &ActiveEventLoop) {
        while !self.exiting {
            let Some(event) = self.pending_events.pop_front() else {
                break;
            };
            self.dispatch(event_loop, event);
        }
    }

    fn handle_ime(&mut self, event_loop: &ActiveEventLoop, window: WindowId, ime: WinitImeEvent) {
        match ime {
            WinitImeEvent::Enabled => {
                self.ime_composing.insert(window, false);
                self.dispatch_input(event_loop, window, ImeEvent::Enabled);
            }
            WinitImeEvent::Disabled => {
                if self.ime_composing.remove(&window).unwrap_or(false) {
                    self.dispatch_input(event_loop, window, ImeEvent::Cancelled);
                }
                self.dispatch_input(event_loop, window, ImeEvent::Disabled);
            }
            WinitImeEvent::Preedit(text, selection) if text.is_empty() => {
                if self.ime_composing.insert(window, false).unwrap_or(false) {
                    self.dispatch_input(event_loop, window, ImeEvent::Cancelled);
                }
            }
            WinitImeEvent::Preedit(text, selection) => {
                self.ime_composing.insert(window, true);
                self.dispatch_input(event_loop, window, ImeEvent::Preedit { text, selection });
            }
            WinitImeEvent::Commit(text) => {
                self.ime_composing.insert(window, false);
                self.dispatch_input(event_loop, window, ImeEvent::Commit(text));
            }
        }
    }

    fn dispatch_input(&mut self, event_loop: &ActiveEventLoop, window: WindowId, event: ImeEvent) {
        self.dispatch(
            event_loop,
            PlatformEvent::Input {
                window,
                event: InputEvent::Ime(event),
            },
        );
    }

    fn apply_control(&mut self, event_loop: &ActiveEventLoop, control: LoopControl) {
        match control {
            LoopControl::Continue => {}
            LoopControl::RequestRedraw(window) => {
                if let Some(host) = self.hosts.get(&window) {
                    host.window.request_redraw();
                } else {
                    self.fail(event_loop, PlatformError::UnknownWindow(window));
                }
            }
            LoopControl::WaitUntil { window, deadline } if deadline <= Instant::now() => {
                if let Some(host) = self.hosts.get(&window) {
                    host.window.request_redraw();
                } else {
                    self.fail(event_loop, PlatformError::UnknownWindow(window));
                }
            }
            LoopControl::WaitUntil { window, deadline } => {
                if self.hosts.contains_key(&window) {
                    self.redraw_at.insert(window, deadline);
                } else {
                    self.fail(event_loop, PlatformError::UnknownWindow(window));
                }
            }
            LoopControl::Exit => {
                self.exiting = true;
                event_loop.exit();
            }
        }
    }

    fn fail(&mut self, event_loop: &ActiveEventLoop, error: PlatformError) {
        self.error = Some(error);
        self.exiting = true;
        event_loop.exit();
    }
}

impl Backend for WinitBackend {
    fn run(mut self, options: WindowOptions, app: &mut dyn AppLoop) -> Result<(), PlatformError> {
        let event_loop = self
            .event_loop
            .take()
            .ok_or_else(|| PlatformError::Backend("event loop already consumed".into()))?;
        let poster = task_poster(event_loop.create_proxy());
        let mut runner = Runner {
            options: Some(options),
            hosts: HashMap::new(),
            native_ids: HashMap::new(),
            next_id: 1,
            app,
            pending_events: VecDeque::new(),
            redraw_at: HashMap::new(),
            ime_composing: HashMap::new(),
            pointer_positions: HashMap::new(),
            modifiers: UiModifiers::default(),
            paths: self.paths,
            outputs: Vec::new(),
            capabilities: self.capabilities,
            poster,
            ime_windows: self.ime_windows,
            error: None,
            exiting: false,
        };
        event_loop
            .run_app(&mut runner)
            .map_err(|error| PlatformError::Backend(error.to_string()))?;
        if let Some(error) = runner.error {
            return Err(error);
        }
        Ok(())
    }
}

fn task_poster(proxy: EventLoopProxy<UserEvent>) -> UiTaskPoster {
    UiTaskPoster::new(move |task| {
        proxy
            .send_event(UserEvent::Run(task))
            .map_err(|_| PlatformError::EventLoopClosed)
    })
}

fn logical_size(physical: winit::dpi::PhysicalSize<u32>, scale: ScaleFactor) -> Size {
    Size {
        width: Dip(physical.width as f32 / scale.0 as f32),
        height: Dip(physical.height as f32 / scale.0 as f32),
    }
}

fn output_snapshot(event_loop: &ActiveEventLoop) -> Vec<Output> {
    let mut monitors = event_loop.available_monitors().collect::<Vec<_>>();
    monitors.sort_by_key(|monitor| {
        let position = monitor.position();
        (position.x, position.y, monitor.name().unwrap_or_default())
    });
    monitors
        .into_iter()
        .enumerate()
        .map(|(index, monitor)| {
            let scale = ScaleFactor(monitor.scale_factor());
            let position = monitor.position();
            let size = monitor.size();
            Output {
                id: OutputId(Id::new(index as u64 + 1)),
                name: monitor.name(),
                logical_bounds: Rect {
                    origin: Point {
                        x: Dip(position.x as f32 / scale.0 as f32),
                        y: Dip(position.y as f32 / scale.0 as f32),
                    },
                    size: logical_size(size, scale),
                },
                scale_factor: scale,
            }
        })
        .collect()
}

fn resolve_paths() -> Result<Paths, PlatformError> {
    #[cfg(target_os = "windows")]
    {
        let config = env_path("APPDATA")?;
        let local = env_path("LOCALAPPDATA")?;
        Ok(Paths {
            config_dir: config,
            data_dir: local.clone(),
            cache_dir: local,
            runtime_dir: None,
        })
    }
    #[cfg(target_os = "macos")]
    {
        let home = env_path("HOME")?;
        let data = home.join("Library/Application Support");
        Ok(Paths {
            config_dir: data.clone(),
            data_dir: data,
            cache_dir: home.join("Library/Caches"),
            runtime_dir: None,
        })
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let home = env_path("HOME")?;
        Ok(Paths {
            config_dir: env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".config")),
            data_dir: env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".local/share")),
            cache_dir: env::var_os("XDG_CACHE_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".cache")),
            runtime_dir: env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from),
        })
    }
}

fn env_path(variable: &'static str) -> Result<PathBuf, PlatformError> {
    env::var_os(variable)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| PlatformError::PathUnavailable {
            variable,
            reason: "environment variable is unset or empty".into(),
        })
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
        winit::event::MouseButton::Other(value) => zui_platform::MouseButton::Other(value),
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

    #[test]
    fn path_failures_are_structured() {
        let error = env_path("ZUI_TEST_PATH_THAT_IS_NOT_SET").unwrap_err();
        assert!(matches!(
            error,
            PlatformError::PathUnavailable {
                variable: "ZUI_TEST_PATH_THAT_IS_NOT_SET",
                ..
            }
        ));
    }

    #[test]
    fn supported_platform_paths_resolve_to_non_empty_directories() {
        let paths = resolve_paths().expect("the test host must expose its user directories");
        assert!(!paths.config_dir.as_os_str().is_empty());
        assert!(!paths.data_dir.as_os_str().is_empty());
        assert!(!paths.cache_dir.as_os_str().is_empty());
    }
}
