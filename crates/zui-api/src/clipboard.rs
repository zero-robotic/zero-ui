//! Text clipboard contract; implementations are injected by application assembly.
use alloc::string::String;
use core::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipboardError {
    Unavailable,
    NoText,
    BudgetExceeded,
    ReadFailed,
    WriteFailed,
}
impl fmt::Display for ClipboardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "clipboard error: {self:?}")
    }
}
impl core::error::Error for ClipboardError {}

pub trait Clipboard {
    /// UTF-8 output must not exceed max_bytes. This is a returned-text limit,
    /// not a guarantee about allocations inside the operating system.
    fn read_text(&mut self, max_bytes: usize) -> Result<String, ClipboardError>;
    /// Success means the text was accepted. Callers must not delete cut text
    /// until this returns Ok; an error does not guarantee OS clipboard rollback.
    fn write_text(&mut self, text: &str) -> Result<(), ClipboardError>;
}

pub struct UnavailableClipboard;
impl Clipboard for UnavailableClipboard {
    fn read_text(&mut self, _: usize) -> Result<String, ClipboardError> {
        Err(ClipboardError::Unavailable)
    }
    fn write_text(&mut self, _: &str) -> Result<(), ClipboardError> {
        Err(ClipboardError::Unavailable)
    }
}
