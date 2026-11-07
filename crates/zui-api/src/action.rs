//! Opaque control actions interpreted only by the application assembly layer.
use alloc::boxed::Box;
use core::{any::Any, fmt};
use zui_types::NodeId;

/// Chosen by the application while constructing a control description.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ActionId(pub u64);

pub struct WidgetAction {
    pub id: ActionId,
    payload: Box<dyn Any>,
}
impl WidgetAction {
    pub fn new(id: ActionId, payload: impl Any) -> Self {
        Self {
            id,
            payload: Box::new(payload),
        }
    }
    /// The caller knows the control's public payload contract. The core only
    /// transports this value and never recognizes application or control types.
    pub fn payload<T: Any>(&self) -> Option<&T> {
        self.payload.downcast_ref()
    }
}
impl fmt::Debug for WidgetAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WidgetAction")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}
#[derive(Debug)]
pub struct ActionEvent {
    pub source: NodeId,
    pub action: WidgetAction,
}
