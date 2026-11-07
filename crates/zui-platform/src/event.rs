pub use zui_input::*;

#[derive(Clone, Debug, PartialEq)]
pub enum WindowEvent {
    Resumed,
    Resized,
    ScaleChanged,
    Focused(bool),
    Occluded(bool),
    Input(InputEvent),
    RedrawRequested,
    /// An application-requested timer or external wakeup fired.
    Wake,
    CloseRequested,
}
