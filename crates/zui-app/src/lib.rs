//! Platform-neutral application orchestration.
//!
//! [`Application::run_with`] accepts a platform backend and renderer. The
//! default `winit` feature adds the [`Application::run`] convenience path but
//! does not change the shared `AppLoop` lifecycle used by other backends.

use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};

#[cfg(feature = "winit")]
use zui_backend_winit::WinitBackend;
use zui_core::{Dip, Point, Size};
use zui_platform::{
    AppContext, AppLoop, Backend, InputEvent, LoopControl, PlatformError, PlatformEvent,
    WindowOptions,
};
use zui_render::{
    FrameDeferReason, FrameOutcome, ImageId, ImageResource, RenderError, RendererOptions,
    SurfaceMetrics,
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
    renderer_options: RendererOptions,
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
            renderer_options: RendererOptions::default(),
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

    /// Configures native renderer quality without leaking GPU objects into
    /// the application or widget layers.
    pub fn renderer_options(mut self, options: RendererOptions) -> Self {
        self.renderer_options = options;
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
        R: ApplicationRenderer,
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
        R: ApplicationRenderer,
        C: Component,
    {
        self.run_with(backend, renderer, ComponentRoot::new(component))
    }

    /// Runs the standard native stack selected by the default `winit` feature.
    #[cfg(feature = "winit")]
    pub fn run(self, root: impl Widget + 'static) -> Result<(), Box<dyn std::error::Error>> {
        let backend = WinitBackend::new()?;
        let renderer = ActiveRenderer::new_blocking_with_options(self.renderer_options)?;
        self.run_with(backend, renderer, root).map(|_| ())
    }

    #[cfg(feature = "winit")]
    pub fn run_component<C: Component>(
        self,
        component: C,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let backend = WinitBackend::new()?;
        let renderer = ActiveRenderer::new_blocking_with_options(self.renderer_options)?;
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
        R: ApplicationRenderer,
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
            pointer_positions: HashMap::new(),
            pending_resizes: HashMap::new(),
            pending_present: HashSet::new(),
            attached_windows: HashSet::new(),
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
    pointer_positions: HashMap<zui_core::WindowId, Point>,
    /// Native resize notifications can arrive much faster than the GPU can
    /// recreate render targets. Retain only the latest dimensions.
    pending_resizes: HashMap<zui_core::WindowId, (Size, SurfaceMetrics)>,
    /// The renderer accepted the latest scene but could not present it because
    /// the native drawable was temporarily unavailable.
    pending_present: HashSet<zui_core::WindowId>,
    attached_windows: HashSet<zui_core::WindowId>,
    error: Option<RendererError>,
}

impl<R: ApplicationRenderer> AppLoop for RunnerState<R> {
    fn event(&mut self, context: &mut dyn AppContext, event: PlatformEvent) -> LoopControl {
        match self.handle(context, event) {
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

    fn attach_host(
        &mut self,
        context: &mut dyn AppContext,
        window: zui_core::WindowId,
    ) -> Result<(), RendererError>
    where
        R: ApplicationRenderer,
    {
        let host = context.host(window).ok_or_else(|| {
            RendererError::Backend(PlatformError::UnknownWindow(window).to_string())
        })?;
        let size = host.size();
        let scale = host.scale_factor();
        let target = host.surface_target();
        self.tree.layout(Constraints::loose(size));
        self.tree.request_paint(None);
        self.renderer
            .attach_surface(window, target, native_surface_metrics(size, scale))?;
        self.attached_windows.insert(window);
        self.pending_present.insert(window);
        Ok(())
    }

    fn handle(
        &mut self,
        context: &mut dyn AppContext,
        event: PlatformEvent,
    ) -> Result<LoopControl, RendererError>
    where
        R: ApplicationRenderer,
    {
        match event {
            PlatformEvent::WindowCreated(window) => {
                self.attach_host(context, window)?;
                Ok(LoopControl::RequestRedraw(window))
            }
            PlatformEvent::RedrawRequested(window) => self.redraw(context, window),
            PlatformEvent::WindowResized { window, size } => {
                let scale_factor = context
                    .host(window)
                    .ok_or_else(|| {
                        RendererError::Backend(PlatformError::UnknownWindow(window).to_string())
                    })?
                    .scale_factor();
                let metrics = native_surface_metrics(size, scale_factor);
                self.pending_resizes.insert(window, (size, metrics));
                Ok(LoopControl::RequestRedraw(window))
            }
            PlatformEvent::ScaleFactorChanged {
                window,
                size,
                scale_factor,
            } => {
                let metrics = native_surface_metrics(size, scale_factor);
                self.pending_resizes.insert(window, (size, metrics));
                Ok(LoopControl::RequestRedraw(window))
            }
            PlatformEvent::Input { window, event } => {
                let ui_event = match event {
                    InputEvent::CursorMoved { position } => {
                        self.pointer_position = position;
                        self.pointer_positions.insert(window, position);
                        UiEvent::pointer(
                            Some(window),
                            position,
                            InputEvent::CursorMoved { position },
                        )
                    }
                    InputEvent::MouseInput { .. } => {
                        let position = self
                            .pointer_positions
                            .get(&window)
                            .copied()
                            .unwrap_or(self.pointer_position);
                        UiEvent::pointer(Some(window), position, event)
                    }
                    _ => UiEvent::input(event),
                };
                let (result, actions) = self.tree.event(&ui_event);
                if actions
                    .iter()
                    .any(|action| action.kind == zui_ui::ActionKind::FocusRequested)
                {
                    if let Some(ime) = context.capabilities().ime() {
                        ime.set_enabled(window, true)
                            .map_err(|error| RendererError::Backend(error.to_string()))?;
                        let position = self
                            .pointer_positions
                            .get(&window)
                            .copied()
                            .unwrap_or_default();
                        ime.set_cursor_area(
                            window,
                            zui_core::Rect {
                                origin: position,
                                size: Size {
                                    width: Dip(1.0),
                                    height: Dip(26.0),
                                },
                            },
                        )
                        .map_err(|error| RendererError::Backend(error.to_string()))?;
                    }
                }
                Ok(if result == EventResult::RequestRedraw {
                    LoopControl::RequestRedraw(window)
                } else {
                    LoopControl::Continue
                })
            }
            PlatformEvent::CloseRequested(window) => {
                if self.attached_windows.remove(&window) {
                    self.renderer.detach_surface(window);
                }
                self.pending_present.remove(&window);
                Ok(LoopControl::Continue)
            }
            PlatformEvent::WindowDestroyed(window) => {
                if self.attached_windows.remove(&window) {
                    self.renderer.detach_surface(window);
                }
                self.pending_present.remove(&window);
                self.pending_resizes.remove(&window);
                self.pointer_positions.remove(&window);
                Ok(LoopControl::Continue)
            }
            PlatformEvent::WindowOccluded {
                window,
                occluded: false,
            } if self.pending_present.contains(&window) || self.tree.needs_redraw() => {
                Ok(LoopControl::RequestRedraw(window))
            }
            PlatformEvent::WindowOccluded { .. } => Ok(LoopControl::Continue),
            PlatformEvent::OutputsChanged(_)
            | PlatformEvent::DragDrop(_)
            | PlatformEvent::DialogCompleted(_)
            | PlatformEvent::AboutToWait => Ok(LoopControl::Continue),
        }
    }

    fn redraw(
        &mut self,
        context: &mut dyn AppContext,
        window: zui_core::WindowId,
    ) -> Result<LoopControl, RendererError>
    where
        R: ApplicationRenderer,
    {
        if let Some((size, metrics)) = self.pending_resizes.remove(&window) {
            self.tree.layout(Constraints::loose(size));
            self.renderer.resize(window, metrics)?;
            self.tree.request_paint(None);
            self.pending_present.remove(&window);
        }
        if !self.tree.needs_redraw() && !self.pending_present.contains(&window) {
            if self.tree.next_redraw().is_some() {
                self.tree.request_paint(None);
            } else {
                return Ok(LoopControl::Continue);
            }
        }
        let semantics = accessibility_node(self.tree.semantics());
        if let Some(accessibility) = context.capabilities().accessibility() {
            accessibility
                .submit_tree(zui_platform::AccessibilityTree {
                    window,
                    root: semantics,
                })
                .map_err(|error| RendererError::Backend(error.to_string()))?;
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
                self.pending_present.remove(&window);
            }
            Ok(FrameOutcome::Deferred(reason)) => {
                // The renderer retained this exact scene revision, so the UI
                // can clear its build dirtiness while keeping presentation
                // pending. A retry submits the cached scene with no update.
                self.tree.mark_clean();
                self.pending_present.insert(window);
                return Ok(match reason {
                    FrameDeferReason::Timeout => LoopControl::WaitUntil {
                        window,
                        deadline: Instant::now() + Duration::from_millis(16),
                    },
                    FrameDeferReason::Occluded => LoopControl::Continue,
                });
            }
            Err(RendererError::HardwareGpu(
                RenderError::SurfaceLost | RenderError::SurfaceOutdated,
            )) => {
                // Renderer retains the backend-produced SurfaceTarget and is
                // the sole owner of native surface recovery.
                let host = context.host(window).ok_or_else(|| {
                    RendererError::Backend(PlatformError::UnknownWindow(window).to_string())
                })?;
                self.renderer.recover_surface(
                    window,
                    native_surface_metrics(host.size(), host.scale_factor()),
                )?;
                self.tree.request_paint(None);
                self.pending_present.remove(&window);
                return Ok(LoopControl::RequestRedraw(window));
            }
            Err(error) => return Err(error),
        }
        Ok(LoopControl::from_redraw_deadline(
            window,
            self.tree.next_redraw(),
        ))
    }
}

fn accessibility_node(node: zui_ui::SemanticsNode) -> zui_platform::AccessibilityNode {
    let role = match node.role {
        zui_ui::SemanticRole::Generic => zui_platform::AccessibilityRole::Generic,
        zui_ui::SemanticRole::Text => zui_platform::AccessibilityRole::Text,
        zui_ui::SemanticRole::Button => zui_platform::AccessibilityRole::Button,
        zui_ui::SemanticRole::TextInput => zui_platform::AccessibilityRole::TextInput,
        zui_ui::SemanticRole::Group => zui_platform::AccessibilityRole::Group,
    };
    zui_platform::AccessibilityNode {
        id: zui_platform::AccessibilityNodeId(node.id.value()),
        role,
        label: node.label,
        enabled: node.enabled,
        bounds: node.bounds,
        children: node.children.into_iter().map(accessibility_node).collect(),
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
    use std::sync::{Arc, Mutex};
    use zui_backend_headless::HeadlessBackend;
    use zui_platform::{Capabilities, Host, Paths, UiTaskPoster};
    use zui_render::{
        FrameDeferReason, FrameOutcome, PaintCommand, RenderNode, RenderNodeIndex,
        SurfaceLifecycle, SurfacePhase,
    };
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
        recover_count: usize,
        resize_metrics: Vec<SurfaceMetrics>,
        lifecycle: SurfaceLifecycle,
        lose_first_surface: bool,
        occlude_first_frame: bool,
    }

    impl ApplicationRenderer for DeferredOnceRenderer {
        fn register_image(&mut self, _id: ImageId, _image: ImageResource) {}

        fn attach_surface(
            &mut self,
            _window: zui_core::WindowId,
            _target: zui_platform::SurfaceTarget,
            _metrics: SurfaceMetrics,
        ) -> Result<(), RendererError> {
            self.attach_count += 1;
            self.lifecycle
                .transition(SurfacePhase::Attached)
                .map_err(|error| RendererError::Backend(error.to_string()))?;
            Ok(())
        }

        fn resize(
            &mut self,
            _window: zui_core::WindowId,
            metrics: SurfaceMetrics,
        ) -> Result<(), RendererError> {
            self.resize_metrics.push(metrics);
            self.lifecycle
                .transition(SurfacePhase::Resized)
                .map_err(|error| RendererError::Backend(error.to_string()))?;
            Ok(())
        }

        fn recover_surface(
            &mut self,
            _window: zui_core::WindowId,
            _metrics: SurfaceMetrics,
        ) -> Result<(), RendererError> {
            self.recover_count += 1;
            self.lifecycle
                .transition(SurfacePhase::Recovered)
                .map_err(|error| RendererError::Backend(error.to_string()))
        }

        fn detach_surface(&mut self, _window: zui_core::WindowId) {
            self.detach_count += 1;
            let _ = self.lifecycle.transition(SurfacePhase::Detached);
        }

        fn surface_phase(&self, _window: zui_core::WindowId) -> SurfacePhase {
            self.lifecycle.phase()
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
                    self.lifecycle
                        .transition(SurfacePhase::Lost)
                        .map_err(|error| RendererError::Backend(error.to_string()))?;
                    Err(RendererError::HardwareGpu(RenderError::SurfaceLost))
                } else if self.occlude_first_frame {
                    self.lifecycle
                        .transition(SurfacePhase::Deferred(FrameDeferReason::Occluded))
                        .map_err(|error| RendererError::Backend(error.to_string()))?;
                    Ok(FrameOutcome::Deferred(FrameDeferReason::Occluded))
                } else {
                    self.lifecycle
                        .transition(SurfacePhase::Deferred(FrameDeferReason::Timeout))
                        .map_err(|error| RendererError::Backend(error.to_string()))?;
                    Ok(FrameOutcome::Deferred(FrameDeferReason::Timeout))
                }
            } else {
                self.lifecycle
                    .transition(SurfacePhase::Presented)
                    .map_err(|error| RendererError::Backend(error.to_string()))?;
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
        assert_eq!(renderer.lifecycle.phase(), SurfacePhase::Presented);
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
        assert_eq!(renderer.attach_count, 1);
        assert_eq!(renderer.detach_count, 0);
        assert_eq!(renderer.recover_count, 1);
        assert!(renderer.updates[0].0);
        assert!(renderer.updates[1].0);
        assert!(!renderer.updates[1].1);
        assert!(renderer.updates[1].2 > renderer.updates[0].2);
        assert_eq!(renderer.lifecycle.phase(), SurfacePhase::Presented);
    }

    #[test]
    fn scale_change_relayouts_and_resizes_the_surface_in_physical_pixels() {
        let window = zui_core::WindowId(zui_core::Id::new(1));
        let logical_size = Size {
            width: Dip(500.0),
            height: Dip(300.0),
        };
        let renderer = Application::new()
            .run_with(
                HeadlessBackend::new().event(PlatformEvent::ScaleFactorChanged {
                    window,
                    size: logical_size,
                    scale_factor: zui_core::ScaleFactor(2.0),
                }),
                DeferredOnceRenderer::default(),
                Text::new("scaled"),
            )
            .expect("scale change should produce a valid resized frame");

        assert_eq!(renderer.resize_metrics.len(), 1);
        assert_eq!(renderer.resize_metrics[0].physical_size.width, 1_000);
        assert_eq!(renderer.resize_metrics[0].physical_size.height, 600);
        assert_eq!(
            renderer.resize_metrics[0].device_scale_factor,
            zui_core::ScaleFactor(2.0)
        );
    }

    struct AccessibilityProbe(Arc<Mutex<Vec<zui_platform::AccessibilityTree>>>);

    impl zui_platform::Accessibility for AccessibilityProbe {
        fn submit_tree(
            &mut self,
            tree: zui_platform::AccessibilityTree,
        ) -> Result<(), PlatformError> {
            self.0.lock().unwrap().push(tree);
            Ok(())
        }
    }

    #[test]
    fn application_submits_ui_semantics_through_the_typed_capability() {
        let submitted = Arc::new(Mutex::new(Vec::new()));
        let capabilities =
            Capabilities::new().with_accessibility(AccessibilityProbe(Arc::clone(&submitted)));
        Application::new()
            .run_with(
                HeadlessBackend::new().capabilities(capabilities),
                HeadlessRenderer::new(),
                Text::new("accessible text"),
            )
            .expect("accessibility submission must not disturb rendering");

        let trees = submitted.lock().unwrap();
        assert!(!trees.is_empty());
        assert_eq!(trees[0].root.role, zui_platform::AccessibilityRole::Text);
        assert_eq!(trees[0].root.label, "accessible text");
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

        fn surface_target(&self) -> zui_platform::SurfaceTarget {
            zui_platform::SurfaceTarget::headless()
        }

        fn request_redraw(&self) -> Result<(), zui_platform::PlatformError> {
            Ok(())
        }
    }

    struct OcclusionCycleBackend;

    struct VisibilityContext {
        host: Option<VisibilityHost>,
        capabilities: Capabilities,
        paths: Paths,
    }

    impl AppContext for VisibilityContext {
        fn host(&self, window: zui_core::WindowId) -> Option<&dyn Host> {
            self.host
                .as_ref()
                .filter(|host| host.id == window)
                .map(|host| host as &dyn Host)
        }
        fn window_ids(&self) -> Vec<zui_core::WindowId> {
            self.host
                .as_ref()
                .map(|host| vec![host.id])
                .unwrap_or_default()
        }
        fn create_window(
            &mut self,
            _options: WindowOptions,
        ) -> Result<zui_core::WindowId, PlatformError> {
            Err(PlatformError::Backend(
                "test context does not create another window".into(),
            ))
        }
        fn destroy_window(&mut self, window: zui_core::WindowId) -> Result<(), PlatformError> {
            if self.host.as_ref().is_some_and(|host| host.id == window) {
                self.host = None;
                Ok(())
            } else {
                Err(PlatformError::UnknownWindow(window))
            }
        }
        fn paths(&self) -> Result<&Paths, PlatformError> {
            Ok(&self.paths)
        }
        fn outputs(&self) -> &[zui_platform::Output] {
            &[]
        }
        fn capabilities(&mut self) -> &mut Capabilities {
            &mut self.capabilities
        }
        fn ui_task_poster(&self) -> UiTaskPoster {
            UiTaskPoster::new(|task| {
                task.run();
                Ok(())
            })
        }
    }

    impl Backend for OcclusionCycleBackend {
        fn run(
            self,
            options: WindowOptions,
            app: &mut dyn AppLoop,
        ) -> Result<(), zui_platform::PlatformError> {
            let host = VisibilityHost {
                id: zui_core::WindowId(zui_core::Id::new(77)),
                size: options.size,
            };
            let id = host.id;
            let mut context = VisibilityContext {
                host: Some(host),
                capabilities: Capabilities::new(),
                paths: Paths {
                    config_dir: "/test/config".into(),
                    data_dir: "/test/data".into(),
                    cache_dir: "/test/cache".into(),
                    runtime_dir: None,
                },
            };
            assert_eq!(
                app.event(&mut context, PlatformEvent::WindowCreated(id)),
                LoopControl::RequestRedraw(id)
            );

            assert_eq!(
                app.event(&mut context, PlatformEvent::RedrawRequested(id)),
                LoopControl::Continue
            );
            assert_eq!(
                app.event(
                    &mut context,
                    PlatformEvent::WindowOccluded {
                        window: id,
                        occluded: false,
                    },
                ),
                LoopControl::RequestRedraw(id)
            );
            let _ = app.event(&mut context, PlatformEvent::RedrawRequested(id));
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
        assert_eq!(renderer.lifecycle.phase(), SurfacePhase::Presented);
    }

    #[test]
    fn application_retains_native_text_rasterization_options() {
        let options = RendererOptions {
            text_rasterization: zui_render::TextRasterizationOptions {
                gamma: 0.7,
                contrast: 1.25,
            },
        };
        let application = Application::new().renderer_options(options);
        assert_eq!(application.renderer_options, options);
    }
}
