//! Axis-independent extent indexing for virtual lists and table rows/columns.
//! No widgets, IDs, layout engine, renderer, platform or application dependency.
mod index;
mod sparse;
use core::{fmt, ops::Range};
use index::{Variable, total, validate_extent, validate_total};
use sparse::Sparse;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Limits {
    pub max_items: usize,
    pub max_index_bytes: usize,
    pub max_extent: f64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_items: 1_000_000,
            max_index_bytes: 64 * 1024 * 1024,
            max_extent: 1_000_000_000_000.0,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidExtent,
    InvalidGap,
    InvalidViewport,
    InvalidLimits,
    InvalidIndex,
    FixedExtents,
    BudgetExceeded,
    InsufficientPrecision,
    AllocationFailed,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "virtual extent error: {self:?}")
    }
}
impl core::error::Error for Error {}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ItemExtent {
    pub start: f64,
    pub end: f64,
}
/// An item index and offset relative to its start. Negative offsets represent
/// the preceding gap. Reordering requires the owner to remap a stable item key.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Anchor {
    pub index: usize,
    pub within: f64,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Window {
    /// Half-open range including overscan. Only these items need materialization.
    pub items: Range<usize>,
    /// Offset clamped against current content and viewport extents.
    pub offset: f64,
    pub total_extent: f64,
    pub leading_extent: f64,
    pub trailing_extent: f64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alignment {
    Nearest,
    Start,
    Center,
    End,
}
enum Storage {
    Fixed { count: usize, extent: f64 },
    Variable(Variable),
    Estimated(Sparse),
}
pub struct ExtentIndex {
    storage: Storage,
    gap: f64,
    limits: Limits,
}
impl ExtentIndex {
    fn validate(gap: f64, limits: Limits) -> Result<(), Error> {
        if !limits.max_extent.is_finite() || limits.max_extent <= 0.0 {
            return Err(Error::InvalidLimits);
        }
        if !gap.is_finite() || gap < 0.0 {
            return Err(Error::InvalidGap);
        }
        if gap > limits.max_extent {
            return Err(Error::BudgetExceeded);
        }
        Ok(())
    }
    /// No per-item allocation, regardless of item count.
    pub fn fixed(count: usize, extent: f64, gap: f64, limits: Limits) -> Result<Self, Error> {
        Self::validate(gap, limits)?;
        validate_extent(extent)?;
        if count > limits.max_items {
            return Err(Error::BudgetExceeded);
        }
        validate_total(extent * count as f64, extent, count, gap, limits)?;
        Ok(Self {
            storage: Storage::Fixed { count, extent },
            gap,
            limits,
        })
    }
    /// Copies extents into a bounded aggregate index; zero-height items must be
    /// removed from the logical sequence instead of creating ambiguous intervals.
    pub fn variable(extents: &[f64], gap: f64, limits: Limits) -> Result<Self, Error> {
        Self::validate(gap, limits)?;
        Ok(Self {
            storage: Storage::Variable(Variable::new(extents, gap, limits)?),
            gap,
            limits,
        })
    }
    /// No per-item initialization. Updates allocate only paths to measured rows;
    /// allocated paths are retained for reuse until explicit compaction or drop.
    pub fn estimated(count: usize, estimate: f64, gap: f64, limits: Limits) -> Result<Self, Error> {
        Self::validate(gap, limits)?;
        Ok(Self {
            storage: Storage::Estimated(Sparse::new(count, estimate, gap, limits)?),
            gap,
            limits,
        })
    }
    /// Allocated node-buffer capacity only; excludes self, inputs and allocator overhead.
    pub fn allocated_index_bytes(&self) -> usize {
        match &self.storage {
            Storage::Fixed { .. } => 0,
            Storage::Variable(index) => index.bytes(),
            Storage::Estimated(index) => index.bytes(),
        }
    }
    /// Remove default-only sparse paths and spare capacity, retaining all values.
    /// Returns released node-buffer bytes. Fixed/dense storage is unchanged.
    /// Temporary mark/new-buffer allocations are bounded separately from retained
    /// index bytes. Failure leaves the old index intact. Work is O(allocated nodes).
    pub fn compact(&mut self, max_scratch_bytes: usize) -> Result<usize, Error> {
        match &mut self.storage {
            Storage::Estimated(index) => index.compact(max_scratch_bytes, self.gap, self.limits),
            _ => Ok(0),
        }
    }
    pub fn len(&self) -> usize {
        match &self.storage {
            Storage::Fixed { count, .. } => *count,
            Storage::Variable(index) => index.count,
            Storage::Estimated(index) => index.count,
        }
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn total_extent(&self) -> f64 {
        match &self.storage {
            Storage::Fixed { count, extent } => total(*extent * *count as f64, *count, self.gap),
            Storage::Variable(index) => index.total(self.gap),
            Storage::Estimated(index) => index.total(self.gap),
        }
    }
    fn prefix(&self, count: usize) -> f64 {
        match &self.storage {
            Storage::Fixed { extent, .. } => (*extent + self.gap) * count as f64,
            Storage::Variable(index) => index.prefix(count, self.gap),
            Storage::Estimated(index) => index.prefix(count, self.gap),
        }
    }
    pub fn item(&self, index: usize) -> Result<ItemExtent, Error> {
        if index >= self.len() {
            return Err(Error::InvalidIndex);
        }
        let extent = match &self.storage {
            Storage::Fixed { extent, .. } => *extent,
            Storage::Variable(storage) => storage.extent(index),
            Storage::Estimated(storage) => storage.extent(index),
        };
        let start = self.prefix(index);
        Ok(ItemExtent {
            start,
            end: start + extent,
        })
    }
    pub fn update(&mut self, index: usize, extent: f64) -> Result<bool, Error> {
        if index >= self.len() {
            return Err(Error::InvalidIndex);
        }
        match &mut self.storage {
            Storage::Fixed { .. } => Err(Error::FixedExtents),
            Storage::Variable(storage) => storage.update(index, extent, self.gap, self.limits),
            Storage::Estimated(storage) => storage.update(index, extent, self.gap, self.limits),
        }
    }
    pub fn anchor(&self, offset: f64, viewport: f64) -> Result<Option<Anchor>, Error> {
        let offset = self.offset(offset, viewport)?;
        if self.is_empty() {
            return Ok(None);
        }
        let index = self.partition(offset + self.gap, true).min(self.len() - 1);
        Ok(Some(Anchor {
            index,
            within: offset - self.item(index)?.start,
        }))
    }
    /// Restore an anchor after extent updates; shrinking its item clamps the
    /// within-item position. Does not automatically remap insertions or reorders.
    pub fn offset_for_anchor(&self, anchor: Anchor, viewport: f64) -> Result<f64, Error> {
        self.offset(0.0, viewport)?;
        if !anchor.within.is_finite() {
            return Err(Error::InvalidViewport);
        }
        let item = self.item(anchor.index)?;
        let position = item.start + anchor.within.clamp(-self.gap, item.end - item.start);
        self.offset(position.max(0.0), viewport)
    }
    fn partition(&self, threshold: f64, inclusive: bool) -> usize {
        match &self.storage {
            Storage::Variable(index) => index.partition(threshold, inclusive, self.gap),
            Storage::Estimated(index) => index.partition(threshold, inclusive, self.gap),
            Storage::Fixed { count, .. } => {
                let (mut low, mut high) = (0, *count);
                while low < high {
                    let mid = low + (high - low) / 2;
                    let end = self.prefix(mid + 1);
                    if if inclusive {
                        end <= threshold
                    } else {
                        end < threshold
                    } {
                        low = mid + 1;
                    } else {
                        high = mid;
                    }
                }
                low
            }
        }
    }
    fn offset(&self, offset: f64, viewport: f64) -> Result<f64, Error> {
        if !offset.is_finite() || offset < 0.0 || !viewport.is_finite() || viewport < 0.0 {
            return Err(Error::InvalidViewport);
        }
        Ok(offset.min((self.total_extent() - viewport).max(0.0)))
    }
    /// O(log N) query; overscan is a count of extra items on each side.
    /// A zero viewport returns no items, even with overscan.
    pub fn window(&self, offset: f64, viewport: f64, overscan: usize) -> Result<Window, Error> {
        let offset = self.offset(offset, viewport)?;
        let total = self.total_extent();
        let count = self.len();
        let first = self.partition(offset + self.gap, true);
        let items = if viewport == 0.0 || count == 0 {
            first..first
        } else {
            let last = self
                .partition((offset + viewport).min(total), false)
                .saturating_add(1)
                .min(count)
                .max(first);
            first.saturating_sub(overscan)..last.saturating_add(overscan).min(count)
        };
        let leading = if items.start == count {
            total
        } else {
            self.prefix(items.start)
        };
        let end = if items.is_empty() {
            leading
        } else {
            self.item(items.end - 1)?.end
        };
        Ok(Window {
            items,
            offset,
            total_extent: total,
            leading_extent: leading,
            trailing_extent: (total - end).max(0.0),
        })
    }
    /// Position an index without materializing it. Does not change an existing
    /// scroll state; callers apply the returned offset through their own contract.
    pub fn offset_for(
        &self,
        index: usize,
        current: f64,
        viewport: f64,
        alignment: Alignment,
    ) -> Result<f64, Error> {
        let current = self.offset(current, viewport)?;
        let item = self.item(index)?;
        let next = if viewport == 0.0 {
            item.start
        } else {
            match alignment {
                Alignment::Start => item.start,
                Alignment::Center => item.start + (item.end - item.start - viewport) / 2.0,
                Alignment::End => item.end - viewport,
                Alignment::Nearest => {
                    let start = item.start - current;
                    let end = item.end - current - viewport;
                    let delta = if item.end - item.start <= viewport {
                        if start < 0.0 { start } else { end.max(0.0) }
                    } else if start > 0.0 {
                        start
                    } else {
                        end.min(0.0)
                    };
                    current + delta
                }
            }
        };
        Ok(next.clamp(0.0, (self.total_extent() - viewport).max(0.0)))
    }
}
