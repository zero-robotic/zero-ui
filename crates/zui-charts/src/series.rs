use alloc::{rc::Rc, vec::Vec};
/// Maximum samples per sparkline; large datasets need explicit aggregation.
pub const MAX_SPARKLINE_SAMPLES: usize = 65_536;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SeriesError {
    TooManySamples,
    NonFinite(usize),
}
impl core::fmt::Display for SeriesError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::TooManySamples => f.write_str("too many sparkline samples"),
            Self::NonFinite(index) => write!(f, "non-finite sample at {index}"),
        }
    }
}
impl core::error::Error for SeriesError {}
/// Immutable uniformly spaced values. None preserves its time slot as a gap.
#[derive(Clone, Debug, PartialEq)]
pub struct SparklineSeries {
    values: Rc<[Option<f64>]>,
    bounds: Option<(f64, f64)>,
}
impl SparklineSeries {
    pub fn new(values: Vec<Option<f64>>) -> Result<Self, SeriesError> {
        if values.len() > MAX_SPARKLINE_SAMPLES {
            return Err(SeriesError::TooManySamples);
        }
        let mut bounds: Option<(f64, f64)> = None;
        for (index, value) in values.iter().enumerate() {
            if let Some(value) = value {
                if !value.is_finite() {
                    return Err(SeriesError::NonFinite(index));
                }
                bounds = Some(match bounds {
                    Some((min, max)) => (min.min(*value), max.max(*value)),
                    None => (*value, *value),
                });
            }
        }
        Ok(Self {
            values: values.into(),
            bounds,
        })
    }
    pub fn values(&self) -> &[Option<f64>] {
        &self.values
    }
    pub fn bounds(&self) -> Option<(f64, f64)> {
        self.bounds
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChartRange {
    min: f64,
    max: f64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidChartRange;
impl core::fmt::Display for InvalidChartRange {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("chart range must have finite increasing endpoints")
    }
}
impl core::error::Error for InvalidChartRange {}
impl ChartRange {
    pub fn new(min: f64, max: f64) -> Result<Self, InvalidChartRange> {
        if !min.is_finite() || !max.is_finite() || min >= max {
            return Err(InvalidChartRange);
        }
        Ok(Self { min, max })
    }
    pub fn min(self) -> f64 {
        self.min
    }
    pub fn max(self) -> f64 {
        self.max
    }
}
pub(crate) fn fraction(value: f64, min: f64, max: f64) -> f64 {
    if min == max {
        return 0.5;
    }
    if value <= min {
        return 0.0;
    }
    if value >= max {
        return 1.0;
    }
    let span = max - min;
    if span.is_finite() {
        (value - min) / span
    } else {
        (value * 0.5 - min * 0.5) / (max * 0.5 - min * 0.5)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ranges_support_extreme_and_subnormal_values_without_nonfinite_geometry() {
        assert_eq!(fraction(0.0, -f64::MAX, f64::MAX), 0.5);
        let tiny = f64::from_bits(1);
        assert_eq!(fraction(tiny, 0.0, tiny * 2.0), 0.5);
        assert_eq!(fraction(10.0, 10.0, 10.0), 0.5);
        assert_eq!(fraction(-1.0, 0.0, 10.0), 0.0);
        assert_eq!(fraction(20.0, 0.0, 10.0), 1.0);
        for (min, max) in [
            (1.0, 1.0),
            (2.0, 1.0),
            (f64::NAN, 1.0),
            (0.0, f64::INFINITY),
        ] {
            assert!(ChartRange::new(min, max).is_err());
        }
    }
    #[test]
    fn series_preserves_gaps_and_validates_before_use() {
        let series = SparklineSeries::new(alloc::vec![Some(3.0), None, Some(-2.0)]).unwrap();
        assert_eq!(series.bounds(), Some((-2.0, 3.0)));
        assert_eq!(series.values()[1], None);
        assert_eq!(
            SparklineSeries::new(alloc::vec![None; 3]).unwrap().bounds(),
            None
        );
        assert_eq!(
            SparklineSeries::new(alloc::vec![Some(f64::NAN)]),
            Err(SeriesError::NonFinite(0))
        );
        assert_eq!(
            SparklineSeries::new(alloc::vec![None; MAX_SPARKLINE_SAMPLES + 1]),
            Err(SeriesError::TooManySamples)
        );
    }
}
