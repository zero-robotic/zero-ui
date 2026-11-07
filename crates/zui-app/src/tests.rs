use super::*;
use std::error::Error;

struct FailingApplication;
impl Application for FailingApplication {
    fn update(
        &mut self,
        _: WindowEvent,
        _: WindowMetrics,
        _: Duration,
    ) -> Result<WindowAction, ApplicationError> {
        Err(std::io::Error::other("input failed").into())
    }
    fn draw(
        &mut self,
        _: &mut Scene,
        _: WindowMetrics,
        _: Duration,
    ) -> Result<(), ApplicationError> {
        Err(std::io::Error::other("preparation failed").into())
    }
}
struct MustNotPresent;
impl Presenter for MustNotPresent {
    fn present(&mut self, _: &Scene, _: Frame) -> Result<Presentation, PlatformError> {
        panic!("failed application frame must never be presented")
    }
}
#[test]
fn failed_draw_returns_original_error_without_presenting() {
    let mut driver = Driver {
        application: FailingApplication,
        backend: Backend::Cpu,
        limits: Default::default(),
        window: None,
        presenter: Some(Box::new(MustNotPresent)),
        scene: Scene::new(),
        wake: Default::default(),
        ime: Default::default(),
        input_available: true,
    };
    let error = driver
        .event(
            WindowEvent::RedrawRequested,
            WindowMetrics {
                extent: zui_types::Extent::new(100, 100),
                scale: 1.0,
                occluded: false,
            },
            Duration::ZERO,
        )
        .unwrap_err();
    assert_eq!(error.source().unwrap().to_string(), "preparation failed");
    assert!(
        error
            .source()
            .unwrap()
            .downcast_ref::<std::io::Error>()
            .is_some()
    );
}

#[test]
fn failed_update_preserves_error_source() {
    let mut driver = Driver {
        application: FailingApplication,
        backend: Backend::Cpu,
        limits: Default::default(),
        window: None,
        presenter: Some(Box::new(MustNotPresent)),
        scene: Scene::new(),
        wake: Default::default(),
        ime: Default::default(),
        input_available: true,
    };
    let error = driver
        .event(
            WindowEvent::Focused(true),
            WindowMetrics {
                extent: zui_types::Extent::new(100, 100),
                scale: 1.0,
                occluded: false,
            },
            Duration::ZERO,
        )
        .unwrap_err();
    assert_eq!(error.source().unwrap().to_string(), "input failed");
    assert!(
        error
            .source()
            .unwrap()
            .downcast_ref::<std::io::Error>()
            .is_some()
    );
}

#[test]
fn driver_disables_ime_on_focus_loss_hidden_frames_errors_and_disconnect() {
    use raw_window_handle::{
        DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
    };
    use std::sync::Mutex;
    use zui_platform::{ImeRequest, ImeSessionId};
    use zui_types::{Extent, Rect, Size};
    struct Window {
        log: Mutex<Vec<ime::Command>>,
    }
    impl HasWindowHandle for Window {
        fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
            Err(HandleError::Unavailable)
        }
    }
    impl HasDisplayHandle for Window {
        fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
            Err(HandleError::Unavailable)
        }
    }
    impl NativeWindow for Window {
        fn metrics(&self) -> WindowMetrics {
            WindowMetrics {
                extent: Extent::new(200, 100),
                scale: 1.0,
                occluded: false,
            }
        }
        fn request_redraw(&self) {}
        fn request_size(&self, _: Size) {}
        fn set_title(&self, _: &str) {}
        fn set_ime_enabled(&self, value: bool) {
            self.log.lock().unwrap().push(ime::Command::Enabled(value));
        }
        fn set_ime_cursor_area(&self, value: Rect) {
            self.log
                .lock()
                .unwrap()
                .push(ime::Command::CursorArea(value));
        }
        fn pre_present_notify(&self) {}
    }
    struct App {
        request: ImeRequest,
        fail: bool,
    }
    impl Application for App {
        fn ime_request(&self) -> Option<ImeRequest> {
            Some(self.request)
        }
        fn update(
            &mut self,
            _: WindowEvent,
            _: WindowMetrics,
            _: Duration,
        ) -> Result<WindowAction, ApplicationError> {
            Ok(WindowAction::default())
        }
        fn draw(
            &mut self,
            _: &mut Scene,
            _: WindowMetrics,
            _: Duration,
        ) -> Result<(), ApplicationError> {
            if self.fail {
                Err(std::io::Error::other("failed frame").into())
            } else {
                Ok(())
            }
        }
    }
    let window = Arc::new(Window {
        log: Mutex::new(Vec::new()),
    });
    let metrics = window.metrics();
    let area = Rect::new(20.0, 10.0, 1.0, 24.0);
    let mut driver = Driver {
        application: App {
            request: ImeRequest {
                session: ImeSessionId {
                    owner: 1,
                    generation: 0,
                },
                cursor_area: area,
            },
            fail: false,
        },
        backend: Backend::Cpu,
        limits: Default::default(),
        window: Some(window.clone()),
        presenter: None,
        scene: Scene::new(),
        wake: Default::default(),
        ime: Default::default(),
        input_available: true,
    };
    driver
        .event(WindowEvent::RedrawRequested, metrics, Duration::ZERO)
        .unwrap();
    assert_eq!(
        *window.log.lock().unwrap(),
        [ime::Command::CursorArea(area), ime::Command::Enabled(true)]
    );
    window.log.lock().unwrap().clear();
    driver
        .event(WindowEvent::Focused(false), metrics, Duration::ZERO)
        .unwrap();
    driver
        .event(WindowEvent::RedrawRequested, metrics, Duration::ZERO)
        .unwrap();
    assert_eq!(*window.log.lock().unwrap(), [ime::Command::Enabled(false)]);
    driver
        .event(WindowEvent::Focused(true), metrics, Duration::ZERO)
        .unwrap();
    window.log.lock().unwrap().clear();
    driver
        .event(
            WindowEvent::RedrawRequested,
            WindowMetrics {
                occluded: true,
                ..metrics
            },
            Duration::ZERO,
        )
        .unwrap();
    assert_eq!(*window.log.lock().unwrap(), [ime::Command::Enabled(false)]);
    driver
        .event(WindowEvent::RedrawRequested, metrics, Duration::ZERO)
        .unwrap();
    window.log.lock().unwrap().clear();
    driver.application.fail = true;
    assert!(
        driver
            .event(WindowEvent::RedrawRequested, metrics, Duration::ZERO)
            .is_err()
    );
    assert_eq!(*window.log.lock().unwrap(), [ime::Command::Enabled(false)]);
    driver.application.fail = false;
    driver
        .event(WindowEvent::RedrawRequested, metrics, Duration::ZERO)
        .unwrap();
    window.log.lock().unwrap().clear();
    struct FailedPresenter;
    impl presenter::Presenter for FailedPresenter {
        fn present(&mut self, _: &Scene, _: Frame) -> Result<Presentation, PlatformError> {
            Err(PlatformError::Unsupported("failed presentation"))
        }
    }
    driver.presenter = Some(Box::new(FailedPresenter));
    assert!(matches!(
        driver.event(WindowEvent::RedrawRequested, metrics, Duration::ZERO),
        Err(PlatformError::Unsupported("failed presentation"))
    ));
    assert_eq!(*window.log.lock().unwrap(), [ime::Command::Enabled(false)]);
    driver.presenter = None;
    driver
        .event(WindowEvent::RedrawRequested, metrics, Duration::ZERO)
        .unwrap();
    window.log.lock().unwrap().clear();
    driver.disconnected();
    assert_eq!(*window.log.lock().unwrap(), [ime::Command::Enabled(false)]);
}

