//! Bounded ingestion is separate from immutable render snapshots and scheduling.
use super::{MAX_SPARKLINE_SAMPLES, SparklineSeries};
use alloc::collections::VecDeque;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryError {
    InvalidCapacity,
    AllocationFailed,
    /// Index in the incoming batch; zero for push.
    NonFinite(usize),
}
impl core::fmt::Display for HistoryError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidCapacity => {
                f.write_str("history capacity must be between 1 and MAX_SPARKLINE_SAMPLES")
            }
            Self::AllocationFailed => f.write_str("cannot reserve history storage"),
            Self::NonFinite(index) => write!(f, "non-finite incoming sample at {index}"),
        }
    }
}
impl core::error::Error for HistoryError {}

/// Fixed-capacity, oldest-first sample history. None is an explicit missing slot.
/// Ingestion never schedules a frame; the application chooses snapshot frequency.
pub struct SparklineHistory {
    values: VecDeque<Option<f64>>,
    limit: usize,
    snapshot: Option<SparklineSeries>,
}
impl SparklineHistory {
    pub fn new(capacity: usize) -> Result<Self, HistoryError> {
        if capacity == 0 || capacity > MAX_SPARKLINE_SAMPLES {
            return Err(HistoryError::InvalidCapacity);
        }
        let mut values = VecDeque::new();
        values
            .try_reserve_exact(capacity)
            .map_err(|_| HistoryError::AllocationFailed)?;
        Ok(Self {
            values,
            limit: capacity,
            snapshot: None,
        })
    }
    pub fn capacity(&self) -> usize {
        self.limit
    }
    pub fn len(&self) -> usize {
        self.values.len()
    }
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
    pub fn iter(&self) -> impl ExactSizeIterator<Item = Option<f64>> + DoubleEndedIterator + '_ {
        self.values.iter().copied()
    }
    /// Returns the evicted slot. Some(None) means an evicted missing sample;
    /// None means storage was not full. Invalid input leaves all state unchanged.
    pub fn push(&mut self, value: Option<f64>) -> Result<Option<Option<f64>>, HistoryError> {
        if value.is_some_and(|v| !v.is_finite()) {
            return Err(HistoryError::NonFinite(0));
        }
        Ok(self.push_validated(value))
    }
    fn push_validated(&mut self, value: Option<f64>) -> Option<Option<f64>> {
        let evicted = if self.len() == self.limit {
            self.values.pop_front()
        } else {
            None
        };
        self.values.push_back(value);
        self.snapshot = None;
        evicted
    }
    /// Validates the whole batch before mutation, then appends in arrival order.
    /// Returns the number of evictions, including samples from this batch.
    pub fn extend(&mut self, values: &[Option<f64>]) -> Result<usize, HistoryError> {
        for (index, value) in values.iter().enumerate() {
            if value.is_some_and(|v| !v.is_finite()) {
                return Err(HistoryError::NonFinite(index));
            }
        }
        let evictions = values.len().saturating_sub(self.limit - self.len());
        for value in values {
            self.push_validated(*value);
        }
        Ok(evictions)
    }
    /// Keeps the reserved ring storage; existing render snapshots remain valid.
    pub fn clear(&mut self) {
        if !self.values.is_empty() {
            self.values.clear();
            self.snapshot = None;
        }
    }
    /// O(N) copy/extrema scan on the first call after mutation, O(1) shared clone
    /// otherwise. Snapshot allocation uses the global allocator and is not covered
    /// by new()'s AllocationFailed result. Retained snapshots consume extra memory.
    pub fn snapshot(&mut self) -> SparklineSeries {
        self.snapshot
            .get_or_insert_with(|| {
                SparklineSeries::new(self.values.iter().copied().collect())
                    .expect("history contains only validated samples within the series limit")
            })
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;
    #[test]
    fn ring_order_evictions_and_gaps_match_bounded_reference() {
        for capacity in [1, 2, 7, 128] {
            let mut history = SparklineHistory::new(capacity).unwrap();
            let allocation = history.values.capacity();
            let mut reference = Vec::new();
            for index in 0..1024 {
                let sample = (index % 5 != 0).then_some(index as f64);
                let expected = if reference.len() == capacity {
                    Some(reference.remove(0))
                } else {
                    None
                };
                reference.push(sample);
                assert_eq!(history.push(sample).unwrap(), expected);
                assert_eq!(history.iter().collect::<Vec<_>>(), reference);
                assert_eq!(history.values.capacity(), allocation);
            }
            assert_eq!(history.snapshot().values(), reference);
        }
    }
    #[test]
    fn snapshots_are_cached_immutable_and_recompute_extrema_after_eviction() {
        let mut history = SparklineHistory::new(3).unwrap();
        history.extend(&[Some(-9.0), None, Some(4.0)]).unwrap();
        let old = history.snapshot();
        assert_eq!(old.bounds(), Some((-9.0, 4.0)));
        assert!(core::ptr::eq(
            old.values().as_ptr(),
            history.snapshot().values().as_ptr()
        ));
        history.push(Some(2.0)).unwrap();
        let new = history.snapshot();
        assert_eq!(new.values(), &[None, Some(4.0), Some(2.0)]);
        assert_eq!(new.bounds(), Some((2.0, 4.0)));
        assert_eq!(old.values(), &[Some(-9.0), None, Some(4.0)]);
        assert!(!core::ptr::eq(old.values().as_ptr(), new.values().as_ptr()));
        history.clear();
        assert!(history.is_empty());
        assert_eq!(history.capacity(), 3);
        assert!(history.snapshot().values().is_empty());
        assert_eq!(new.bounds(), Some((2.0, 4.0)));
    }
    #[test]
    fn batch_validation_is_atomic_and_large_batches_keep_only_latest_slots() {
        assert!(matches!(
            SparklineHistory::new(0),
            Err(HistoryError::InvalidCapacity)
        ));
        assert!(matches!(
            SparklineHistory::new(MAX_SPARKLINE_SAMPLES + 1),
            Err(HistoryError::InvalidCapacity)
        ));
        let mut history = SparklineHistory::new(2).unwrap();
        history.push(Some(1.0)).unwrap();
        let before = history.snapshot();
        assert_eq!(
            history.extend(&[Some(2.0), Some(f64::NAN), None]),
            Err(HistoryError::NonFinite(1))
        );
        assert_eq!(
            history.push(Some(f64::INFINITY)),
            Err(HistoryError::NonFinite(0))
        );
        assert_eq!(history.extend(&[]), Ok(0));
        assert!(core::ptr::eq(
            before.values().as_ptr(),
            history.snapshot().values().as_ptr()
        ));
        assert_eq!(
            history.extend(&[Some(2.0), None, Some(4.0), Some(5.0)]),
            Ok(3)
        );
        assert_eq!(history.snapshot().values(), &[Some(4.0), Some(5.0)]);
    }
}
