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

/// Primary typed action exposed by a declarative control. Assembly adapters can
/// connect a handler without knowing the concrete widget or erasing its payload.
/// Controls may additionally emit diagnostic or secondary action payloads.
pub trait ActionView: Sized + 'static {
    type Action: 'static;
    fn set_action(&mut self, action: ActionId);
    /// Convert either a leaf spec or a composite control into a description.
    fn into_view(self) -> crate::view::View;
}

/// A controlled value and the primary action requesting its replacement.
/// This describes the control contract, not state ownership or subscriptions.
pub trait ValueView: ActionView {
    type Value: Clone + PartialEq + 'static;
    fn set_value(&mut self, value: Self::Value);
    /// None for secondary actions such as submission or rejection.
    fn requested_value(action: &Self::Action) -> Option<Self::Value>;
}
