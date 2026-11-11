//! Optional native OpenGL widget. GL renders into a private FBO, then ZUI
//! composites immutable pixels using its normal CPU/GPU painting pipeline.
//! See `docs/opengl-view.md` for threading, alpha and scheduling contracts.
mod native;
mod renderer;
#[cfg(test)]
mod tests;

pub use glow;
use std::{fmt, rc::Rc, time::Duration};
use zui_api::{
    event::*,
    layout::{AvailableSpace, LayoutError},
    view::ViewSpec,
    widget::*,
};
use zui_paint::{ImageLimits, ImageResource, ImageSampling, Painter, Path};
use zui_types::{Affine, Color, Extent, Size};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpenGlError {
    InvalidSize,
    TooLarge,
    Backend(String),
}
impl OpenGlError {
    fn backend(error: impl fmt::Display) -> Self {
        Self::Backend(error.to_string())
    }
}
impl fmt::Display for OpenGlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSize => f.write_str("invalid OpenGL size or scale factor"),
            Self::TooLarge => f.write_str("OpenGL target exceeds pixel resource limits"),
            Self::Backend(e) => f.write_str(e),
        }
    }
}
impl std::error::Error for OpenGlError {}

/// Values written by the application into the RGBA8 attachment are sRGB-encoded.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OpenGlAlphaMode {
    /// Ignore attachment alpha; composite a fully opaque image.
    #[default]
    Opaque,
    Straight,
    Premultiplied,
}
#[derive(Clone, Copy, Debug)]
pub struct OpenGlFrame {
    /// Framework-owned draw target, already bound. Do not delete its attachments.
    pub framebuffer: glow::Framebuffer,
    pub extent: Extent,
    pub logical_size: Size,
    pub scale_factor: f64,
    pub elapsed: Duration,
}

/// Each mounted widget owns one renderer and one private GL context.
/// GL callbacks execute on the widget/UI thread with that context current.
/// Do not switch/destroy the context or delete the framework FBO. Configure
/// your own program, VAO, blend/depth/cull state in `render`. Input is CPU-only;
/// queue changes there, then apply them to GL during the next render callback.
pub trait OpenGlRenderer: 'static {
    fn initialize(&mut self, _gl: &glow::Context) -> Result<(), OpenGlError> {
        Ok(())
    }
    fn resize(
        &mut self,
        _gl: &glow::Context,
        _extent: Extent,
        _scale_factor: f64,
    ) -> Result<(), OpenGlError> {
        Ok(())
    }
    fn render(&mut self, gl: &glow::Context, frame: OpenGlFrame) -> Result<(), OpenGlError>;
    fn input(&mut self, _context: EventContext, _event: &WidgetEvent) -> EventResponse {
        EventResponse::default()
    }
    /// Also called after partially failed initialization. Must not panic.
    fn destroy(&mut self, _gl: &glow::Context) {}
}

type Factory = dyn Fn() -> Box<dyn OpenGlRenderer>;
type ErrorHandler = dyn Fn(&OpenGlError);
#[derive(Clone)]
pub struct OpenGlView {
    pub natural_size: Size,
    /// Supply WindowMetrics.scale on initial creation and ScaleChanged.
    pub scale_factor: f64,
    pub alpha_mode: OpenGlAlphaMode,
    /// None: render only after layout/scale/revision/input changes.
    pub frame_interval: Option<Duration>,
    /// Set false for a hidden tab/offscreen panel to suspend animation and input.
    pub visible: bool,
    pub focusable: bool,
    pub fallback_color: Color,
    /// Change to refresh external model data or retry after an error.
    pub revision: u64,
    factory: Rc<Factory>,
    on_error: Rc<ErrorHandler>,
}
impl OpenGlView {
    /// Keep this view in the model and clone it during rebuilds so that the
    /// factory identity (and native resources) remain stable.
    pub fn new<R: OpenGlRenderer>(natural_size: Size, factory: impl Fn() -> R + 'static) -> Self {
        Self {
            natural_size,
            scale_factor: 1.0,
            alpha_mode: OpenGlAlphaMode::Opaque,
            frame_interval: None,
            visible: true,
            focusable: true,
            fallback_color: Color::rgb(32, 36, 44),
            revision: 0,
            factory: Rc::new(move || Box::new(factory())),
            on_error: Rc::new(|error| eprintln!("OpenGlView: {error}")),
        }
    }
    pub fn on_error(mut self, callback: impl Fn(&OpenGlError) + 'static) -> Self {
        self.on_error = Rc::new(callback);
        self
    }
    /// Rebuild the application view after calling this. This does not wake a
    /// sleeping native event loop by itself.
    pub fn request_redraw(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }
}

