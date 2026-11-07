use crate::{Error, Limits};
use alloc::vec::Vec;

#[derive(Clone, Copy)]
struct Span {
    sum: f64,
    min: f64,
    count: usize,
}
impl Span {
    const EMPTY: Self = Self {
        sum: 0.0,
        min: f64::INFINITY,
        count: 0,
    };
    fn leaf(extent: f64) -> Self {
        Self {
            sum: extent,
            min: extent,
            count: 1,
        }
    }
    fn merge(self, other: Self) -> Self {
        Self {
            sum: self.sum + other.sum,
            min: self.min.min(other.min),
            count: self.count + other.count,
        }
    }
    fn weighted(self, gap: f64) -> f64 {
        self.sum + gap * self.count as f64
    }
}

/// Bounded tree of aggregate extents. Updates recompute ancestors instead of
/// repeatedly adding floating-point deltas, avoiding cumulative update drift.
pub(crate) struct Variable {
    nodes: Vec<Span>,
    base: usize,
    pub count: usize,
}
impl Variable {
    pub fn bytes(&self) -> usize {
        self.nodes.capacity() * core::mem::size_of::<Span>()
    }
    pub fn new(extents: &[f64], gap: f64, limits: Limits) -> Result<Self, Error> {
        if extents.len() > limits.max_items {
            return Err(Error::BudgetExceeded);
        }
        for &extent in extents {
            validate_extent(extent)?;
        }
        let base = extents
            .len()
            .max(1)
            .checked_next_power_of_two()
            .ok_or(Error::BudgetExceeded)?;
        let capacity = base.checked_mul(2).ok_or(Error::BudgetExceeded)?;
        let bytes = capacity
            .checked_mul(core::mem::size_of::<Span>())
            .ok_or(Error::BudgetExceeded)?;
        if bytes > limits.max_index_bytes {
            return Err(Error::BudgetExceeded);
        }
        let mut nodes = Vec::new();
        nodes
            .try_reserve_exact(capacity)
            .map_err(|_| Error::AllocationFailed)?;
        nodes.resize(capacity, Span::EMPTY);
        for (index, &extent) in extents.iter().enumerate() {
            nodes[base + index] = Span::leaf(extent);
        }
        for index in (1..base).rev() {
            nodes[index] = nodes[index * 2].merge(nodes[index * 2 + 1]);
        }
        validate_total(nodes[1].sum, nodes[1].min, extents.len(), gap, limits)?;
        Ok(Self {
            nodes,
            base,
            count: extents.len(),
        })
    }
    pub fn total(&self, gap: f64) -> f64 {
        total(self.nodes[1].sum, self.count, gap)
    }
    pub fn prefix(&self, count: usize, gap: f64) -> f64 {
        if count == self.count {
            return self.nodes[1].weighted(gap);
        }
        let mut node = self.base + count;
        let mut sum = 0.0;
        while node > 1 {
            if node % 2 == 1 {
                sum += self.nodes[node - 1].weighted(gap);
            }
            node /= 2;
        }
        sum
    }
    pub fn extent(&self, index: usize) -> f64 {
        self.nodes[self.base + index].sum
    }
    pub fn partition(&self, threshold: f64, inclusive: bool, gap: f64) -> usize {
        let mut node = 1;
        let mut prefix = 0.0;
        let mut count = 0;
        while node < self.base {
            let left = self.nodes[node * 2];
            let end = prefix + left.weighted(gap);
            if if inclusive {
                end <= threshold
            } else {
                end < threshold
            } {
                prefix = end;
                count += left.count;
                node = node * 2 + 1;
            } else {
                node *= 2;
            }
        }
        let leaf = self.nodes[node];
        let end = prefix + leaf.weighted(gap);
        if if inclusive {
            end <= threshold
        } else {
            end < threshold
        } {
            count += leaf.count;
        }
        count.min(self.count)
    }
    pub fn update(
        &mut self,
        index: usize,
        extent: f64,
        gap: f64,
        limits: Limits,
    ) -> Result<bool, Error> {
        validate_extent(extent)?;
        let leaf = self.base + index;
        if self.nodes[leaf].sum == extent {
            return Ok(false);
        }
        // Validate the hypothetical root before mutating any node.
        let mut candidate = Span::leaf(extent);
        let mut node = leaf;
        while node > 1 {
            candidate = if node.is_multiple_of(2) {
                candidate.merge(self.nodes[node + 1])
            } else {
                self.nodes[node - 1].merge(candidate)
            };
            node /= 2;
        }
        validate_total(candidate.sum, candidate.min, candidate.count, gap, limits)?;
        self.nodes[leaf] = Span::leaf(extent);
        node = leaf / 2;
        while node > 0 {
            self.nodes[node] = self.nodes[node * 2].merge(self.nodes[node * 2 + 1]);
            node /= 2;
        }
        Ok(true)
    }
}
pub(crate) fn validate_extent(value: f64) -> Result<(), Error> {
    if value.is_finite() && value > 0.0 {
        Ok(())
    } else {
        Err(Error::InvalidExtent)
    }
}
pub(crate) fn total(sum: f64, count: usize, gap: f64) -> f64 {
    if count == 0 {
        0.0
    } else {
        sum + gap * (count - 1) as f64
    }
}
pub(crate) fn validate_total(
    sum: f64,
    min: f64,
    count: usize,
    gap: f64,
    limits: Limits,
) -> Result<(), Error> {
    let extent = total(sum, count, gap);
    if !extent.is_finite() || extent > limits.max_extent {
        return Err(Error::BudgetExceeded);
    }
    // Leave headroom for prefix arithmetic; reject sub-resolution rows instead
    // of silently losing their nonempty intervals in a very large coordinate space.
    if count > 0 && (extent + gap) / min > (1u64 << 48) as f64 {
        return Err(Error::InsufficientPrecision);
    }
    Ok(())
}
