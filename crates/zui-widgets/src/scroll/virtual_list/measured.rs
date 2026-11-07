//! Observations belong to the committed row tag, never the new desired window.
use super::*;

impl VirtualList {
    pub(super) fn record_measurements(
        &mut self,
        context: PaintContext,
        children: &[ChildLayout],
    ) -> Result<Invalidation, LayoutError> {
        if !self.properties.sizes.measured() {
            return Ok(Invalidation::None);
        }
        let index = self
            .index
            .as_mut()
            .ok_or(LayoutError::InvalidStyle(context.id))?;
        let old_offset = self.scroll.offset.y;
        let anchor = if self.properties.preserve_anchor {
            index
                .anchor(old_offset, self.scroll.page.height)
                .map_err(|_| LayoutError::InvalidStyle(context.id))?
        } else {
            None
        };
        let mut changed = false;
        let mut first = None;
        let mut jump_measured = false;
        for child in children {
            let Some(tag) = child
                .tag
                .filter(|tag| tag.revision == self.measurement_generation)
            else {
                continue;
            };
            let row =
                usize::try_from(tag.value).map_err(|_| LayoutError::InvalidStyle(context.id))?;
            if row >= index.len() {
                return Err(LayoutError::InvalidStyle(context.id));
            }
            first.get_or_insert(row);
            let height = child.layout.bounds.size.height;
            match index.update(row, height) {
                Ok(update) => changed |= update,
                Err(_) => {
                    // Earlier updates may have succeeded. Rebuild from estimates
                    // before retrying, and reject old tags from this failed pass.
                    self.index_dirty = true;
                    return Err(LayoutError::InvalidMeasurement(context.id));
                }
            }
            jump_measured |= self
                .measurement_jump
                .is_some_and(|request| request.index == row);
        }
        let max = (index.total_extent() - self.scroll.page.height).max(0.0);
        let offset = if jump_measured {
            let request = self.measurement_jump.expect("measured target");
            index.offset_for(
                request.index,
                old_offset,
                self.scroll.page.height,
                request.alignment,
            )
        } else if let Some(anchor) = anchor {
            index.offset_for_anchor(anchor, self.scroll.page.height)
        } else {
            Ok(old_offset.min(max))
        }
        .map_err(|_| LayoutError::InvalidStyle(context.id))?;
        if jump_measured {
            self.measurement_jump = None;
        }
        self.scroll.max_offset.y = max;
        self.scroll.offset.y = offset;
        if let Some(first) = first {
            self.origin = index
                .item(first)
                .map_err(|_| LayoutError::InvalidStyle(context.id))?
                .start;
        }
        Ok(if changed || offset != old_offset {
            Invalidation::Paint
        } else {
            Invalidation::None
        })
    }
}
