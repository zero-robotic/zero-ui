//! Platform-neutral application orchestration.
//!
//! [`Application::run_with`] accepts a platform backend and renderer. The
//! default `winit` feature adds the [`Application::run`] convenience path but
//! does not change the shared `AppLoop` lifecycle used by other backends.

use std::time::{Duration, Instant};

#[cfg(feature = "winit")]
use zui_backend_winit::WinitBackend;
use zui_core::{Dip, Point, Size};
use zui_platform::{AppLoop, Backend, Host, InputEvent, LoopControl, PlatformEvent, WindowOptions};
use zui_render::{
    FrameDeferReason, FrameOutcome, ImageId, ImageResource, RenderError, SurfaceMetrics,
};
#[cfg(feature = "winit")]
use zui_render_runtime::ActiveRenderer;
use zui_render_runtime::{ApplicationRenderer, RendererError};
use zui_ui::{
    Component, ComponentRoot, Constraints, EventResult, Theme, UiEvent, Widget, WidgetTree,
};

/// Configures an application independently of its platform and renderer.
pub struct Application {
    options: WindowOptions,
    theme: Theme,
    images: Vec<(ImageId, ImageResource)>,
}

impl Default for Application {
    fn default() -> Self {
        Self::new()
    }
}

impl Application {
    pub fn new() -> Self {
        Self {
            options: WindowOptions::default(),
            theme: Theme::default(),
            images: Vec::new(),
        }
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.options.title = title.into();
        self
    }

    pub fn options(mut self, options: WindowOptions) -> Self {
        self.options = options;
        self
    }

    pub fn theme(mut self, theme: Theme) -> Self {
        self.theme = theme;
        self
    }

    pub fn image(mut self, id: ImageId, image: ImageResource) -> Self {
        self.images.push((id, image));
        self
    }

    /// Runs with explicitly selected platform and renderer implementations,
    /// returning the renderer after finite backends (such as headless) exit.
    pub fn run_with<B, R>(
        self,
        backend: B,
        renderer: R,
        root: impl Widget + 'static,
    ) -> Result<R, Box<dyn std::error::Error>>
    where
        B: Backend,
        R: ApplicationRenderer<B::Host>,
    {
        WindowRunner::new(self.options, self.theme, self.images, renderer, root).run(backend)
    }

    pub fn run_component_with<B, R, C>(
        self,
        backend: B,
        renderer: R,
        component: C,
    ) -> Result<R, Box<dyn std::error::Error>>
    where
        B: Backend,
        R: ApplicationRenderer<B::Host>,
        C: Component,
    {
        self.run_with(backend, renderer, ComponentRoot::new(component))
    }

    /// Runs the standard native stack selected by the default `winit` feature.
    #[cfg(feature = "winit")]
    pub fn run(self, root: impl Widget + 'static) -> Result<(), Box<dyn std::error::Error>> {
        let backend = WinitBackend::new()?;
        let renderer = ActiveRenderer::new_blocking()?;
        self.run_with(backend, renderer, root).map(|_| ())
    }

    #[cfg(feature = "winit")]
    pub fn run_component<C: Component>(
        self,
        component: C,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let backend = WinitBackend::new()?;
        let renderer = ActiveRenderer::new_blocking()?;
        self.run_component_with(backend, renderer, component)
            .map(|_| ())
    }
}

/// Owns one `WidgetTree` and an injected renderer while a backend drives it.
pub struct WindowRunner<R> {
    options: WindowOptions,
    theme: Theme,
    tree: WidgetTree,
    images: Vec<(ImageId, ImageResource)>,
    renderer: R,
}

impl<R> WindowRunner<R> {
    pub fn new(
        options: WindowOptions,
        theme: Theme,
        images: Vec<(ImageId, ImageResource)>,
        renderer: R,
        root: impl Widget + 'static,
    ) -> Self {
        let mut tree = WidgetTree::new(root);
        tree.set_theme(theme.clone());
        Self {
            options,
            theme,
            tree,
            images,
            renderer,
        }
    }

    pub fn run<B>(mut self, backend: B) -> Result<R, Box<dyn std::error::Error>>
    where
        B: Backend,
        R: ApplicationRenderer<B::Host>,
    {
        for (id, image) in self.images {
            self.renderer.register_image(id, image);
        }
        let mut state = RunnerState {
            renderer: self.renderer,
            tree: self.tree,
            background: self.theme.background,
            pointer_position: Point {
                x: Dip::ZERO,
                y: Dip::ZERO,
            },
            pending_resize: None,
            pending_present: false,
            error: None,
        };

        backend.run(self.options, &mut state)?;
        if let Some(error) = state.error.take() {
            return Err(Box::new(error));
        }
        Ok(state.renderer)
    }
}

