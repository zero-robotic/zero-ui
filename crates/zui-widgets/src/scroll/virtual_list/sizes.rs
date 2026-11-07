//! Immutable size descriptions; index rebuilding is separate from scrolling.
use alloc::{rc::Rc, vec::Vec};
use zui_virtual::{Error, ExtentIndex, Limits};

#[derive(Clone)]
pub(super) enum RowSizes {
    Fixed {
        count: usize,
        height: f64,
    },
    Variable(Rc<[f64]>),
    Measured {
        count: usize,
        estimate: f64,
        revision: u64,
    },
}
impl RowSizes {
    pub fn same(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::Fixed {
                    count: a,
                    height: x,
                },
                Self::Fixed {
                    count: b,
                    height: y,
                },
            ) => a == b && x == y,
            (
                Self::Measured {
                    count: a,
                    estimate: x,
                    revision: r,
                },
                Self::Measured {
                    count: b,
                    estimate: y,
                    revision: s,
                },
            ) => a == b && x == y && r == s,
            (Self::Variable(a), Self::Variable(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
    pub fn measured(&self) -> bool {
        matches!(self, Self::Measured { .. })
    }
    pub fn height(&self, index: usize) -> Option<f32> {
        match self {
            Self::Fixed { height, .. } => Some(*height as f32),
            Self::Variable(heights) => Some(heights[index] as f32),
            Self::Measured { .. } => None,
        }
    }
    pub fn build_index(&self, gap: f64, limits: Limits) -> Result<ExtentIndex, Error> {
        if !(gap as f32).is_finite() || gap < 0.0 {
            return Err(Error::InvalidGap);
        }
        let gap = gap as f32 as f64;
        match self {
            Self::Fixed { count, height } => {
                ExtentIndex::fixed(*count, normalize(*height)?, gap, limits)
            }
            Self::Measured {
                count, estimate, ..
            } => ExtentIndex::estimated(*count, normalize(*estimate)?, gap, limits),
            Self::Variable(heights) => {
                if heights.len() > limits.max_items {
                    return Err(Error::BudgetExceeded);
                }
                let mut normalized = Vec::new();
                normalized
                    .try_reserve_exact(heights.len())
                    .map_err(|_| Error::AllocationFailed)?;
                for &height in heights.iter() {
                    normalized.push(normalize(height)?);
                }
                ExtentIndex::variable(&normalized, gap, limits)
            }
        }
    }
}
fn normalize(height: f64) -> Result<f64, Error> {
    let height = height as f32;
    if height.is_finite() && height > 0.0 {
        Ok(height as f64)
    } else {
        Err(Error::InvalidExtent)
    }
}
