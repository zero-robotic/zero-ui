//! Application and window runner for the standard winit + ActiveRenderer stack.
//!
//! The runner owns platform-to-UI event translation, layout, redraw scheduling
//! and renderer surface management so applications only provide a root widget
//! (or a declarative [`Component`]).

use std::{cell::RefCell, rc::Rc, time::Instant};

use zui_backend_winit::{WinitBackend, WinitHost};
use zui_core::{Dip, Point, Size};
use zui_platform::{Host, InputEvent, PlatformEvent, WindowOptions};
use zui_render::{ImageId, ImageResource, RenderError, SurfaceMetrics};
use zui_render_runtime::{ActiveRenderer, RendererError};
use zui_ui::{
    Component, ComponentRoot, Constraints, EventResult, Theme, UiEvent, Widget, WidgetTree,
};

/// Configures and launches a one-window application.
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

    pub fn run(self, root: impl Widget + 'static) -> Result<(), Box<dyn std::error::Error>> {
        WindowRunner::new(self.options, self.theme, self.images, root).run()
    }

    pub fn run_component<C: Component>(
        self,
        component: C,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.run(ComponentRoot::new(component))
    }
}

/// Owns a `WidgetTree`, the selected renderer and the event-loop integration.
pub struct WindowRunner {
    options: WindowOptions,
    theme: Theme,
    tree: WidgetTree,
    images: Vec<(ImageId, ImageResource)>,
}

impl WindowRunner {
    pub fn new(
        options: WindowOptions,
        theme: Theme,
        images: Vec<(ImageId, ImageResource)>,
        root: impl Widget + 'static,
    ) -> Self {
        let mut tree = WidgetTree::new(root);
        tree.set_theme(theme.clone());
        Self {
            options,
            theme,
            tree,
            images,
        }
    }

    pub fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        let state = Rc::new(RefCell::new(RunnerState {
            renderer: {
                let mut renderer = ActiveRenderer::new_blocking()?;
                for (id, image) in self.images {
                    renderer.register_image(id, image);
                }
                renderer
            },
            tree: self.tree,
            background: self.theme.background,
            pointer_position: Point {
                x: Dip::ZERO,
                y: Dip::ZERO,
            },
            pending_resize: None,
        }));

        let window_state = Rc::clone(&state);
        let mut on_window = move |host: &WinitHost| {
            window_state.borrow_mut().on_window(host);
        };
        let event_state = Rc::clone(&state);
        let mut handler = move |event: PlatformEvent| event_state.borrow_mut().handle(event);

        WinitBackend::new()?.run_with_options_and_schedule(
            self.options,
            &mut on_window,
            &mut handler,
        )?;
        Ok(())
    }
}

struct RunnerState {
    renderer: ActiveRenderer,
    tree: WidgetTree,
    background: zui_core::Color,
    /// Last pointer position in native window DIPs. UI layout uses the same
    /// coordinate space, independent of the physical monitor scale factor.
    pointer_position: Point,
    /// Native resize notifications can arrive much faster than the GPU can
    /// recreate render targets. Retain only the latest dimensions.
    pending_resize: Option<(zui_core::WindowId, Size, SurfaceMetrics)>,
}

impl RunnerState {
    fn on_window(&mut self, host: &WinitHost) {
        self.tree.layout(Constraints::loose(host.size()));
        self.tree.request_paint(None);
        self.renderer
            .attach_surface(
                host.id(),
                host,
                native_surface_metrics(host.size(), host.scale_factor()),
            )
            .expect("failed to attach render surface");
        host.request_redraw()
            .expect("failed to request initial redraw");
    }

    fn handle(&mut self, event: PlatformEvent) -> Option<Instant> {
        match event {
            PlatformEvent::RedrawRequested(window) => self.redraw(window),
            PlatformEvent::WindowResized {
                window,
                size,
                scale_factor,
            } => {
                // A larger window increases the layout constraints but never
                // changes the size of one DIP. Intrinsic controls therefore
                // stay fixed while explicitly flexible layout children can
                // consume the newly available space.
                let metrics = native_surface_metrics(size, scale_factor);
                self.pending_resize = Some((window, size, metrics));
                // Keep only the newest native size, but present it on the next
                // compositor-driven redraw. Multiple requests made before that
                // redraw are coalesced by winit, allowing live resize/maximize
                // to track the window without rebuilding once per raw event.
                Some(Instant::now())
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
                (result == EventResult::RequestRedraw).then(Instant::now)
            }
            PlatformEvent::CloseRequested(window) => {
                self.renderer.detach_surface(window);
                None
            }
            PlatformEvent::WindowCreated(_) | PlatformEvent::AboutToWait => None,
        }
    }

    fn redraw(&mut self, window: zui_core::WindowId) -> Option<Instant> {
        if let Some((resize_window, size, metrics)) = self.pending_resize.take() {
            if metrics.physical_size != zui_core::PhysicalSize::default() {
                self.tree.layout(Constraints::loose(size));
                self.renderer
                    .resize(resize_window, metrics)
                    .expect("failed to resize render surface");
                self.tree.request_paint(None);
            }
        }
        if !self.tree.needs_redraw() {
            if self.tree.next_redraw().is_some() {
                self.tree.request_paint(None);
            } else {
                return None;
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
                eprintln!("render surface lost; waiting for resize")
            }
            Err(error) => eprintln!("failed to render frame: {error}"),
        }
        self.tree.next_redraw()
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
}
