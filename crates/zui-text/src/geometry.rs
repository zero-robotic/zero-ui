//! Retains the exact shaped layout behind a portable, immutable query contract.
use parley::{Affinity, Cursor, Layout, Selection};
use unicode_segmentation::UnicodeSegmentation;
use zui_api::text::{
    TextAffinity, TextGeometry, TextGeometryError as Error, TextPosition, VisualDirection,
};
use zui_types::{Point, Rect};

pub(crate) struct ParleyGeometry {
    layout: Layout<()>,
    boundaries: Vec<usize>,
}
impl ParleyGeometry {
    pub(crate) fn new(layout: Layout<()>, text: &str) -> Self {
        let mut boundaries: Vec<_> = text.grapheme_indices(true).map(|(i, _)| i).collect();
        boundaries.push(text.len());
        Self { layout, boundaries }
    }
    fn cursor(&self, position: TextPosition) -> Result<Cursor, Error> {
        if self.boundaries.binary_search(&position.byte_index).is_err() {
            return Err(Error::InvalidPosition);
        }
        let cursor = Cursor::from_byte_index(
            &self.layout,
            position.byte_index,
            match position.affinity {
                TextAffinity::Upstream => Affinity::Upstream,
                TextAffinity::Downstream => Affinity::Downstream,
            },
        );
        if cursor.index() != position.byte_index {
            return Err(Error::InvalidPosition);
        }
        Ok(cursor)
    }
    fn position(cursor: Cursor) -> TextPosition {
        TextPosition {
            byte_index: cursor.index(),
            affinity: match cursor.affinity() {
                Affinity::Upstream => TextAffinity::Upstream,
                Affinity::Downstream => TextAffinity::Downstream,
            },
        }
    }
    fn rect(bounds: parley::BoundingBox) -> Result<Rect, Error> {
        let rect = Rect::new(
            bounds.x0,
            bounds.y0,
            bounds.x1 - bounds.x0,
            bounds.y1 - bounds.y0,
        );
        if rect.is_valid() {
            Ok(rect)
        } else {
            Err(Error::InvalidGeometry)
        }
    }
}
impl TextGeometry for ParleyGeometry {
    fn cursor_at(&self, point: Point) -> Result<TextPosition, Error> {
        if !point.is_finite() || point.x.abs() > f32::MAX as f64 || point.y.abs() > f32::MAX as f64
        {
            return Err(Error::InvalidPoint);
        }
        let cursor = Cursor::from_point(&self.layout, point.x as f32, point.y as f32);
        match self.boundaries.binary_search(&cursor.index()) {
            Ok(_) => Ok(Self::position(cursor)),
            Err(index) => {
                // A fallback-font run can divide one Unicode grapheme. Resolve
                // that partial hit to the closest full-grapheme caret, not a byte guess.
                let before = *self
                    .boundaries
                    .get(index.saturating_sub(1))
                    .ok_or(Error::InvalidGeometry)?;
                let after = *self.boundaries.get(index).ok_or(Error::InvalidGeometry)?;
                let candidates = [
                    TextPosition {
                        byte_index: before,
                        affinity: TextAffinity::Downstream,
                    },
                    TextPosition {
                        byte_index: after,
                        affinity: TextAffinity::Upstream,
                    },
                ];
                let mut best = None;
                for position in candidates {
                    let rect = self.caret(position, 0.0)?;
                    let dx = rect.origin.x - point.x;
                    let dy = (rect.origin.y + rect.size.height / 2.0) - point.y;
                    let distance = dx * dx + dy * dy;
                    if best.is_none_or(|(_, previous)| distance < previous) {
                        best = Some((position, distance));
                    }
                }
                Ok(best.ok_or(Error::InvalidGeometry)?.0)
            }
        }
    }
    fn caret(&self, position: TextPosition, width: f32) -> Result<Rect, Error> {
        if !width.is_finite() || width < 0.0 {
            return Err(Error::InvalidCaretWidth);
        }
        Self::rect(self.cursor(position)?.geometry(&self.layout, width))
    }
    fn selection_rects(
        &self,
        anchor: TextPosition,
        focus: TextPosition,
    ) -> Result<Vec<Rect>, Error> {
        let selection = Selection::new(self.cursor(anchor)?, self.cursor(focus)?);
        selection
            .geometry(&self.layout)
            .into_iter()
            .map(|(bounds, _)| Self::rect(bounds))
            .collect()
    }
    fn move_visual(
        &self,
        position: TextPosition,
        direction: VisualDirection,
    ) -> Result<TextPosition, Error> {
        let mut cursor = self.cursor(position)?;
        // At most two affinities per byte. Bound iteration even if an upstream
        // implementation unexpectedly cycles while crossing split graphemes.
        let limit = self
            .boundaries
            .last()
            .copied()
            .unwrap_or(0)
            .saturating_mul(2)
            .saturating_add(2);
        for _ in 0..limit {
            let next = match direction {
                VisualDirection::Left => cursor.previous_visual(&self.layout),
                VisualDirection::Right => cursor.next_visual(&self.layout),
            };
            if self.boundaries.binary_search(&next.index()).is_ok() {
                return Ok(Self::position(next));
            }
            if next == cursor {
                return Err(Error::InvalidGeometry);
            }
            cursor = next;
        }
        Err(Error::InvalidGeometry)
    }
}
