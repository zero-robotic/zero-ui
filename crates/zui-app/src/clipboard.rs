//! Optional desktop text clipboard adapter implementing the portable contract.
use zui_api::clipboard::{Clipboard, ClipboardError};

/// Lazily connects on first use and retains the connection for clipboard ownership.
/// Construction does not read or overwrite the user's clipboard.
#[derive(Default)]
pub struct NativeClipboard {
    inner: Option<arboard::Clipboard>,
}
impl NativeClipboard {
    fn connection(&mut self) -> Result<&mut arboard::Clipboard, ClipboardError> {
        if self.inner.is_none() {
            self.inner = Some(arboard::Clipboard::new().map_err(|_| ClipboardError::Unavailable)?);
        }
        Ok(self.inner.as_mut().expect("initialized clipboard"))
    }
}
impl Clipboard for NativeClipboard {
    fn read_text(&mut self, max_bytes: usize) -> Result<String, ClipboardError> {
        let text = self.connection()?.get_text().map_err(|error| match error {
            arboard::Error::ContentNotAvailable => ClipboardError::NoText,
            _ => ClipboardError::ReadFailed,
        })?;
        if text.len() > max_bytes {
            return Err(ClipboardError::BudgetExceeded);
        }
        Ok(text)
    }
    fn write_text(&mut self, text: &str) -> Result<(), ClipboardError> {
        self.connection()?
            .set_text(text)
            .map_err(|_| ClipboardError::WriteFailed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn construction_does_not_connect_to_system_clipboard() {
        let clipboard = NativeClipboard::default();
        assert!(clipboard.inner.is_none());
    }
}