struct RunnerState<R> {
    renderer: R,
    tree: WidgetTree,
    background: zui_core::Color,
    /// Last pointer position in native window DIPs. UI layout uses the same
    /// coordinate space, independent of the physical monitor scale factor.
    pointer_position: Point,
    /// Native resize notifications can arrive much faster than the GPU can
    /// recreate render targets. Retain only the latest dimensions.
    pending_resize: Option<(zui_core::WindowId, Size, SurfaceMetrics)>,
    /// The renderer accepted the latest scene but could not present it because
    /// the native drawable was temporarily unavailable.
    pending_present: bool,
    error: Option<RendererError>,
}

impl<H, R> AppLoop<H> for RunnerState<R>
where
    H: Host,
    R: ApplicationRenderer<H>,
{
    fn host_ready(&mut self, host: &H) -> LoopControl {
        match self.attach_host(host) {
            Ok(()) => LoopControl::RequestRedraw,
            Err(error) => self.fail(error),
        }
    }

    fn event(&mut self, host: &H, event: PlatformEvent) -> LoopControl {
        match self.handle(host, event) {
            Ok(control) => control,
            Err(error) => self.fail(error),
        }
    }
}

impl<R> RunnerState<R> {
    fn fail(&mut self, error: RendererError) -> LoopControl {
        self.error = Some(error);
        LoopControl::Exit
    }

    fn attach_host<H>(&mut self, host: &H) -> Result<(), RendererError>
    where
        H: Host,
        R: ApplicationRenderer<H>,
    {
        self.tree.layout(Constraints::loose(host.size()));
        self.tree.request_paint(None);
        self.renderer.attach_surface(
            host.id(),
            host,
            native_surface_metrics(host.size(), host.scale_factor()),
        )
    }

    fn handle<H>(&mut self, host: &H, event: PlatformEvent) -> Result<LoopControl, RendererError>
    where
        H: Host,
        R: ApplicationRenderer<H>,
    {
        match event {
            PlatformEvent::RedrawRequested(window) => self.redraw(host, window),
            PlatformEvent::WindowResized {
                window,
                size,
                scale_factor,
            } => {
                let metrics = native_surface_metrics(size, scale_factor);
                self.pending_resize = Some((window, size, metrics));
                Ok(LoopControl::RequestRedraw)
            }
            PlatformEvent::Input { window, event } => {
                let ui_event = match event {
                    InputEvent::CursorMoved { position } => {
                        self.pointer_position = position;
                        UiEvent::pointer(
                            Some(window),
                            position,
                            InputEvent::CursorMoved { position },
                        )
                    }
                    InputEvent::MouseInput { .. } => {
                        UiEvent::pointer(Some(window), self.pointer_position, event)
                    }
                    _ => UiEvent::input(event),
                };
                let (result, _outputs) = self.tree.event(&ui_event);
                Ok(if result == EventResult::RequestRedraw {
                    LoopControl::RequestRedraw
                } else {
                    LoopControl::Continue
                })
            }
            PlatformEvent::CloseRequested(window) => {
                self.renderer.detach_surface(window);
                self.pending_present = false;
                Ok(LoopControl::Continue)
            }
            PlatformEvent::WindowOccluded {
                occluded: false, ..
            } if self.pending_present || self.tree.needs_redraw() => Ok(LoopControl::RequestRedraw),
            PlatformEvent::WindowOccluded { .. } => Ok(LoopControl::Continue),
            PlatformEvent::WindowCreated(_) | PlatformEvent::AboutToWait => {
                Ok(LoopControl::Continue)
            }
        }
    }