#[test]
fn animation_wakes_do_not_consume_business_timers_and_hidden_windows_stop_animation() {
    struct App {
        deadline: Option<Duration>,
        animation_calls: usize,
        business_wakes: usize,
    }
    impl Application for App {
        fn animation_deadline(&self) -> Option<Duration> {
            self.deadline
        }
        fn animate(
            &mut self,
            _: WindowMetrics,
            elapsed: Duration,
        ) -> Result<bool, ApplicationError> {
            self.animation_calls += 1;
            self.deadline = Some(elapsed + Duration::from_millis(500));
            Ok(true)
        }
        fn update(
            &mut self,
            event: WindowEvent,
            _: WindowMetrics,
            _: Duration,
        ) -> Result<WindowAction, ApplicationError> {
            if event == WindowEvent::Wake {
                self.business_wakes += 1;
            }
            Ok(WindowAction {
                wake_at: (event == WindowEvent::Focused(true)).then_some(Duration::from_secs(2)),
                ..Default::default()
            })
        }
        fn draw(
            &mut self,
            _: &mut Scene,
            _: WindowMetrics,
            _: Duration,
        ) -> Result<(), ApplicationError> {
            Ok(())
        }
    }
    let mut driver = Driver {
        application: App {
            deadline: Some(Duration::from_millis(500)),
            animation_calls: 0,
            business_wakes: 0,
        },
        backend: Backend::Cpu,
        limits: Default::default(),
        window: None,
        presenter: None,
        scene: Scene::new(),
        wake: Default::default(),
        ime: Default::default(),
        input_available: true,
    };
    let metrics = WindowMetrics {
        extent: zui_types::Extent::new(100, 100),
        scale: 1.0,
        occluded: false,
    };
    assert_eq!(
        driver
            .event(WindowEvent::Focused(true), metrics, Duration::ZERO)
            .unwrap()
            .wake_at,
        Some(Duration::from_millis(500))
    );
    let tick = driver
        .event(WindowEvent::Wake, metrics, Duration::from_millis(500))
        .unwrap();
    assert!(tick.redraw);
    assert_eq!(tick.wake_at, Some(Duration::from_secs(1)));
    assert_eq!(driver.application.animation_calls, 1);
    assert_eq!(driver.application.business_wakes, 0);
    let hidden = WindowMetrics {
        occluded: true,
        ..metrics
    };
    assert_eq!(
        driver
            .event(
                WindowEvent::Occluded(true),
                hidden,
                Duration::from_millis(750)
            )
            .unwrap()
            .wake_at,
        Some(Duration::from_secs(2))
    );
    assert!(
        !driver
            .event(WindowEvent::Wake, hidden, Duration::from_secs(1))
            .unwrap()
            .redraw
    );
    assert_eq!(driver.application.animation_calls, 1);
    assert_eq!(driver.application.business_wakes, 0);
    driver
        .event(WindowEvent::Wake, hidden, Duration::from_secs(2))
        .unwrap();
    assert_eq!(driver.application.business_wakes, 1);
    assert_eq!(driver.application.animation_calls, 1);
}
