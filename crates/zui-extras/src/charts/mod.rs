//! Portable chart widgets; data and painting never depend on the runtime/backend.
mod history;
pub use history::{HistoryError, SparklineHistory};
mod series;
mod sparkline;
pub use series::{
    ChartRange, InvalidChartRange, MAX_SPARKLINE_SAMPLES, SeriesError, SparklineSeries,
};
pub use sparkline::{Sparkline, SparklineStyle, SparklineView};