    fn redraw<H>(
        &mut self,
        host: &H,
        window: zui_core::WindowId,
    ) -> Result<LoopControl, RendererError>
    where
        H: Host,
        R: ApplicationRenderer<H>,
    {
        if let Some((resize_window, size, metrics)) = self.pending_resize.take() {
            if metrics.physical_size != zui_core::PhysicalSize::default() {
                self.tree.layout(Constraints::loose(size));
                self.renderer.resize(resize_window, metrics)?;
                self.tree.request_paint(None);
                self.pending_present = false;
            }
        }
        if !self.tree.needs_redraw() && !self.pending_present {
            if self.tree.next_redraw().is_some() {
                self.tree.request_paint(None);
            } else {
                return Ok(LoopControl::Continue);
            }
        }
        let result = {
            let scene = self.tree.scene_submission();
            self.renderer.render_scene(
                window,
                scene.node,
                self.background,
                scene.index,
                scene.update,
            )
        };
        match result {
            Ok(FrameOutcome::Presented) => {
                self.tree.mark_clean();
                self.pending_present = false;
            }
            Ok(FrameOutcome::Deferred(reason)) => {
                // The renderer retained this exact scene revision, so the UI
                // can clear its build dirtiness while keeping presentation
                // pending. A retry submits the cached scene with no update.
                self.tree.mark_clean();
                self.pending_present = true;
                return Ok(match reason {
                    FrameDeferReason::Timeout => {
                        LoopControl::WaitUntil(Instant::now() + Duration::from_millis(16))
                    }
                    FrameDeferReason::Occluded => LoopControl::Continue,
                });
            }
            Err(RendererError::HardwareGpu(
                RenderError::SurfaceLost | RenderError::SurfaceOutdated,
            )) => {
                // Recreate rather than merely resize: wgpu requires a lost
                // surface to be recreated from the live native host.
                self.renderer.detach_surface(window);
                self.renderer.attach_surface(
                    window,
                    host,
                    native_surface_metrics(host.size(), host.scale_factor()),
                )?;
                self.tree.request_paint(None);
                self.pending_present = false;
                return Ok(LoopControl::RequestRedraw);
            }
            Err(error) => return Err(error),
        }
        Ok(LoopControl::from_redraw_deadline(self.tree.next_redraw()))
    }
}

