//! Platform-neutral application orchestration.
//!
//! [`Application::run_with`] accepts a platform backend and renderer. The
//! default `winit` feature adds the [`Application::run`] convenience path but
//! does not change the shared `AppLoop` lifecycle used by other backends.

#[cfg(feature = "winit")]
use zui_backend_winit::WinitBackend;
use zui_core::{Dip, Point, Size};
use zui_platform::{AppLoop, Backend, Host, InputEvent, LoopControl, PlatformEvent, WindowOptions};
use zui_render::{ImageId, ImageResource, RenderError, SurfaceMetrics};
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

    fn event(&mut self, event: PlatformEvent) -> LoopControl {
        match self.handle::<H>(event) {
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

    fn handle<H>(&mut self, event: PlatformEvent) -> Result<LoopControl, RendererError>
    where
        H: Host,
        R: ApplicationRenderer<H>,
    {
        match event {
            PlatformEvent::RedrawRequested(window) => self.redraw::<H>(window),
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
                Ok(LoopControl::Continue)
            }
            PlatformEvent::WindowCreated(_) | PlatformEvent::AboutToWait => {
                Ok(LoopControl::Continue)
            }
        }
    }

    fn redraw<H>(&mut self, window: zui_core::WindowId) -> Result<LoopControl, RendererError>
    where
        H: Host,
        R: ApplicationRenderer<H>,
    {
        if let Some((resize_window, size, metrics)) = self.pending_resize.take() {
            if metrics.physical_size != zui_core::PhysicalSize::default() {
                self.tree.layout(Constraints::loose(size));
                self.renderer.resize(resize_window, metrics)?;
                self.tree.request_paint(None);
            }
        }
        if !self.tree.needs_redraw() {
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
            Ok(()) => self.tree.mark_clean(),
            Err(RendererError::HardwareGpu(RenderError::SurfaceLost)) => {
                return Ok(LoopControl::Continue);
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
    use zui_render::PaintCommand;
    use zui_render_runtime::HeadlessRenderer;
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
}
