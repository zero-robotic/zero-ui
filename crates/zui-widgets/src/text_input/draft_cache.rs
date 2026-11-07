//! Explicit bounded registration and least-recently-used idle payload maintenance.
use super::TextInputDraft;
use alloc::vec::Vec;
use core::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DraftCacheError {
    EntryLimit,
    AllocationFailed,
    DraftInUse,
}
impl fmt::Display for DraftCacheError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "draft cache error: {self:?}")
    }
}
impl core::error::Error for DraftCacheError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DraftCachePolicy {
    /// Preserve committed text/selection, even when the budget cannot be met.
    HistoryOnly,
    /// After histories, discard idle text/selection as needed. Explicitly lossy.
    DiscardIdleDrafts,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DraftCacheUsage {
    pub entries: usize,
    /// Active payload is outside the cache; never counted as zero-byte idle state.
    pub active_entries: usize,
    pub idle_allocated_bytes: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DraftCacheReport {
    pub before: DraftCacheUsage,
    pub after: DraftCacheUsage,
    pub history_released_bytes: usize,
    pub discarded_drafts: usize,
    /// Applies only to idle payload at the end of this maintenance call.
    pub budget_met: bool,
}

/// A bounded business-key registry. Access updates LRU order; no background work
/// or automatic loss of drafts. Maintenance clears payload but preserves handles
/// and keys, so external cloned handles retain their identity. Remove keys explicitly.
/// Lookup is O(entries), intended for a bounded working set, not a whole dataset.
pub struct TextInputDraftCache<K> {
    entries: Vec<(K, TextInputDraft)>,
    max_entries: usize,
}
impl<K: Eq> TextInputDraftCache<K> {
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: Vec::new(),
            max_entries,
        }
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn max_entries(&self) -> usize {
        self.max_entries
    }
    pub fn get(&mut self, key: &K) -> Option<TextInputDraft> {
        let position = self
            .entries
            .iter()
            .position(|(candidate, _)| candidate == key)?;
        let entry = self.entries.remove(position);
        let handle = entry.1.clone();
        self.entries.push(entry);
        Some(handle)
    }
    /// Existing keys remain available at the entry limit. New keys fail without
    /// eviction or state changes. Allocation failure here covers the entry array.
    pub fn get_or_insert(&mut self, key: K) -> Result<TextInputDraft, DraftCacheError> {
        if let Some(handle) = self.get(&key) {
            return Ok(handle);
        }
        if self.entries.len() == self.max_entries {
            return Err(DraftCacheError::EntryLimit);
        }
        self.entries
            .try_reserve_exact(1)
            .map_err(|_| DraftCacheError::AllocationFailed)?;
        let handle = TextInputDraft::new();
        self.entries.push((key, handle.clone()));
        Ok(handle)
    }
    /// Remove a business field and clear its idle state, including external clones.
    /// Active entries cannot be removed. Dropping the cache itself only drops handles.
    pub fn remove(&mut self, key: &K) -> Result<bool, DraftCacheError> {
        let Some(position) = self
            .entries
            .iter()
            .position(|(candidate, _)| candidate == key)
        else {
            return Ok(false);
        };
        self.entries[position]
            .1
            .clear()
            .map_err(|_| DraftCacheError::DraftInUse)?;
        self.entries.remove(position);
        Ok(true)
    }
    /// O(entries + retained history records). Excludes keys/entry-array/Rc overhead.
    pub fn usage(&self) -> DraftCacheUsage {
        let mut usage = DraftCacheUsage {
            entries: self.entries.len(),
            ..Default::default()
        };
        for (_, draft) in &self.entries {
            match draft.memory_usage() {
                Ok(memory) => {
                    usage.idle_allocated_bytes = usage
                        .idle_allocated_bytes
                        .saturating_add(memory.allocated_bytes())
                }
                Err(_) => usage.active_entries += 1,
            }
        }
        usage
    }
    /// Explicit maintenance of idle payload only, oldest access first. First
    /// releases histories, then (only with DiscardIdleDrafts) clears full drafts.
    /// Active edits and later detachments can exceed this idle snapshot budget.
    pub fn maintain(
        &mut self,
        max_idle_bytes: usize,
        policy: DraftCachePolicy,
    ) -> DraftCacheReport {
        let before = self.usage();
        let mut remaining = before.idle_allocated_bytes;
        let mut history_released_bytes = 0usize;
        let mut discarded_drafts = 0;
        for (_, draft) in &self.entries {
            if remaining <= max_idle_bytes {
                break;
            }
            if let Ok(released) = draft.release_history() {
                remaining = remaining.saturating_sub(released);
                history_released_bytes = history_released_bytes.saturating_add(released);
            }
        }
        if policy == DraftCachePolicy::DiscardIdleDrafts {
            for (_, draft) in &self.entries {
                if remaining <= max_idle_bytes {
                    break;
                }
                if let Ok(memory) = draft.memory_usage() {
                    let bytes = memory.allocated_bytes();
                    if bytes > 0 && draft.clear().is_ok() {
                        remaining = remaining.saturating_sub(bytes);
                        discarded_drafts += 1;
                    }
                }
            }
        }
        let after = self.usage();
        DraftCacheReport {
            before,
            after,
            history_released_bytes,
            discarded_drafts,
            budget_met: after.idle_allocated_bytes <= max_idle_bytes,
        }
    }
}
