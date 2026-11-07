//! Exclusive committed-state handoff across widget recycling. No runtime IDs.
use super::*;
use alloc::rc::Rc;
use core::{cell::RefCell, fmt};

/// Application-owned editing state. Clone the handle for descriptions of the same
/// business field; never attach it to two live editors. Drop/clear idle handles to
/// bound application cache memory. No focus, IME preedit or shaped layout is saved.
#[derive(Clone)]
pub struct TextInputDraft(Rc<RefCell<Slot>>);

/// Owned payload capacities only; excludes the Rc/slot allocation, allocator
/// metadata, model copies, layout and renderer resources. Count shared handles once.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextInputDraftMemory {
    pub edit: Option<zui_edit::EditMemoryUsage>,
    pub external_text_capacity_bytes: usize,
}
impl TextInputDraftMemory {
    pub fn allocated_bytes(self) -> usize {
        self.external_text_capacity_bytes.saturating_add(
            self.edit
                .map_or(0, zui_edit::EditMemoryUsage::allocated_bytes),
        )
    }
}
enum Slot {
    Idle(Option<Saved>),
    Attached,
}
struct Saved {
    buffer: Result<TextBuffer, TextInputError>,
    external_text: String,
    revision: u64,
    limits: EditLimits,
    anchor_affinity: TextAffinity,
    focus_affinity: TextAffinity,
    scroll: f64,
}
impl Default for TextInputDraft {
    fn default() -> Self {
        Self(Rc::new(RefCell::new(Slot::Idle(None))))
    }
}
impl PartialEq for TextInputDraft {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for TextInputDraft {}
impl fmt::Debug for TextInputDraft {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TextInputDraft")
            .field("attached", &self.is_attached())
            .finish_non_exhaustive()
    }
}
impl TextInputDraft {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn is_attached(&self) -> bool {
        matches!(*self.0.borrow(), Slot::Attached)
    }
    /// Query idle retained payload. Attached is an explicit error, never zero use.
    pub fn memory_usage(&self) -> Result<TextInputDraftMemory, TextInputError> {
        match &*self.0.borrow() {
            Slot::Attached => Err(TextInputError::DraftInUse),
            Slot::Idle(None) => Ok(TextInputDraftMemory::default()),
            Slot::Idle(Some(saved)) => Ok(TextInputDraftMemory {
                edit: saved.buffer.as_ref().ok().map(TextBuffer::memory_usage),
                external_text_capacity_bytes: saved.external_text.capacity(),
            }),
        }
    }
    /// Explicit idle maintenance: lose undo/redo, preserve committed text/selection.
    pub fn release_history(&self) -> Result<usize, TextInputError> {
        match &mut *self.0.borrow_mut() {
            Slot::Attached => Err(TextInputError::DraftInUse),
            Slot::Idle(None) => Ok(0),
            Slot::Idle(Some(saved)) => Ok(saved
                .buffer
                .as_mut()
                .ok()
                .map_or(0, TextBuffer::release_history)),
        }
    }
    /// Discard idle state; live editors must first be unmounted or rebound.
    pub fn clear(&self) -> Result<(), TextInputError> {
        let mut slot = self.0.borrow_mut();
        if matches!(*slot, Slot::Attached) {
            return Err(TextInputError::DraftInUse);
        }
        *slot = Slot::Idle(None);
        Ok(())
    }
}
impl TextInput {
    /// Active committed editing payload; excludes prepared text and paint caches.
    pub fn editing_memory_usage(&self) -> TextInputDraftMemory {
        TextInputDraftMemory {
            edit: self.buffer.as_ref().ok().map(TextBuffer::memory_usage),
            external_text_capacity_bytes: self.properties.text.capacity(),
        }
    }
    pub(super) fn attach_draft(&mut self) {
        let Some(handle) = &self.properties.draft else {
            return;
        };
        let saved = {
            let mut slot = handle.0.borrow_mut();
            if matches!(*slot, Slot::Attached) {
                self.buffer = Err(TextInputError::DraftInUse);
                return;
            }
            let Slot::Idle(saved) = core::mem::replace(&mut *slot, Slot::Attached) else {
                unreachable!()
            };
            saved
        };
        self.draft_attached = true;
        if let Some(saved) = saved {
            let forced = saved.revision != self.properties.revision
                || saved.limits != self.properties.limits;
            let replacement = saved.external_text != self.properties.text
                && saved.buffer.as_ref().ok().map(TextBuffer::text)
                    != Some(self.properties.text.as_str());
            if !forced && !replacement {
                self.buffer = saved.buffer;
                self.anchor_affinity = saved.anchor_affinity;
                self.focus_affinity = saved.focus_affinity;
                self.scroll = saved.scroll;
            }
        }
    }
    pub(super) fn release_draft(&mut self) {
        if !self.draft_attached {
            return;
        }
        let handle = self.properties.draft.as_ref().expect("attached draft");
        let saved = Saved {
            buffer: core::mem::replace(&mut self.buffer, Err(TextInputError::DraftInUse)),
            external_text: core::mem::take(&mut self.properties.text),
            revision: self.properties.revision,
            limits: self.properties.limits,
            anchor_affinity: self.anchor_affinity,
            focus_affinity: self.focus_affinity,
            scroll: self.scroll,
        };
        *handle.0.borrow_mut() = Slot::Idle(Some(saved));
        self.draft_attached = false;
    }
}
impl Drop for TextInput {
    fn drop(&mut self) {
        self.release_draft();
    }
}
