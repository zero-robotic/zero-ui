//! Renderer error types.

use zui_core::WindowId;

use crate::{ImageId, SurfaceTransitionError};

#[derive(Debug)]
pub enum RenderError {
    AdapterUnavailable,
    Device(String),
    Surface(String),
    SurfaceNotAttached(WindowId),
    SurfaceAlreadyAttached(WindowId),
    UnsupportedSurfaceTarget,
    InvalidSurfaceTransition(SurfaceTransitionError),
    SurfaceOutdated,
    SurfaceLost,
    InvalidCommand(String),
    MissingImage(ImageId),
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AdapterUnavailable => write!(f, "no compatible GPU adapter available"),
            Self::Device(message) => write!(f, "GPU device error: {message}"),
            Self::Surface(message) => write!(f, "surface error: {message}"),
            Self::SurfaceNotAttached(id) => write!(f, "surface {id:?} is not attached"),
            Self::SurfaceAlreadyAttached(id) => write!(f, "surface {id:?} is already attached"),
            Self::UnsupportedSurfaceTarget => {
                f.write_str("renderer does not support this surface target")
            }
            Self::InvalidSurfaceTransition(error) => error.fmt(f),
            Self::SurfaceOutdated => write!(f, "surface configuration is outdated"),
            Self::SurfaceLost => write!(f, "surface was lost"),
            Self::InvalidCommand(message) => write!(f, "invalid paint command: {message}"),
            Self::MissingImage(id) => write!(f, "image resource {id:?} is not registered"),
        }
    }
}

impl std::error::Error for RenderError {}

impl From<SurfaceTransitionError> for RenderError {
    fn from(error: SurfaceTransitionError) -> Self {
        Self::InvalidSurfaceTransition(error)
    }
}
