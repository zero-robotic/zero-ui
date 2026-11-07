//! Desktop window lifecycle and input translation. No renderer dependency.
mod input;
mod runner;
mod schedule;
#[cfg(feature = "software")]
mod software;
mod window;

pub use runner::run;
#[cfg(feature = "software")]
pub use software::SoftwareSurface;

fn backend(error: impl std::fmt::Display) -> zui_platform::PlatformError {
    zui_platform::PlatformError::Backend(error.to_string())
}
