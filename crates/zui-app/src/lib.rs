//! Top-level platform and renderer assembly.
#[cfg(feature = "clipboard")]
pub mod clipboard;
mod ime;
mod refresh;
pub use refresh::{PhaseTiming, RefreshMetrics, RefreshPhase};
mod presenter;
#[cfg(test)]
mod tests;
mod wake;
mod widget;
pub use widget::{ViewAction, ViewApplication, ViewFocusRequest, WidgetHost};

pub type ApplicationError = Box<dyn std::error::Error + Send + Sync>;

use presenter::{Presentation, Presenter};
use std::{sync::Arc, time::Duration};
use zui_paint::{Frame, RenderLimits, Scene};
use zui_platform::{
    ImeRequest, NativeWindow, PlatformError, RunReport, WindowAction, WindowConfig, WindowEvent,
    WindowHandler, WindowMetrics,
};
use zui_types::Color;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Backend {
    Cpu,
    #[default]
    Gpu,
}

impl Backend {
    pub fn from_args(
        args: &mut std::iter::Peekable<impl Iterator<Item = String>>,
    ) -> Result<Self, &'static str> {
        match args.peek().map(String::as_str) {
            None => Ok(Self::default()),
            Some(option) if option.starts_with("--") => Ok(Self::default()),
            Some("cpu") => {
                args.next();
                Ok(Self::Cpu)
            }
            Some("gpu") => {
                args.next();
                Ok(Self::Gpu)
            }
            Some(_) => Err("expected cpu or gpu"),
        }
    }
}

/// Scene-level application contract. `WidgetHost` assembles declarative controls
/// through this same entry point without changing the platform event loop.
pub trait Application {
    fn profiling_enabled(&self) -> bool {
        false
    }
    /// CPU wall time for renderer + presentation submission; not GPU completion time.
    fn record_presentation(&mut self, _duration: Duration) {}
    /// Process every event in order. Implementations may merge deferred work, never input events.
    fn update_batch(
        &mut self,
        events: impl IntoIterator<Item = WindowEvent>,
        metrics: WindowMetrics,
        elapsed: Duration,
    ) -> Result<WindowAction, ApplicationError>
    where
        Self: Sized,
    {
        let mut merged = WindowAction::default();
        for event in events {
            let action = self.update(event, metrics, elapsed)?;
            merged.redraw |= action.redraw;
            merged.exit |= action.exit;
            merged.wake_at = match (merged.wake_at, action.wake_at) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            };
            if merged.exit {
                break;
            }
        }
        Ok(merged)
    }

    fn animation_deadline(&self) -> Option<Duration> {
        None
    }
    /// Invoked only for animation deadlines, independently of application Wake events.
    fn animate(
        &mut self,
        _metrics: WindowMetrics,
        _elapsed: Duration,
    ) -> Result<bool, ApplicationError> {
        Ok(false)
    }
    fn ime_request(&self) -> Option<ImeRequest> {
        None
    }
    fn animate_action(
        &mut self,
        metrics: WindowMetrics,
        elapsed: Duration,
    ) -> Result<WindowAction, ApplicationError> {
        Ok(WindowAction {
            redraw: self.animate(metrics, elapsed)?,
            ..Default::default()
        })
    }
    fn connected(&mut self, _window: &dyn NativeWindow) {}
    fn update(
        &mut self,
        event: WindowEvent,
        metrics: WindowMetrics,
        elapsed: Duration,
    ) -> Result<WindowAction, ApplicationError>;
    fn draw(
        &mut self,
        scene: &mut Scene,
        metrics: WindowMetrics,
        elapsed: Duration,
    ) -> Result<(), ApplicationError>;
    fn background(&self) -> Color {
        Color::BLACK
    }
    fn disconnected(&mut self) {}
}

pub fn run(
    config: WindowConfig,
    backend: Backend,
    application: impl Application,
) -> Result<RunReport, PlatformError> {
    let driver = Driver {
        application,
        backend,
        limits: RenderLimits::default(),
        window: None,
        presenter: None,
        scene: Scene::new(),
        wake: wake::WakeSchedule::default(),
        ime: ime::ImeSync::default(),
        input_available: true,
    };
    zui_backend_winit::run(config, driver)
}

struct Driver<A> {
    application: A,
    backend: Backend,
    limits: RenderLimits,
    window: Option<Arc<dyn NativeWindow>>,
    presenter: Option<Box<dyn Presenter>>,
    scene: Scene,
    wake: wake::WakeSchedule,
    ime: ime::ImeSync,
    input_available: bool,
}

