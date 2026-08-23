//! Stable, platform-neutral contracts consumed by applications and UI code.

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
    fn request_redraw(&self) -> Result<(), PlatformError>;
}

pub trait Backend {
    type Host: Host;
    fn create_window(&mut self, options: WindowOptions) -> Result<Self::Host, PlatformError>;
    fn run(self, handler: &mut dyn FnMut(PlatformEvent)) -> Result<(), PlatformError>;
}

/// Backend-only hooks. Applications must not depend on this module.
pub mod spi {
    use raw_window_handle::{RawDisplayHandle, RawWindowHandle};

    pub trait RawWindowHandleProvider {
        fn raw_window_handle(&self) -> Result<RawWindowHandle, raw_window_handle::HandleError>;
        fn raw_display_handle(&self) -> Result<RawDisplayHandle, raw_window_handle::HandleError>;
    }
}
