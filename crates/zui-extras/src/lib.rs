//! Optional charts and dashboard components built on portable ZUI contracts.
#![no_std]
extern crate alloc;

#[cfg(feature = "charts")]
pub mod charts;
#[cfg(feature = "dashboard")]
pub mod dashboard;