fn native_surface_metrics(
    size: Size,
    device_scale_factor: zui_core::ScaleFactor,
) -> SurfaceMetrics {
    SurfaceMetrics::new(
        device_scale_factor.to_physical(size),
        device_scale_factor,
        1.0,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use zui_backend_headless::HeadlessBackend;
    use zui_render::{FrameDeferReason, FrameOutcome, PaintCommand, RenderNode, RenderNodeIndex};
    use zui_render_runtime::{ApplicationRenderer, HeadlessRenderer};
    use zui_ui::Text;

    fn contains_text(node: &zui_render::RenderNode, expected: &str) -> bool {
        node.commands
            .iter()
            .any(|command| matches!(command, PaintCommand::Text { text, .. } if text == expected))
            || node
                .children
                .iter()
                .any(|child| contains_text(child, expected))
    }

    #[test]
    fn window_growth_does_not_change_pixels_per_dip() {
        let scale = zui_core::ScaleFactor(2.0);
        let metrics = |width, height| {
            native_surface_metrics(
                Size {
                    width: Dip(width),
                    height: Dip(height),
                },
                scale,
            )
        };

        let initial = metrics(800.0, 600.0);
        let enlarged = metrics(1600.0, 1000.0);

        assert_eq!(initial.pixels_per_content_dip(), scale);
        assert_eq!(enlarged.pixels_per_content_dip(), scale);
        assert_eq!(initial.ui_scale, 1.0);
        assert_eq!(enlarged.ui_scale, 1.0);
    }

    #[test]
    fn headless_backend_drives_widget_tree_scene_into_a_frame() {
        let root = Text::new("headless end-to-end frame");
        let root_id = root.id().value();
        let options = WindowOptions {
            size: Size {
                width: Dip(320.0),
                height: Dip(180.0),
            },
            ..WindowOptions::default()
        };

        let renderer = Application::new()
            .options(options)
            .run_with(HeadlessBackend::new(), HeadlessRenderer::new(), root)
            .expect("headless application should complete");

        let frame = renderer
            .last_frame()
            .expect("headless application should submit one frame");
        assert_eq!(renderer.frames().len(), 1);
        assert_eq!(frame.metrics.physical_size.width, 320);
        assert_eq!(frame.metrics.physical_size.height, 180);
        assert!(frame.update.full_rebuild());
        assert!(frame.update.revision() > 0);
        assert!(frame.index.path_for(root_id).is_some());
        assert!(contains_text(&frame.node, "headless end-to-end frame"));
    }

    #[derive(Default)]
    struct DeferredOnceRenderer {
        attempts: usize,
        updates: Vec<(bool, bool, u64)>,
        attach_count: usize,
        detach_count: usize,
        lose_first_surface: bool,
        occlude_first_frame: bool,
    }

    impl<H: Host> ApplicationRenderer<H> for DeferredOnceRenderer {
        fn register_image(&mut self, _id: ImageId, _image: ImageResource) {}

        fn attach_surface(
            &mut self,
            _window: zui_core::WindowId,
            _host: &H,
            _metrics: SurfaceMetrics,
        ) -> Result<(), RendererError> {
            self.attach_count += 1;
            Ok(())
        }

        fn resize(
            &mut self,
            _window: zui_core::WindowId,
            _metrics: SurfaceMetrics,
        ) -> Result<(), RendererError> {
            Ok(())
        }

        fn detach_surface(&mut self, _window: zui_core::WindowId) {
            self.detach_count += 1;
        }

        fn render_scene(
            &mut self,
            _window: zui_core::WindowId,
            _node: &RenderNode,
            _clear: zui_core::Color,
            _index: &RenderNodeIndex,
            update: &zui_render::SceneUpdate,
        ) -> Result<FrameOutcome, RendererError> {
            self.attempts += 1;
            self.updates
                .push((update.full_rebuild(), update.is_empty(), update.revision()));
            if self.attempts == 1 {
                if self.lose_first_surface {
                    Err(RendererError::HardwareGpu(RenderError::SurfaceLost))
                } else if self.occlude_first_frame {
                    Ok(FrameOutcome::Deferred(FrameDeferReason::Occluded))
                } else {
                    Ok(FrameOutcome::Deferred(FrameDeferReason::Timeout))
                }
            } else {
                Ok(FrameOutcome::Presented)
            }
        }
    }

    #[test]
    fn deferred_frame_is_retried_without_rebuilding_the_accepted_scene() {
        let renderer = Application::new()
            .run_with(
                HeadlessBackend::new(),
                DeferredOnceRenderer::default(),
                Text::new("deferred frame"),
            )
            .expect("a deferred frame should be retried");

        assert_eq!(renderer.attempts, 2);
        assert_eq!(renderer.attach_count, 1);
        assert_eq!(renderer.detach_count, 0);
        assert!(renderer.updates[0].0);
        assert!(!renderer.updates[0].1);
        assert!(renderer.updates[1].1);
        assert_eq!(renderer.updates[1].2, renderer.updates[0].2);
    }

    #[test]
    fn lost_surface_is_recreated_and_forces_a_complete_frame() {
        let renderer = Application::new()
            .run_with(
                HeadlessBackend::new(),
                DeferredOnceRenderer {
                    lose_first_surface: true,
                    ..DeferredOnceRenderer::default()
                },
                Text::new("surface recovery"),
            )
            .expect("a lost surface should be recreated");

        assert_eq!(renderer.attempts, 2);
        assert_eq!(renderer.attach_count, 2);
        assert_eq!(renderer.detach_count, 1);
        assert!(renderer.updates[0].0);
        assert!(renderer.updates[1].0);
        assert!(!renderer.updates[1].1);
        assert!(renderer.updates[1].2 > renderer.updates[0].2);
    }

    struct VisibilityHost {
        id: zui_core::WindowId,
        size: Size,
    }

    impl Host for VisibilityHost {
        fn id(&self) -> zui_core::WindowId {
            self.id
        }

        fn size(&self) -> Size {
            self.size
        }

        fn scale_factor(&self) -> zui_core::ScaleFactor {
            zui_core::ScaleFactor::default()
        }

        fn request_redraw(&self) -> Result<(), zui_platform::PlatformError> {
            Ok(())
        }
    }

    struct OcclusionCycleBackend;

    impl Backend for OcclusionCycleBackend {
        type Host = VisibilityHost;

        fn run(
            self,
            options: WindowOptions,
            app: &mut dyn AppLoop<Self::Host>,
        ) -> Result<(), zui_platform::PlatformError> {
            let host = VisibilityHost {
                id: zui_core::WindowId(zui_core::Id::new(77)),
                size: options.size,
            };
            let created = app.event(&host, PlatformEvent::WindowCreated(host.id));
            assert!(!matches!(created, LoopControl::Exit));
            assert_eq!(app.host_ready(&host), LoopControl::RequestRedraw);

            assert_eq!(
                app.event(&host, PlatformEvent::RedrawRequested(host.id)),
                LoopControl::Continue
            );
            assert_eq!(
                app.event(
                    &host,
                    PlatformEvent::WindowOccluded {
                        window: host.id,
                        occluded: false,
                    },
                ),
                LoopControl::RequestRedraw
            );
            let _ = app.event(&host, PlatformEvent::RedrawRequested(host.id));
            Ok(())
        }
    }

    #[test]
    fn deferred_occluded_frame_is_presented_after_window_reappears() {
        let renderer = Application::new()
            .run_with(
                OcclusionCycleBackend,
                DeferredOnceRenderer {
                    occlude_first_frame: true,
                    ..DeferredOnceRenderer::default()
                },
                Text::new("occlusion recovery"),
            )
            .expect("an unoccluded window should present its retained scene");

        assert_eq!(renderer.attempts, 2);
        assert!(renderer.updates[0].0);
        assert!(renderer.updates[1].1);
        assert_eq!(renderer.updates[1].2, renderer.updates[0].2);
    }
}
