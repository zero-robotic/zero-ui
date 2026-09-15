//! Deterministic backend for tests and no-display environments.

use std::cell::Cell;
use zui_core::{Id, ScaleFactor, Size, WindowId};
use zui_platform::spi;
use zui_platform::{
    AppLoop, Backend, Host, LoopControl, PlatformError, PlatformEvent, SurfaceTarget, WindowOptions,
};

pub struct HeadlessBackend {
    next_id: u64,
    max_iterations: usize,
}

impl Default for HeadlessBackend {
    fn default() -> Self {
        Self {
            next_id: 1,
            max_iterations: 1_024,
        }
    }
}

impl HeadlessBackend {
    pub fn new() -> Self {
        Self::default()
    }

    /// Guards deterministic tests against applications that continuously
    /// request immediate frames.
    pub fn max_iterations(mut self, value: usize) -> Self {
        self.max_iterations = value.max(1);
        self
    }
}

pub struct HeadlessHost {
    id: WindowId,
    size: Size,
    scale_factor: ScaleFactor,
    redraws: Cell<u32>,
}

impl HeadlessHost {
    pub fn redraw_count(&self) -> u32 {
        self.redraws.get()
    }
}

impl Host for HeadlessHost {
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
        spi::headless_surface_target()
    }

    fn request_redraw(&self) -> Result<(), PlatformError> {
        self.redraws.set(self.redraws.get().saturating_add(1));
        Ok(())
    }
}

impl Backend for HeadlessBackend {
    type Host = HeadlessHost;

    fn run(
        mut self,
        options: WindowOptions,
        app: &mut dyn AppLoop<Self::Host>,
    ) -> Result<(), PlatformError> {
        let id = WindowId(Id::new(self.next_id));
        self.next_id += 1;
        let host = HeadlessHost {
            id,
            size: options.size,
            scale_factor: ScaleFactor::default(),
            redraws: Cell::new(0),
        };
        let mut exit = apply_control(&host, app.event(&host, PlatformEvent::WindowCreated(id)))?;
        if !exit {
            exit = apply_control(&host, app.host_ready(&host))?;
        }

        let mut delivered = 0_u32;
        for _ in 0..self.max_iterations {
            if exit {
                break;
            }
            if delivered < host.redraw_count() {
                delivered += 1;
                exit = apply_control(&host, app.event(&host, PlatformEvent::RedrawRequested(id)))?;
            }
            if exit {
                break;
            }
            let before_wait = host.redraw_count();
            exit = apply_control(&host, app.event(&host, PlatformEvent::AboutToWait))?;
            if exit || (delivered >= host.redraw_count() && host.redraw_count() == before_wait) {
                break;
            }
        }
        if !exit && delivered < host.redraw_count() {
            return Err(PlatformError::Backend(format!(
                "headless event loop exceeded {} iterations",
                self.max_iterations
            )));
        }
        Ok(())
    }
}

fn apply_control(host: &HeadlessHost, control: LoopControl) -> Result<bool, PlatformError> {
    match control {
        LoopControl::Continue => Ok(false),
        // Headless has no wall-clock event source. Advance a scheduled frame
        // immediately while retaining the iteration guard against runaways.
        LoopControl::RequestRedraw | LoopControl::WaitUntil(_) => {
            host.request_redraw()?;
            Ok(false)
        }
        LoopControl::Exit => Ok(true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zui_platform::SurfaceTargetKind;

    #[derive(Default)]
    struct RecordingLoop {
        host_ready: bool,
        events: Vec<PlatformEvent>,
    }

    impl AppLoop<HeadlessHost> for RecordingLoop {
        fn host_ready(&mut self, host: &HeadlessHost) -> LoopControl {
            self.host_ready = host.size() == WindowOptions::default().size;
            LoopControl::RequestRedraw
        }

        fn event(&mut self, _host: &HeadlessHost, event: PlatformEvent) -> LoopControl {
            self.events.push(event);
            LoopControl::Continue
        }
    }

    #[test]
    fn drives_the_public_app_loop_lifecycle() {
        let mut app = RecordingLoop::default();

        HeadlessBackend::new()
            .run(WindowOptions::default(), &mut app)
            .unwrap();

        assert!(app.host_ready);
        assert!(matches!(
            app.events.as_slice(),
            [
                PlatformEvent::WindowCreated(_),
                PlatformEvent::RedrawRequested(_),
                PlatformEvent::AboutToWait,
            ]
        ));
    }

    #[test]
    fn rejects_an_unbounded_immediate_redraw_loop() {
        struct RedrawLoop;

        impl AppLoop<HeadlessHost> for RedrawLoop {
            fn host_ready(&mut self, _host: &HeadlessHost) -> LoopControl {
                LoopControl::RequestRedraw
            }

            fn event(&mut self, _host: &HeadlessHost, event: PlatformEvent) -> LoopControl {
                if matches!(event, PlatformEvent::RedrawRequested(_)) {
                    LoopControl::RequestRedraw
                } else {
                    LoopControl::Continue
                }
            }
        }

        let error = HeadlessBackend::new()
            .max_iterations(3)
            .run(WindowOptions::default(), &mut RedrawLoop)
            .expect_err("an unbounded redraw loop must fail deterministically");

        assert!(error.to_string().contains("exceeded 3 iterations"));
    }

    #[test]
    fn headless_target_runs_the_shared_surface_lifecycle_contract() {
        let host = HeadlessHost {
            id: WindowId(Id::new(1)),
            size: WindowOptions::default().size,
            scale_factor: ScaleFactor::default(),
            redraws: Cell::new(0),
        };

        let target = host.surface_target();
        assert_eq!(target.kind(), SurfaceTargetKind::Headless);
        assert!(target.native_source().is_none());
        zui_render::test_support::assert_surface_lifecycle_contract();
    }
}