pub struct OpenGlWidget {
    properties: OpenGlView,
    session: Option<renderer::Session>,
    image: Option<ImageResource>,
    size: Size,
    dirty: bool,
    failed: bool,
    next_frame: Option<Duration>,
}
impl ViewSpec for OpenGlView {
    type Widget = OpenGlWidget;
    fn build(&self) -> OpenGlWidget {
        OpenGlWidget {
            properties: self.clone(),
            session: None,
            image: None,
            size: Size::ZERO,
            dirty: true,
            failed: false,
            next_frame: None,
        }
    }
    fn update(&self, widget: &mut OpenGlWidget) -> Invalidation {
        let old = &widget.properties;
        let replace = !Rc::ptr_eq(&self.factory, &old.factory);
        let layout = self.natural_size != old.natural_size;
        let redraw = replace
            || layout
            || self.revision != old.revision
            || self.scale_factor != old.scale_factor
            || self.alpha_mode != old.alpha_mode
            || self.visible != old.visible
            || self.frame_interval != old.frame_interval;
        let paint =
            redraw || self.fallback_color != old.fallback_color || self.focusable != old.focusable;
        if replace {
            widget.session = None;
            widget.image = None;
        }
        if redraw {
            widget.dirty = true;
            widget.failed = false;
            widget.next_frame = None;
        }
        widget.properties = self.clone();
        if layout {
            Invalidation::Layout
        } else if paint {
            Invalidation::Paint
        } else {
            Invalidation::None
        }
    }
}
impl OpenGlWidget {
    fn fail(&mut self, error: OpenGlError) {
        self.session = None;
        self.image = None;
        self.failed = true;
        self.dirty = false;
        self.next_frame = None;
        (self.properties.on_error)(&error);
    }
}
impl Widget for OpenGlWidget {
    fn enabled(&self) -> bool {
        self.properties.visible
    }
    fn focusable(&self) -> bool {
        self.properties.visible && self.properties.focusable
    }
    fn measure(&mut self, c: MeasureContext) -> Result<Size, LayoutError> {
        if !self.properties.natural_size.is_valid() {
            return Err(LayoutError::InvalidStyle(c.id));
        }
        let fit = |value: f64, available| match available {
            AvailableSpace::Definite(v) => value.min(f64::from(v)),
            AvailableSpace::MinContent => 0.0,
            AvailableSpace::MaxContent => value,
        };
        Ok(Size::new(
            c.input.known_width.map(f64::from).unwrap_or_else(|| {
                fit(self.properties.natural_size.width, c.input.available_width)
            }),
            c.input.known_height.map(f64::from).unwrap_or_else(|| {
                fit(
                    self.properties.natural_size.height,
                    c.input.available_height,
                )
            }),
        ))
    }
    fn prepare(&mut self, c: PrepareContext) -> Result<(), LayoutError> {
        if !self.properties.natural_size.is_valid() {
            return Err(LayoutError::InvalidStyle(c.id));
        }
        if self.size != c.content_size {
            self.size = c.content_size;
            self.dirty = true;
            self.failed = false;
        }
        Ok(())
    }
    fn animate(&mut self, elapsed: Duration) -> AnimationUpdate {
        if !self.properties.visible
            || self.size.width <= 0.0
            || self.size.height <= 0.0
            || self.failed
        {
            self.next_frame = None;
            return AnimationUpdate::default();
        }
        let interval = self.properties.frame_interval.filter(|d| !d.is_zero());
        let due = self.next_frame.is_some_and(|deadline| elapsed >= deadline);
        if !self.dirty && !due {
            return AnimationUpdate {
                next_frame: self.next_frame,
                ..Default::default()
            };
        }
        let result = (|| {
            let extent = pixel_extent(self.size, self.properties.scale_factor)?;
            if self.session.is_none() {
                self.session = Some(renderer::Session::new((self.properties.factory)())?);
            }
            self.session.as_mut().unwrap().render(
                extent,
                self.size,
                self.properties.scale_factor,
                elapsed,
                self.properties.alpha_mode,
            )
        })();
        match result {
            Ok(image) => {
                self.image = Some(image);
                self.dirty = false;
                self.next_frame = interval.and_then(|d| elapsed.checked_add(d));
            }
            Err(error) => self.fail(error),
        }
        AnimationUpdate {
            invalidation: Invalidation::Paint,
            next_frame: self.next_frame,
        }
    }
    fn paint(&self, c: PaintContext, p: &mut Painter<'_>) {
        if !self.properties.visible
            || c.content_bounds.size.width <= 0.0
            || c.content_bounds.size.height <= 0.0
        {
            return;
        }
        p.with_clip(Path::rectangle(c.content_bounds), |p| {
            if let Some(image) = &self.image {
                let e = image.extent();
                p.with_transform(
                    Affine::translate(c.content_bounds.origin.x, c.content_bounds.origin.y)
                        .compose(Affine::scale(
                            c.content_bounds.size.width / f64::from(e.width),
                            c.content_bounds.size.height / f64::from(e.height),
                        )),
                    |p| p.image(image.clone(), ImageSampling::Bilinear),
                );
            } else {
                p.fill(
                    Path::rectangle(c.content_bounds),
                    self.properties.fallback_color,
                );
            }
        });
    }
    fn event(&mut self, c: EventContext, event: &WidgetEvent) -> EventResponse {
        if !self.properties.visible || !matches!(c.phase, EventPhase::Target | EventPhase::Direct) {
            return EventResponse::default();
        }
        let mut response = self
            .session
            .as_mut()
            .map_or_else(EventResponse::default, |s| s.input(c, event));
        if response.invalidation != Invalidation::None {
            self.dirty = true;
        }
        if let WidgetEvent::Input(InputEvent::PointerButton {
            button: PointerButton::Primary,
            pressed: true,
            ..
        }) = event
            && self.properties.focusable
            && response.focus == FocusRequest::None
        {
            response.focus = FocusRequest::Focus;
        }
        response
    }
    fn lifecycle(&mut self, _: zui_types::NodeId, event: Lifecycle) {
        if event == Lifecycle::Unmounted {
            self.session = None;
            self.image = None;
            self.next_frame = None;
        }
    }
}

fn pixel_extent(size: Size, scale: f64) -> Result<Extent, OpenGlError> {
    if !size.is_valid()
        || size.width <= 0.0
        || size.height <= 0.0
        || !scale.is_finite()
        || scale <= 0.0
    {
        return Err(OpenGlError::InvalidSize);
    }
    let width = (size.width * scale).ceil();
    let height = (size.height * scale).ceil();
    let limits = ImageLimits::default();
    if !width.is_finite()
        || !height.is_finite()
        || width > f64::from(limits.max_dimension)
        || height > f64::from(limits.max_dimension)
    {
        return Err(OpenGlError::TooLarge);
    }
    let extent = Extent::new(width.max(1.0) as u32, height.max(1.0) as u32);
    if extent.rgba_len().is_none_or(|len| len > limits.max_bytes) {
        return Err(OpenGlError::TooLarge);
    }
    Ok(extent)
}
