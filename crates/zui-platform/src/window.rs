use crate::{PlatformError, WindowEvent};
use alloc::{string::String, sync::Arc};
use core::time::Duration;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use zui_paint::PixelBuffer;
use zui_types::{Extent, Point, Rect, Size};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowMetrics {
    pub extent: Extent,
    pub scale: f64,
    pub occluded: bool,
}

impl WindowMetrics {
    pub fn logical_size(self) -> Size {
        Size::new(
            self.extent.width as f64 / self.scale,
            self.extent.height as f64 / self.scale,
        )
    }
    pub fn logical_point(self, physical: Point) -> Point {
        Point::new(physical.x / self.scale, physical.y / self.scale)
    }
    pub fn drawable(self) -> bool {
        !self.occluded
            && self.extent.width != 0
            && self.extent.height != 0
            && self.scale.is_finite()
            && self.scale > 0.0
    }
}

#[derive(Clone, Debug)]
pub struct WindowConfig {
    pub title: String,
    pub size: Size,
    pub resizable: bool,
    /// Native title bar and frame. False permits borderless or application-drawn chrome.
    pub decorations: bool,
    /// Requests to the desktop backend; platforms may disable rather than hide buttons.
    pub close_button: bool,
    pub minimize_button: bool,
    pub maximize_button: bool,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            title: String::from("ZUI"),
            size: Size::new(800.0, 480.0),
            resizable: true,
            decorations: true,
            close_button: true,
            minimize_button: true,
            maximize_button: true,
        }
    }
}

/// Desktop native-handle interoperability is isolated in this platform contract.
/// Future bare-metal display drivers do not need to implement NativeWindow.
pub trait NativeWindow: HasWindowHandle + HasDisplayHandle + Send + Sync {
    fn metrics(&self) -> WindowMetrics;
    fn request_redraw(&self);
    fn request_size(&self, size: Size);
    fn set_title(&self, title: &str);
    /// Begin a native window move from a primary pointer press in custom chrome.
    fn drag_window(&self) -> Result<(), PlatformError> {
        Err(PlatformError::Unsupported("window dragging"))
    }
    fn set_minimized(&self, _minimized: bool) -> Result<(), PlatformError> {
        Err(PlatformError::Unsupported("window minimizing"))
    }
    fn set_maximized(&self, _maximized: bool) -> Result<(), PlatformError> {
        Err(PlatformError::Unsupported("window maximizing"))
    }
    fn set_ime_enabled(&self, enabled: bool);
    fn set_ime_cursor_area(&self, rect: Rect);
    fn pre_present_notify(&self);
}

/// Opaque application session identity; the platform does not interpret owners.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImeSessionId {
    pub owner: u64,
    pub generation: u64,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImeRequest {
    pub session: ImeSessionId,
    /// Window-relative logical units; the desktop adapter applies DPI conversion.
    pub cursor_area: Rect,
}

/// Copy an already rendered pixel buffer to a software display surface.
pub trait PixelPresenter {
    fn present(&mut self, pixels: &PixelBuffer) -> Result<(), PlatformError>;
}

/// Responses are requests, not knowledge of a particular desktop event loop.
#[derive(Clone, Copy, Debug, Default)]
pub struct WindowAction {
    pub redraw: bool,
    pub exit: bool,
    /// Absolute monotonic time since the event loop started.
    /// Requests coalesce to the earliest outstanding deadline.
    pub wake_at: Option<Duration>,
}

impl WindowAction {
    pub fn redraw() -> Self {
        Self {
            redraw: true,
            ..Self::default()
        }
    }
    pub fn exit() -> Self {
        Self {
            exit: true,
            ..Self::default()
        }
    }
}

/// Implemented by the upper assembly layer. The platform knows only this trait.
pub trait WindowHandler {
    fn connected(&mut self, window: Arc<dyn NativeWindow>) -> Result<(), PlatformError>;
    fn event(
        &mut self,
        event: WindowEvent,
        metrics: WindowMetrics,
        elapsed: Duration,
    ) -> Result<WindowAction, PlatformError>;
    /// Release surface resources before the native window is destroyed/suspended.
    fn disconnected(&mut self) {}
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RunReport {
    pub events: u64,
    pub redraws: u64,
}
