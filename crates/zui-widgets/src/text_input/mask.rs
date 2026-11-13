use alloc::{string::String, vec::Vec};
/// Bidirectional mapping between original grapheme boundaries and bullet bytes.
pub(super) struct Mask {
    pub text: String,
    offsets: Vec<usize>,
}
impl Mask {
    pub fn new(text: &str) -> Self {
        let offsets: Vec<_> = crate::edit::grapheme_boundaries(text).collect();
        let text = "•".repeat(offsets.len().saturating_sub(1));
        Self { text, offsets }
    }
    pub fn display(&self, original: usize) -> usize {
        self.offsets
            .partition_point(|i| *i <= original)
            .saturating_sub(1)
            * "•".len()
    }
    pub fn original(&self, display: usize) -> usize {
        self.offsets[(display / "•".len()).min(self.offsets.len() - 1)]
    }
}
