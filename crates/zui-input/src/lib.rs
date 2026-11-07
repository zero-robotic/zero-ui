//! Portable input values shared by platforms and UI contracts.
#![no_std]
extern crate alloc;
use alloc::string::String;
use zui_types::Point;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub shift: bool,
    pub control: bool,
    pub alt: bool,
    pub super_key: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Key {
    Character(String),
    Enter,
    Escape,
    Tab,
    Backspace,
    Delete,
    Space,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    ArrowDown,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    Function(u8),
    Unidentified,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyInput {
    pub key: Key,
    pub pressed: bool,
    pub repeat: bool,
    /// Text produced by a keyboard event, separate from IME preedit/commit.
    pub text: Option<String>,
    pub modifiers: Modifiers,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerButton {
    Primary,
    Secondary,
    Middle,
    Back,
    Forward,
    Other(u16),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScrollDelta {
    /// Device-independent line units. The control chooses its line height.
    Lines { x: f64, y: f64 },
    /// Logical pixels; the backend already removed the window scale factor.
    Pixels { x: f64, y: f64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TouchPhase {
    Started,
    Moved,
    Ended,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq)]
pub enum InputEvent {
    PointerEntered,
    PointerLeft,
    PointerMoved(Point),
    PointerButton {
        button: PointerButton,
        pressed: bool,
        position: Option<Point>,
    },
    Scroll(ScrollDelta),
    Touch {
        id: u64,
        phase: TouchPhase,
        position: Point,
    },
    Key(KeyInput),
    ModifiersChanged(Modifiers),
    ImeEnabled,
    /// Cursor indices are UTF-8 byte offsets into `text`, as supplied by the OS.
    ImePreedit {
        text: String,
        cursor: Option<(usize, usize)>,
    },
    ImeCommit(String),
    ImeDisabled,
}
