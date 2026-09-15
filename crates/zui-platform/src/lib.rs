//! Platform-neutral contracts consumed by applications and UI code.
//!
//! The crate is split by API maturity rather than by operating system:
//! [`core`] is the versioned application-window contract, [`capability`]
//! contains optional services, and [`experimental`] contains contracts that
//! are intentionally allowed to evolve before promotion.

use std::{path::PathBuf, sync::Arc, time::Instant};

use raw_window_handle::{HasDisplayHandle, HasWindowHandle};

pub use zui_core::{Dip, Id, OutputId, PhysicalSize, Point, Rect, ScaleFactor, Size, WindowId};

/// APIs in this module form the `core v0` platform contract.
pub mod core {
    pub use super::{
        AppContext, AppLoop, Backend, Host, InputEvent, KeyCode, KeyState, LoopControl, Modifiers,
        MouseButton, Output, Paths, PlatformError, PlatformEvent, SurfaceTarget, SurfaceTargetKind,
        UiTaskPoster, WindowOptions, CORE_API_VERSION,
    };
}

/// Optional, typed services. Absence is reported by `Option`, never by panic.
pub mod capability {
    pub use super::{
        Capabilities, Clipboard, Dialog, DialogRequest, DialogRequestId, DialogResponse, DragDrop,
        DragDropEvent,
    };
}

/// Contracts that may change between minor releases until promoted.
pub mod experimental {
    pub use super::{
        Accessibility, AccessibilityNode, AccessibilityNodeId, AccessibilityRole,
        AccessibilityTree, Ime, ImeEvent,
    };
}

