//! Default-filled intervals are implicit. Only update paths allocate arena nodes.
use crate::{
    Error, Limits,
    index::{total, validate_extent, validate_total},
};
use alloc::vec::Vec;
const NONE: usize = usize::MAX;
#[derive(Clone, Copy)]
struct Span {
    sum: f64,
    min: f64,
}
impl Span {
    fn merge(self, other: Self) -> Self {
        Self {
            sum: self.sum + other.sum,
            min: self.min.min(other.min),
        }
    }
}
struct Node {
    children: [usize; 2],
    span: Span,
}
pub(crate) struct Sparse {
    nodes: Vec<Node>,
    pub count: usize,
    estimate: f64,
}
impl Sparse {
    pub fn new(count: usize, estimate: f64, gap: f64, limits: Limits) -> Result<Self, Error> {
        validate_extent(estimate)?;
        if count > limits.max_items {
            return Err(Error::BudgetExceeded);
        }
        validate_total(estimate * count as f64, estimate, count, gap, limits)?;
        Ok(Self {
            nodes: Vec::new(),
            count,
            estimate,
        })
    }
    fn root(&self) -> usize {
        if self.nodes.is_empty() { NONE } else { 0 }
    }
    fn span(&self, node: usize, count: usize) -> Span {
        if node == NONE {
            Span {
                sum: self.estimate * count as f64,
                min: self.estimate,
            }
        } else {
            self.nodes[node].span
        }
    }
    fn child(&self, node: usize, side: usize) -> usize {
        if node == NONE {
            NONE
        } else {
            self.nodes[node].children[side]
        }
    }
    pub fn bytes(&self) -> usize {
        self.nodes.capacity() * core::mem::size_of::<Node>()
    }
    /// Rebuild without default-only paths. Scratch includes marks and the new
    /// buffer while the old buffer remains live, allowing failure without mutation.
    pub fn compact(
        &mut self,
        max_scratch_bytes: usize,
        gap: f64,
        limits: Limits,
    ) -> Result<usize, Error> {
        let needed = self.analyze(self.root(), 0, self.count, &mut |_, _| {});
        let before = self.bytes();
        if needed == 0 {
            self.nodes = Vec::new();
            return Ok(before);
        }
        if needed == self.nodes.len() && self.nodes.capacity() == needed {
            return Ok(0);
        }
        let node_bytes = needed
            .checked_mul(core::mem::size_of::<Node>())
            .ok_or(Error::BudgetExceeded)?;
        let scratch = self
            .nodes
            .len()
            .checked_add(node_bytes)
            .ok_or(Error::BudgetExceeded)?;
        if node_bytes > limits.max_index_bytes || scratch > max_scratch_bytes {
            return Err(Error::BudgetExceeded);
        }
        let mut marks = Vec::new();
        marks
            .try_reserve_exact(self.nodes.len())
            .map_err(|_| Error::AllocationFailed)?;
        marks.resize(self.nodes.len(), 0_u8);
        self.analyze(self.root(), 0, self.count, &mut |node, keep| {
            marks[node] = u8::from(keep)
        });
        let mut compacted = Self {
            nodes: Vec::new(),
            count: self.count,
            estimate: self.estimate,
        };
        compacted
            .nodes
            .try_reserve_exact(needed)
            .map_err(|_| Error::AllocationFailed)?;
        self.copy_kept(self.root(), 0, self.count, &marks, &mut compacted);
        debug_assert_eq!(compacted.nodes.len(), needed);
        let root = compacted.span(compacted.root(), self.count);
        validate_total(root.sum, root.min, self.count, gap, limits)?;
        let released = before.saturating_sub(compacted.bytes());
        *self = compacted;
        Ok(released)
    }
    fn analyze(
        &self,
        node: usize,
        low: usize,
        high: usize,
        mark: &mut impl FnMut(usize, bool),
    ) -> usize {
        if node == NONE {
            return 0;
        }
        let kept = if high - low == 1 {
            usize::from(self.nodes[node].span.sum != self.estimate)
        } else {
            let mid = low + (high - low) / 2;
            let children = self.analyze(self.child(node, 0), low, mid, mark)
                + self.analyze(self.child(node, 1), mid, high, mark);
            if children == 0 { 0 } else { children + 1 }
        };
        mark(node, kept != 0);
        kept
    }
    fn copy_kept(
        &self,
        node: usize,
        low: usize,
        high: usize,
        marks: &[u8],
        output: &mut Self,
    ) -> usize {
        if node == NONE || marks[node] == 0 {
            return NONE;
        }
        let new = output.nodes.len();
        output.nodes.push(Node {
            children: [NONE; 2],
            span: self.nodes[node].span,
        });
        if high - low > 1 {
            let mid = low + (high - low) / 2;
            let left = self.copy_kept(self.child(node, 0), low, mid, marks, output);
            let right = self.copy_kept(self.child(node, 1), mid, high, marks, output);
            output.nodes[new].children = [left, right];
            output.nodes[new].span = output
                .span(left, mid - low)
                .merge(output.span(right, high - mid));
        }
        new
    }
    pub fn total(&self, gap: f64) -> f64 {
        total(self.span(self.root(), self.count).sum, self.count, gap)
    }
    pub fn extent(&self, index: usize) -> f64 {
        let (mut node, mut low, mut high) = (self.root(), 0, self.count);
        while high - low > 1 && node != NONE {
            let mid = low + (high - low) / 2;
            if index < mid {
                node = self.child(node, 0);
                high = mid;
            } else {
                node = self.child(node, 1);
                low = mid;
            }
        }
        if node == NONE {
            self.estimate
        } else {
            self.nodes[node].span.sum
        }
    }
    pub fn prefix(&self, count: usize, gap: f64) -> f64 {
        self.prefix_sum(self.root(), 0, self.count, count) + gap * count as f64
    }
    fn prefix_sum(&self, node: usize, low: usize, high: usize, count: usize) -> f64 {
        if count <= low {
            return 0.0;
        }
        if count >= high {
            return self.span(node, high - low).sum;
        }
        if node == NONE {
            return self.estimate * (count - low) as f64;
        }
        let mid = low + (high - low) / 2;
        if count <= mid {
            self.prefix_sum(self.child(node, 0), low, mid, count)
        } else {
            self.span(self.child(node, 0), mid - low).sum
                + self.prefix_sum(self.child(node, 1), mid, high, count)
        }
    }
    pub fn partition(&self, threshold: f64, inclusive: bool, gap: f64) -> usize {
        if self.count == 0 {
            return 0;
        }
        let fits = |end| {
            if inclusive {
                end <= threshold
            } else {
                end < threshold
            }
        };
        let (mut node, mut low, mut high, mut prefix) = (self.root(), 0, self.count, 0.0);
        while high - low > 1 {
            let mid = low + (high - low) / 2;
            let end =
                prefix + self.span(self.child(node, 0), mid - low).sum + gap * (mid - low) as f64;
            if fits(end) {
                prefix = end;
                node = self.child(node, 1);
                low = mid;
            } else {
                node = self.child(node, 0);
                high = mid;
            }
        }
        low + usize::from(fits(prefix + self.span(node, 1).sum + gap))
    }
    // Candidate validation and path allocation both precede mutation. Recompute
    // ancestors from child sums instead of accumulating floating-point deltas.
    pub fn update(
        &mut self,
        index: usize,
        extent: f64,
        gap: f64,
        limits: Limits,
    ) -> Result<bool, Error> {
        validate_extent(extent)?;
        if self.extent(index) == extent {
            return Ok(false);
        }
        let (candidate, needed) = self.candidate(self.root(), 0, self.count, index, extent);
        validate_total(candidate.sum, candidate.min, self.count, gap, limits)?;
        let max_nodes = limits.max_index_bytes / core::mem::size_of::<Node>();
        let required = self
            .nodes
            .len()
            .checked_add(needed)
            .ok_or(Error::BudgetExceeded)?;
        if required > max_nodes {
            return Err(Error::BudgetExceeded);
        }
        if required > self.nodes.capacity() {
            let capacity = required
                .max(self.nodes.capacity().saturating_mul(2))
                .min(max_nodes);
            self.nodes
                .try_reserve_exact(capacity - self.nodes.len())
                .map_err(|_| Error::AllocationFailed)?;
        }
        self.commit(self.root(), 0, self.count, index, extent);
        Ok(true)
    }
    fn candidate(
        &self,
        node: usize,
        low: usize,
        high: usize,
        index: usize,
        extent: f64,
    ) -> (Span, usize) {
        let needed = usize::from(node == NONE);
        if high - low == 1 {
            return (
                Span {
                    sum: extent,
                    min: extent,
                },
                needed,
            );
        }
        let mid = low + (high - low) / 2;
        if index < mid {
            let (left, more) = self.candidate(self.child(node, 0), low, mid, index, extent);
            (
                left.merge(self.span(self.child(node, 1), high - mid)),
                needed + more,
            )
        } else {
            let (right, more) = self.candidate(self.child(node, 1), mid, high, index, extent);
            (
                self.span(self.child(node, 0), mid - low).merge(right),
                needed + more,
            )
        }
    }
    fn commit(&mut self, node: usize, low: usize, high: usize, index: usize, extent: f64) -> usize {
        let node = if node == NONE {
            let node = self.nodes.len();
            self.nodes.push(Node {
                children: [NONE; 2],
                span: self.span(NONE, high - low),
            });
            node
        } else {
            node
        };
        if high - low == 1 {
            self.nodes[node].span = Span {
                sum: extent,
                min: extent,
            };
        } else {
            let mid = low + (high - low) / 2;
            let (side, start, end) = if index < mid {
                (0, low, mid)
            } else {
                (1, mid, high)
            };
            let child = self.commit(self.child(node, side), start, end, index, extent);
            self.nodes[node].children[side] = child;
            self.nodes[node].span = self
                .span(self.child(node, 0), mid - low)
                .merge(self.span(self.child(node, 1), high - mid));
        }
        node
    }
}
