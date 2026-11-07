//! Validated finite ranges shared by numeric controls.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NumericRange {
    min: f64,
    max: f64,
    step: f64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidNumericRange;
impl NumericRange {
    /// Endpoints are always selectable, even if the last interval is shorter.
    /// Steps must be representable throughout the range and no larger than it.
    pub fn new(min: f64, max: f64, step: f64) -> Result<Self, InvalidNumericRange> {
        let span = max - min;
        if !min.is_finite()
            || !max.is_finite()
            || !span.is_finite()
            || span <= 0.0
            || !step.is_finite()
            || step <= 0.0
            || step > span
            || span / step > (1_u64 << 52) as f64
            || min + step <= min
            || max - step >= max
        {
            return Err(InvalidNumericRange);
        }
        Ok(Self { min, max, step })
    }
    pub fn min(self) -> f64 {
        self.min
    }
    pub fn max(self) -> f64 {
        self.max
    }
    pub fn step(self) -> f64 {
        self.step
    }
    pub fn contains(self, value: f64) -> bool {
        value.is_finite() && value >= self.min && value <= self.max
    }
    pub(crate) fn fraction(self, value: f64) -> f64 {
        (value - self.min) / (self.max - self.min)
    }
    pub(crate) fn at(self, fraction: f64) -> f64 {
        if fraction <= 0.0 {
            return self.min;
        }
        if fraction >= 1.0 {
            return self.max;
        }
        let value = self.min + fraction * (self.max - self.min);
        let index = (((value - self.min) / self.step + 0.5) as u64) as f64;
        let grid = (self.min + index * self.step).clamp(self.min, self.max);
        // The final endpoint participates in nearest-value snapping.
        if self.max - value < (grid - value).abs() {
            self.max
        } else {
            grid
        }
    }
    pub(crate) fn shift(self, value: f64, forward: bool, count: u32) -> f64 {
        let index = (value - self.min) / self.step;
        // Correct division noise at values previously produced from this grid.
        let nearest = ((index + 0.5) as u64) as f64;
        let tolerance = (f64::EPSILON * index.abs() * 4.0).min(0.25);
        let aligned = (index - nearest).abs() <= tolerance;
        let floor = (index as u64) as f64;
        let base = if aligned {
            nearest
        } else if forward {
            floor
        } else {
            floor + 1.0
        };
        let next = if forward {
            base + count as f64
        } else {
            base - count as f64
        };
        if next <= 0.0 {
            self.min
        } else {
            (self.min + next * self.step).clamp(self.min, self.max)
        }
    }
}
impl core::fmt::Display for InvalidNumericRange {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("invalid or unrepresentable numeric range")
    }
}
impl core::error::Error for InvalidNumericRange {}