/// Version marker for the deliberately small stable platform core.
pub const CORE_API_VERSION: u32 = 0;

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Paths {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
    /// Some platforms do not define a per-user runtime directory.
    pub runtime_dir: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Output {
    pub id: OutputId,
    pub name: Option<String>,
    pub logical_bounds: Rect,
    pub scale_factor: ScaleFactor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceTargetKind {
    Native,
    Headless,
}

pub trait NativeSurfaceSource: HasWindowHandle + HasDisplayHandle + Send + Sync {}

impl<T> NativeSurfaceSource for T where T: HasWindowHandle + HasDisplayHandle + Send + Sync {}

#[derive(Clone)]
enum SurfaceTargetInner {
    Native(Arc<dyn NativeSurfaceSource>),
    Headless,
}

/// Opaque attachment target produced by a platform backend.
#[derive(Clone)]
pub struct SurfaceTarget {
    inner: SurfaceTargetInner,
}

impl SurfaceTarget {
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImeEvent {
    Enabled,
    Disabled,
    Preedit {
        text: String,
        /// UTF-8 byte range selected by the input method, if supplied.
        selection: Option<(usize, usize)>,
    },
    Commit(String),
    Cancelled,
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
    /// Compatibility path for direct text injection. Native IMEs use `Ime`.
    Text(String),
    Ime(ImeEvent),
}

#[derive(Clone, Debug, PartialEq)]
pub enum DragDropEvent {
    Entered {
        window: WindowId,
        paths: Vec<PathBuf>,
    },
    Moved {
        window: WindowId,
        position: Point,
    },
    Dropped {
        window: WindowId,
        paths: Vec<PathBuf>,
    },
    Cancelled {
        window: WindowId,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DialogRequestId(pub Id);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DialogRequest {
    OpenFile {
        title: String,
        multiple: bool,
    },
    SaveFile {
        title: String,
        suggested_name: Option<String>,
    },
    Message {
        title: String,
        message: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DialogResponse {
    pub request: DialogRequestId,
    pub paths: Vec<PathBuf>,
    pub accepted: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PlatformEvent {
    WindowCreated(WindowId),
    WindowDestroyed(WindowId),
    WindowResized {
        window: WindowId,
        size: Size,
    },
    ScaleFactorChanged {
        window: WindowId,
        size: Size,
        scale_factor: ScaleFactor,
    },
    OutputsChanged(Vec<Output>),
    Input {
        window: WindowId,
        event: InputEvent,
    },
    DragDrop(DragDropEvent),
    DialogCompleted(DialogResponse),
    RedrawRequested(WindowId),
    WindowOccluded {
        window: WindowId,
        occluded: bool,
    },
    CloseRequested(WindowId),
    AboutToWait,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlatformError {
    Backend(String),
    UnknownWindow(WindowId),
    PathUnavailable {
        variable: &'static str,
        reason: String,
    },
    Capability {
        name: &'static str,
        reason: String,
    },
    EventLoopClosed,
}

impl std::fmt::Display for PlatformError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Backend(message) => write!(f, "backend error: {message}"),
            Self::UnknownWindow(window) => write!(f, "unknown window {}", window.0),
            Self::PathUnavailable { variable, reason } => {
                write!(f, "platform path unavailable ({variable}): {reason}")
            }
            Self::Capability { name, reason } => write!(f, "{name} capability error: {reason}"),
            Self::EventLoopClosed => f.write_str("platform event loop is closed"),
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

pub trait Clipboard {
    fn read_text(&mut self) -> Result<Option<String>, PlatformError>;
    fn write_text(&mut self, text: &str) -> Result<(), PlatformError>;
}

pub trait Dialog {
    /// Starts a non-blocking request. Completion arrives as `DialogCompleted`.
    fn show(&mut self, request: DialogRequest) -> Result<DialogRequestId, PlatformError>;
}

pub trait DragDrop {
    fn set_enabled(&mut self, window: WindowId, enabled: bool) -> Result<(), PlatformError>;
}

pub trait Ime {
    fn set_enabled(&mut self, window: WindowId, enabled: bool) -> Result<(), PlatformError>;
    fn set_cursor_area(&mut self, window: WindowId, area: Rect) -> Result<(), PlatformError>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AccessibilityNodeId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccessibilityRole {
    Generic,
    Text,
    Button,
    TextInput,
    Group,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AccessibilityNode {
    pub id: AccessibilityNodeId,
    pub role: AccessibilityRole,
    pub label: String,
    pub enabled: bool,
    pub bounds: Rect,
    pub children: Vec<AccessibilityNode>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AccessibilityTree {
    pub window: WindowId,
    pub root: AccessibilityNode,
}

pub trait Accessibility {
    fn submit_tree(&mut self, tree: AccessibilityTree) -> Result<(), PlatformError>;
}

/// Type-safe storage used by backends and tests to inject optional services.
#[derive(Default)]
pub struct Capabilities {
    clipboard: Option<Box<dyn Clipboard>>,
    dialog: Option<Box<dyn Dialog>>,
    drag_drop: Option<Box<dyn DragDrop>>,
    ime: Option<Box<dyn Ime>>,
    accessibility: Option<Box<dyn Accessibility>>,
}

impl Capabilities {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_clipboard(mut self, value: impl Clipboard + 'static) -> Self {
        self.clipboard = Some(Box::new(value));
        self
    }
    pub fn with_dialog(mut self, value: impl Dialog + 'static) -> Self {
        self.dialog = Some(Box::new(value));
        self
    }
    pub fn with_drag_drop(mut self, value: impl DragDrop + 'static) -> Self {
        self.drag_drop = Some(Box::new(value));
        self
    }
    pub fn with_ime(mut self, value: impl Ime + 'static) -> Self {
        self.ime = Some(Box::new(value));
        self
    }
    pub fn with_accessibility(mut self, value: impl Accessibility + 'static) -> Self {
        self.accessibility = Some(Box::new(value));
        self
    }
    pub fn clipboard(&mut self) -> Option<&mut (dyn Clipboard + 'static)> {
        self.clipboard.as_deref_mut()
    }
    pub fn dialog(&mut self) -> Option<&mut (dyn Dialog + 'static)> {
        self.dialog.as_deref_mut()
    }
    pub fn drag_drop(&mut self) -> Option<&mut (dyn DragDrop + 'static)> {
        self.drag_drop.as_deref_mut()
    }
    pub fn ime(&mut self) -> Option<&mut (dyn Ime + 'static)> {
        self.ime.as_deref_mut()
    }
    pub fn accessibility(&mut self) -> Option<&mut (dyn Accessibility + 'static)> {
        self.accessibility.as_deref_mut()
    }
}

/// Opaque task transported by backend event-loop implementations.
#[doc(hidden)]
pub struct UiTask(Box<dyn FnOnce() + Send + 'static>);

impl UiTask {
    pub fn run(self) {
        (self.0)()
    }
}

type PostTask = dyn Fn(UiTask) -> Result<(), PlatformError> + Send + Sync;

/// Cloneable handle that background threads may use to enqueue UI-thread work.
#[derive(Clone)]
pub struct UiTaskPoster {
    post: Arc<PostTask>,
}

impl UiTaskPoster {
    pub fn new(post: impl Fn(UiTask) -> Result<(), PlatformError> + Send + Sync + 'static) -> Self {
        Self {
            post: Arc::new(post),
        }
    }

    pub fn post(&self, task: impl FnOnce() + Send + 'static) -> Result<(), PlatformError> {
        (self.post)(UiTask(Box::new(task)))
    }
}

impl std::fmt::Debug for UiTaskPoster {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UiTaskPoster").finish_non_exhaustive()
    }
}

/// Controlled view of live hosts and application-scoped platform services.
pub trait AppContext {
    fn host(&self, window: WindowId) -> Option<&dyn Host>;
    fn window_ids(&self) -> Vec<WindowId>;
    fn create_window(&mut self, options: WindowOptions) -> Result<WindowId, PlatformError>;
    fn destroy_window(&mut self, window: WindowId) -> Result<(), PlatformError>;
    fn paths(&self) -> Result<&Paths, PlatformError>;
    fn outputs(&self) -> &[Output];
    fn capabilities(&mut self) -> &mut Capabilities;
    fn ui_task_poster(&self) -> UiTaskPoster;
}

/// Scheduling decision returned after each application callback.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LoopControl {
    #[default]
    Continue,
    RequestRedraw(WindowId),
    WaitUntil {
        window: WindowId,
        deadline: Instant,
    },
    Exit,
}

impl LoopControl {
    pub fn from_redraw_deadline(window: WindowId, deadline: Option<Instant>) -> Self {
        match deadline {
            Some(deadline) if deadline <= Instant::now() => Self::RequestRedraw(window),
            Some(deadline) => Self::WaitUntil { window, deadline },
            None => Self::Continue,
        }
    }
}

/// Platform-neutral callback interface. A `WindowCreated` event is delivered
/// only after its host is queryable through `AppContext::host`.
pub trait AppLoop {
    fn event(&mut self, context: &mut dyn AppContext, event: PlatformEvent) -> LoopControl;
}

pub trait Backend {
    fn run(self, initial_window: WindowOptions, app: &mut dyn AppLoop)
        -> Result<(), PlatformError>;
}

/// Backend-only hooks. Applications must not depend on this module.
pub mod spi {
    use super::{SurfaceTarget, SurfaceTargetInner};
    use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
    use std::sync::Arc;

    pub fn native_surface_target<T>(source: Arc<T>) -> SurfaceTarget
    where
        T: HasWindowHandle + HasDisplayHandle + Send + Sync + 'static,
    {
        SurfaceTarget {
            inner: SurfaceTargetInner::Native(source),
        }
    }

    pub fn headless_surface_target() -> SurfaceTarget {
        SurfaceTarget::headless()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_capabilities_are_explicit() {
        let mut capabilities = Capabilities::new();
        assert!(capabilities.clipboard().is_none());
        assert!(capabilities.dialog().is_none());
        assert!(capabilities.drag_drop().is_none());
        assert!(capabilities.ime().is_none());
        assert!(capabilities.accessibility().is_none());
    }

    struct MemoryClipboard(String);
    impl Clipboard for MemoryClipboard {
        fn read_text(&mut self) -> Result<Option<String>, PlatformError> {
            Ok(Some(self.0.clone()))
        }
        fn write_text(&mut self, text: &str) -> Result<(), PlatformError> {
            self.0 = text.into();
            Ok(())
        }
    }

    #[test]
    fn capabilities_are_injected_and_queried_by_trait_type() {
        let mut capabilities = Capabilities::new().with_clipboard(MemoryClipboard(String::new()));
        let clipboard = capabilities.clipboard().expect("clipboard was injected");
        clipboard.write_text("typed capability").unwrap();
        assert_eq!(
            clipboard.read_text().unwrap().as_deref(),
            Some("typed capability")
        );
    }

    #[test]
    fn redraw_deadlines_retain_the_target_window() {
        let window = WindowId(Id::new(7));
        assert_eq!(
            LoopControl::from_redraw_deadline(window, Some(Instant::now())),
            LoopControl::RequestRedraw(window)
        );
    }
}
