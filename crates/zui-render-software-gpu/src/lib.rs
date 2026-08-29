//! Future software-GPU renderer backend.
//!
//! This package intentionally defines only its availability contract today.
//! It will host wgpu software-adapter selection once the control set and
//! conformance scenes are complete.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SoftwareGpuRenderer;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SoftwareGpuUnavailable;

impl std::fmt::Display for SoftwareGpuUnavailable {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("software GPU renderer is not implemented")
    }
}

impl std::error::Error for SoftwareGpuUnavailable {}

impl SoftwareGpuRenderer {
    pub fn new() -> Result<Self, SoftwareGpuUnavailable> {
        Err(SoftwareGpuUnavailable)
    }
}
