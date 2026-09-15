//! Stable, platform-neutral contracts consumed by applications and UI code.

use std::{sync::Arc, time::Instant};

use raw_window_handle::{HasDisplayHandle, HasWindowHandle};

pub use zui_core::{Dip, Id, OutputId, PhysicalSize, Point, Rect, ScaleFactor, Size, WindowId};

#[derive(Clone, Debug, PartialEq)]
pub struct WindowOptions {
    pub title: String,
    pub size: Size,
    pub resizable: bool,
}

impl Default for WindowOptions {
    fn default() -> Self {
        Self {
            title: "zero-ui".into(),
            size: Size {
                width: Dip(800.0),
                height: Dip(600.0),
            },
            resizable: true,
        }
    }
}

/// Platform-neutral category of a renderable host target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceTargetKind {
    Native,
    Headless,
}

/// Native handle source retained by a [`SurfaceTarget`].
///
/// The platform backend owns the concrete implementation (for example a
/// winit window). Renderers only see the standard raw-window-handle traits and
/// never depend on that concrete backend type.
pub trait NativeSurfaceSource: HasWindowHandle + HasDisplayHandle + Send + Sync {}

impl<T> NativeSurfaceSource for T where T: HasWindowHandle + HasDisplayHandle + Send + Sync {}

#[derive(Clone)]
enum SurfaceTargetInner {
    Native(Arc<dyn NativeSurfaceSource>),
    Headless,
}

/// Opaque, cloneable attachment target produced by a platform backend.
///
/// A native target owns a strong reference to its native handle source, so a
/// renderer may safely retain the target for the complete surface lifetime and
/// reuse it when recovering a lost surface. Construction is intentionally
/// restricted to [`spi`]; application and widget code can only pass targets
/// received from a [`Host`] to a renderer.
#[derive(Clone)]
pub struct SurfaceTarget {
    inner: SurfaceTargetInner,
}

impl SurfaceTarget {
    /// Creates a target with no native window handles for deterministic
    /// rendering and application tests.
    pub fn headless() -> Self {
        Self {
            inner: SurfaceTargetInner::Headless,
        }
    }

    pub fn kind(&self) -> SurfaceTargetKind {
        match &self.inner {
            SurfaceTargetInner::Native(_) => SurfaceTargetKind::Native,
            SurfaceTargetInner::Headless => SurfaceTargetKind::Headless,
        }
    }

    /// Returns an owned reference to the platform-neutral native handle
    /// source. This is the only native detail exposed to render integrations.
    pub fn native_source(&self) -> Option<Arc<dyn NativeSurfaceSource>> {
        match &self.inner {
            SurfaceTargetInner::Native(source) => Some(Arc::clone(source)),
            SurfaceTargetInner::Headless => None,
        }
    }
}

impl std::fmt::Debug for SurfaceTarget {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SurfaceTarget")
            .field("kind", &self.kind())
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Other(u16),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyState {
    Pressed,
    Released,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyCode {
    Escape,
    Enter,
    Tab,
    Backspace,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Home,
    End,
    Character(char),
    Unknown,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub shift: bool,
    pub control: bool,
    pub alt: bool,
    pub logo: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum InputEvent {
    CursorMoved {
        position: Point,
    },
    MouseInput {
        button: MouseButton,
        state: KeyState,
    },
    MouseWheel {
        delta_x: Dip,
        delta_y: Dip,
    },
    Keyboard {
        key: KeyCode,
        state: KeyState,
        modifiers: Modifiers,
    },
    Text(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum PlatformEvent {
    WindowCreated(WindowId),
    WindowResized {
        window: WindowId,
        size: Size,
        scale_factor: ScaleFactor,
    },
    Input {
        window: WindowId,
        event: InputEvent,
    },
    RedrawRequested(WindowId),
    WindowOccluded {
        window: WindowId,
        occluded: bool,
    },
    CloseRequested(WindowId),
    AboutToWait,
}

#[derive(Debug)]
pub enum PlatformError {
    Backend(String),
    NoWindow,
}

impl std::fmt::Display for PlatformError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Backend(message) => write!(f, "backend error: {message}"),
            Self::NoWindow => write!(f, "no window available"),
        }
    }
}

impl std::error::Error for PlatformError {}

pub trait Host {
    fn id(&self) -> WindowId;
    fn size(&self) -> Size;
    fn scale_factor(&self) -> ScaleFactor;
    fn surface_target(&self) -> SurfaceTarget;
    fn request_redraw(&self) -> Result<(), PlatformError>;
}

/// Scheduling decision returned by the application loop after a platform
/// callback. Backends own the native event loop and translate this value into
/// their own control-flow mechanism.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LoopControl {
    #[default]
    Continue,
    RequestRedraw,
    WaitUntil(Instant),
    Exit,
}

impl LoopControl {
    pub fn from_redraw_deadline(deadline: Option<Instant>) -> Self {
        match deadline {
            Some(deadline) if deadline <= Instant::now() => Self::RequestRedraw,
            Some(deadline) => Self::WaitUntil(deadline),
            None => Self::Continue,
        }
    }
}

/// Platform-neutral application callbacks driven by a [`Backend`].
///
/// A native host can only be created once its platform event loop is active.
/// Every backend first emits [`PlatformEvent::WindowCreated`], then lends the
/// host through `host_ready` for surface attachment before the first redraw.
/// Returning [`LoopControl::Exit`] from the creation event skips `host_ready`.
/// Headless implementations follow the same ordering as native backends.
pub trait AppLoop<H: Host> {
    fn host_ready(&mut self, host: &H) -> LoopControl;
    /// Handles an event while borrowing the live host. Keeping the host in
    /// this callback lets renderers recreate a lost native surface without
    /// leaking raw-window details into application code.
    fn event(&mut self, host: &H, event: PlatformEvent) -> LoopControl;
}

pub trait Backend {
    type Host: Host;

    /// Creates the host inside the backend's native lifecycle and drives the
    /// supplied application loop until it exits.
    fn run(
        self,
        options: WindowOptions,
        app: &mut dyn AppLoop<Self::Host>,
    ) -> Result<(), PlatformError>;
}

/// Backend-only hooks. Applications must not depend on this module.
pub mod spi {
    use std::sync::Arc;

    use raw_window_handle::{HasDisplayHandle, HasWindowHandle};

    use super::{SurfaceTarget, SurfaceTargetInner};

    /// Creates a native target while retaining ownership of its handle source.
    /// Only backend implementations should call this function.
    pub fn native_surface_target<T>(source: Arc<T>) -> SurfaceTarget
    where
        T: HasWindowHandle + HasDisplayHandle + Send + Sync + 'static,
    {
        SurfaceTarget {
            inner: SurfaceTargetInner::Native(source),
        }
    }

    /// Creates an in-memory target for a backend with no native window.
    pub fn headless_surface_target() -> SurfaceTarget {
        SurfaceTarget::headless()
    }
}
