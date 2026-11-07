//! Dashboard compositions depend on portable controls/charts, never the runtime.
#![no_std]
extern crate alloc;
mod panel;
pub use panel::{DashboardPanelView, PanelStyle};
mod metric;
pub use metric::{MetricCardStyle, MetricCardView, MetricChange, MetricTone};
