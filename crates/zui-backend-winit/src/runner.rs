use crate::{backend, input::InputState, schedule::Schedule, window::DesktopWindow};
use std::{sync::Arc, time::Instant};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::WindowEvent as NativeEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};
use zui_platform::{
    NativeWindow, PlatformError, RunReport, WindowAction, WindowConfig, WindowEvent, WindowHandler,
    WindowMetrics,
};

/// Runs on the platform main thread. The caller supplies only a lower-level contract.
pub fn run(config: WindowConfig, handler: impl WindowHandler) -> Result<RunReport, PlatformError> {
    if !config.size.is_valid() || config.size.width <= 0.0 || config.size.height <= 0.0 {
        return Err(PlatformError::Unsupported("invalid initial window size"));
    }
    let event_loop = EventLoop::new().map_err(backend)?;
    let mut runner = Runner {
        proxy: event_loop.create_proxy(),
        config,
        handler,
        window: None,
        occluded: false,
        input: InputState::default(),
        schedule: Schedule::default(),
        start: Instant::now(),
        failure: None,
        report: RunReport::default(),
    };
    event_loop.run_app(&mut runner).map_err(backend)?;
    match runner.failure {
        Some(error) => Err(error),
        None => Ok(runner.report),
    }
}

struct Runner<H> {
    proxy: winit::event_loop::EventLoopProxy<()>,
    config: WindowConfig,
    handler: H,
    window: Option<Arc<DesktopWindow>>,
    occluded: bool,
    input: InputState,
    schedule: Schedule,
    start: Instant,
    failure: Option<PlatformError>,
    report: RunReport,
}

impl<H: WindowHandler> Runner<H> {
    fn metrics(&self) -> Option<WindowMetrics> {
        self.window.as_ref().map(|window| WindowMetrics {
            occluded: self.occluded,
            ..window.metrics()
        })
    }
    fn fail(&mut self, event_loop: &ActiveEventLoop, error: PlatformError) {
        if self.failure.is_none() {
            self.failure = Some(error);
        }
        event_loop.exit();
    }
    fn dispatch(&mut self, event_loop: &ActiveEventLoop, event: WindowEvent) {
        let Some(metrics) = self.metrics() else {
            return;
        };
        self.report.events += 1;
        match self.handler.event(event, metrics, self.start.elapsed()) {
            Ok(action) => {
                self.schedule.apply(action);
                if action.exit {
                    event_loop.exit();
                }
            }
            Err(error) => self.fail(event_loop, error),
        }
    }
    fn detach(&mut self) {
        if self.window.is_some() {
            self.handler.disconnected();
            self.window = None;
        }
        self.input.clear();
        self.schedule.suspended();
    }
}

impl<H: WindowHandler> ApplicationHandler for Runner<H> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title(&self.config.title)
            .with_inner_size(LogicalSize::new(
                self.config.size.width,
                self.config.size.height,
            ))
            .with_resizable(self.config.resizable)
            .with_decorations(self.config.decorations)
            .with_enabled_buttons(window_buttons(&self.config));
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(DesktopWindow(window, self.proxy.clone())),
            Err(error) => {
                self.fail(event_loop, backend(error));
                return;
            }
        };
        self.window = Some(window.clone());
        self.occluded = false;
        if let Err(error) = self.handler.connected(window) {
            self.fail(event_loop, error);
            return;
        }
        self.schedule.apply(WindowAction::redraw());
        self.dispatch(event_loop, WindowEvent::Resumed);
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, (): ()) {
        self.dispatch(event_loop, WindowEvent::ExternalWake);
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        self.detach();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: NativeEvent,
    ) {
        if !self
            .window
            .as_ref()
            .is_some_and(|window| window.0.id() == window_id)
        {
            return;
        }
        let translated = match event {
            NativeEvent::CloseRequested => WindowEvent::CloseRequested,
            NativeEvent::Destroyed => {
                self.detach();
                event_loop.exit();
                return;
            }
            NativeEvent::RedrawRequested => {
                let drawable = self.metrics().is_some_and(WindowMetrics::drawable);
                if !self.schedule.begin_redraw(drawable) {
                    return;
                }
                self.report.redraws += 1;
                WindowEvent::RedrawRequested
            }
            NativeEvent::Resized(_) => {
                self.schedule.apply(WindowAction::redraw());
                WindowEvent::Resized
            }
            NativeEvent::ScaleFactorChanged { .. } => {
                self.schedule.apply(WindowAction::redraw());
                WindowEvent::ScaleChanged
            }
            NativeEvent::Focused(focused) => {
                if !focused {
                    self.input.clear();
                }
                WindowEvent::Focused(focused)
            }
            NativeEvent::Occluded(occluded) => {
                self.occluded = occluded;
                if !occluded {
                    self.schedule.apply(WindowAction::redraw());
                }
                WindowEvent::Occluded(occluded)
            }
            other => {
                let Some(metrics) = self.metrics() else {
                    return;
                };
                let Some(input) = self.input.translate(other, metrics) else {
                    return;
                };
                WindowEvent::Input(input)
            }
        };
        self.dispatch(event_loop, translated);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.schedule.take_wake(self.start.elapsed()) {
            self.dispatch(event_loop, WindowEvent::Wake);
        }
        if self
            .schedule
            .request_redraw(self.metrics().is_some_and(WindowMetrics::drawable))
            && let Some(window) = &self.window
        {
            window.request_redraw();
        }
        event_loop.set_control_flow(match self.schedule.deadline() {
            Some(deadline) => ControlFlow::WaitUntil(self.start + deadline),
            None => ControlFlow::Wait,
        });
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        self.detach();
    }
}

fn window_buttons(config: &WindowConfig) -> winit::window::WindowButtons {
    use winit::window::WindowButtons;
    let mut buttons = WindowButtons::empty();
    buttons.set(WindowButtons::CLOSE, config.close_button);
    buttons.set(WindowButtons::MINIMIZE, config.minimize_button);
    buttons.set(
        WindowButtons::MAXIMIZE,
        config.maximize_button && config.resizable,
    );
    buttons
}

#[cfg(test)]
mod chrome_tests {
    use super::*;
    #[test]
    fn window_button_requests_are_independent_and_maximize_requires_resize() {
        use winit::window::WindowButtons;
        let mut config = WindowConfig::default();
        assert_eq!(window_buttons(&config), WindowButtons::all());
        config.minimize_button = false;
        assert_eq!(
            window_buttons(&config),
            WindowButtons::CLOSE | WindowButtons::MAXIMIZE
        );
        config.resizable = false;
        assert_eq!(window_buttons(&config), WindowButtons::CLOSE);
        config.close_button = false;
        assert!(window_buttons(&config).is_empty());
    }
}