impl<A: Application> Driver<A> {
    fn sync_animation(&mut self, available: bool) {
        self.wake.animation_at(
            available
                .then(|| self.application.animation_deadline())
                .flatten(),
        );
    }
    fn sync_ime(&mut self, available: bool) -> Result<(), PlatformError> {
        if let Some(window) = &self.window {
            let desired = if available && self.input_available {
                self.application.ime_request()
            } else {
                None
            };
            self.ime
                .sync(desired, |command| match command {
                    ime::Command::Enabled(enabled) => window.set_ime_enabled(enabled),
                    ime::Command::CursorArea(area) => window.set_ime_cursor_area(area),
                })
                .map_err(PlatformError::Handler)?;
        }
        Ok(())
    }
}

impl<A: Application> WindowHandler for Driver<A> {
    fn connected(&mut self, window: Arc<dyn NativeWindow>) -> Result<(), PlatformError> {
        self.presenter = Some(presenter::create(
            self.backend,
            window.clone(),
            self.limits,
        )?);
        self.application.connected(window.as_ref());
        window.set_ime_enabled(false);
        self.ime = ime::ImeSync::default();
        self.input_available = true;
        self.window = Some(window);
        Ok(())
    }

    fn event(
        &mut self,
        event: WindowEvent,
        metrics: WindowMetrics,
        elapsed: Duration,
    ) -> Result<WindowAction, PlatformError> {
        if let WindowEvent::Focused(focused) = event {
            self.input_available = focused;
        }
        if event == WindowEvent::RedrawRequested {
            if !metrics.drawable() {
                self.sync_ime(false)?;
                self.sync_animation(false);
                return Ok(WindowAction {
                    wake_at: self.wake.next(),
                    ..Default::default()
                });
            }
            self.scene.clear();
            if let Err(error) = self.application.draw(&mut self.scene, metrics, elapsed) {
                self.sync_ime(false)?;
                return Err(PlatformError::Handler(error));
            }
            self.sync_ime(true)?;
            self.sync_animation(true);
            let frame = Frame {
                extent: metrics.extent,
                scale: metrics.scale,
                background: self.application.background(),
            };
            if let Some(window) = &self.window {
                window.pre_present_notify();
            }
            if let Some(presenter) = &mut self.presenter {
                let started = self
                    .application
                    .profiling_enabled()
                    .then(std::time::Instant::now);
                let presentation = match presenter.present(&self.scene, frame) {
                    Ok(presentation) => presentation,
                    Err(error) => {
                        self.sync_ime(false)?;
                        return Err(error);
                    }
                };
                if let Some(started) = started {
                    self.application.record_presentation(started.elapsed());
                }
                match presentation {
                    #[cfg(any(feature = "cpu", feature = "gpu"))]
                    Presentation::Presented => self.wake.presented(),
                    #[cfg(feature = "gpu")]
                    Presentation::Occluded => self.wake.presented(),
                    #[cfg(feature = "gpu")]
                    Presentation::Retry => {
                        self.wake.retry_at(elapsed + Duration::from_millis(16));
                    }
                }
            }
            return Ok(WindowAction {
                wake_at: self.wake.next(),
                ..Default::default()
            });
        }
        let (notify, retry, animation) = if event == WindowEvent::Wake {
            self.wake.take_due(elapsed)
        } else {
            (true, false, false)
        };
        let mut action = if notify {
            match self.application.update(event, metrics, elapsed) {
                Ok(action) => action,
                Err(error) => {
                    self.sync_ime(false)?;
                    return Err(PlatformError::Handler(error));
                }
            }
        } else {
            WindowAction::default()
        };
        action.redraw |= retry;
        if animation && metrics.drawable() && !action.exit {
            match self.application.animate_action(metrics, elapsed) {
                Ok(animated) => {
                    action.redraw |= animated.redraw;
                    action.exit |= animated.exit;
                    action.wake_at = match (action.wake_at, animated.wake_at) {
                        (Some(first), Some(second)) => Some(first.min(second)),
                        (first, second) => first.or(second),
                    };
                }
                Err(error) => {
                    self.sync_ime(false)?;
                    return Err(PlatformError::Handler(error));
                }
            }
        }
        self.sync_ime(metrics.drawable() && !action.exit)?;
        self.sync_animation(metrics.drawable() && !action.exit);
        Ok(self.wake.application_action(action))
    }

    fn disconnected(&mut self) {
        // None is always valid, so this cannot fail validation.
        let _ = self.sync_ime(false);
        // Drop all display resources while the native handle is still alive.
        self.presenter = None;
        self.window = None;
        self.wake = wake::WakeSchedule::default();
        self.ime = ime::ImeSync::default();
        self.application.disconnected();
    }
}
