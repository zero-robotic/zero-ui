//! Window, input and presentation contracts. No desktop backend or application.
#![no_std]
extern crate alloc;

mod event;
mod window;

pub use event::*;
pub use window::*;

use alloc::{boxed::Box, string::String};
use core::fmt;
use zui_paint::RenderError;

#[derive(Debug)]
pub enum PlatformError {
    Backend(String),
    Render(RenderError),
    Unsupported(&'static str),
    /// Opaque error from an injected handler; the platform knows no application types.
    Handler(Box<dyn core::error::Error + Send + Sync>),
}

impl fmt::Display for PlatformError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Backend(message) => f.write_str(message),
            Self::Render(error) => error.fmt(f),
            Self::Unsupported(message) => f.write_str(message),
            Self::Handler(error) => error.fmt(f),
        }
    }
}

impl core::error::Error for PlatformError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::Render(error) => Some(error),
            Self::Handler(error) => Some(error.as_ref()),
            _ => None,
        }
    }
}

impl From<RenderError> for PlatformError {
    fn from(error: RenderError) -> Self {
        Self::Render(error)
    }
}
