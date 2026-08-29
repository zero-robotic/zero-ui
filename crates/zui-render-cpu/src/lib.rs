//! Future CPU raster renderer backend.
//!
//! The package is intentionally a non-functional placeholder until every
//! control has CPU raster conformance coverage.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CpuRenderer;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CpuRendererUnavailable;

impl std::fmt::Display for CpuRendererUnavailable {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("CPU renderer is not implemented")
    }
}

impl std::error::Error for CpuRendererUnavailable {}

impl CpuRenderer {
    pub fn new() -> Result<Self, CpuRendererUnavailable> {
        Err(CpuRendererUnavailable)
    }
}
