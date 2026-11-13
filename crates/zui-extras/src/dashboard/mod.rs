//! Dashboard compositions depend on portable controls/charts, never the runtime.
mod panel;
pub use panel::{DashboardPanelView, PanelStyle};
mod metric;
pub use metric::{MetricCardStyle, MetricCardView, MetricChange, MetricTone};
