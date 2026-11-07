//! Event routing contracts; no runtime, backend or concrete control types.
use crate::widget::Invalidation;
use alloc::vec::Vec;
pub use zui_input::*;
use zui_types::{NodeId, Point, Rect, Size};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventPhase {
    Capture,
    Target,
    Bubble,
    Direct,
}
/// Committed layout of the receiver, in its local logical coordinate space.
/// This value is a snapshot, not access to the runtime's mutable layout tree.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EventGeometry {
    pub size: Size,
    pub content_bounds: Rect,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EventContext {
    /// Current modifier snapshot, including pointer events after focus changes.
    pub modifiers: Modifiers,
    pub id: NodeId,
    pub target: NodeId,
    pub phase: EventPhase,
    /// Pointer location relative to this receiver; may lie outside its bounds under capture.
    pub local_position: Option<Point>,
    /// Present for routed input, including keyboard input. Direct ownership and
    /// hover notifications carry None: they may occur while layout is invalid.
    pub geometry: Option<EventGeometry>,
}
/// Narrow, explicitly injected capabilities borrowed only for event dispatch.
pub struct EventServices<'a> {
    pub clipboard: &'a mut dyn crate::clipboard::Clipboard,
}
/// Snapshot offered before the default Tab traversal, from focused widget to root.
/// Child IDs identify the direct branch under the receiver, not a logical row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FocusNavigation {
    pub reverse: bool,
    pub focused: NodeId,
    pub focused_child: Option<NodeId>,
    pub candidate: Option<NodeId>,
    /// None when the candidate is the receiver itself or outside its subtree.
    pub candidate_child: Option<NodeId>,
}

#[derive(Debug, Default)]
pub struct FocusNavigationResponse {
    /// Suppress default traversal and stop the remaining navigation callbacks.
    pub handled: bool,
    pub invalidation: Invalidation,
    pub actions: Vec<crate::action::WidgetAction>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum WidgetEvent {
    /// Positional input coordinates have been transformed into the receiver's space.
    Input(InputEvent),
    FocusChanged(bool),
    PointerCaptureLost,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FocusRequest {
    #[default]
    None,
    /// Focus the receiver itself.
    Focus,
    /// Focus an existing enabled/focusable node in the same tree. Invalid targets
    /// are ignored. This does not alter the current event route.
    Target(NodeId),
    Clear,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CaptureRequest {
    #[default]
    None,
    Capture,
    Release,
}
#[derive(Debug, Default)]
pub struct EventResponse {
    pub actions: Vec<crate::action::WidgetAction>,
    /// Suppresses default actions such as Tab navigation; does not stop propagation.
    pub handled: bool,
    pub stop_propagation: bool,
    pub invalidation: Invalidation,
    /// First valid focus request wins; changes apply after event propagation.
    pub focus: FocusRequest,
    /// Pointer capture always refers to the receiver itself.
    pub capture: CaptureRequest,
}

/// Default Tab is about to enter this subtree from outside (or without focus).
/// Offered outermost first, excluding ancestors shared with the focused node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FocusEntry {
    pub reverse: bool,
    pub focused: Option<NodeId>,
    pub candidate: NodeId,
    /// Direct branch toward the candidate; None when candidate is the receiver.
    pub candidate_child: Option<NodeId>,
}
