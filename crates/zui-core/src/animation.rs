use crate::{Tree, TreeError};
use core::time::Duration;
use zui_api::widget::AnimationUpdate;

impl Tree {
    /// Advances current widgets only, without retaining removed-node callbacks.
    /// Caller supplies monotonic elapsed time; this never reads a system clock.
    pub fn advance_animations(&mut self, elapsed: Duration) -> Result<AnimationUpdate, TreeError> {
        self.advance_animation_frame(elapsed, &mut alloc::vec::Vec::new(), false)
    }
    pub fn advance_animations_with_actions(
        &mut self,
        elapsed: Duration,
        actions: &mut alloc::vec::Vec<zui_api::action::ActionEvent>,
    ) -> Result<AnimationUpdate, TreeError> {
        self.advance_animation_frame(elapsed, actions, true)
    }
    fn advance_animation_frame(
        &mut self,
        elapsed: Duration,
        actions: &mut alloc::vec::Vec<zui_api::action::ActionEvent>,
        emit_actions: bool,
    ) -> Result<AnimationUpdate, TreeError> {
        let mut update = AnimationUpdate::default();
        let mut emitted = alloc::vec::Vec::new();
        for (&id, node) in &mut self.nodes {
            let mut pending = alloc::vec::Vec::new();
            let frame = if emit_actions {
                node.widget.animate_with_actions(elapsed, &mut pending)
            } else {
                node.widget.animate(elapsed)
            };
            emitted.extend(
                pending
                    .into_iter()
                    .map(|action| zui_api::action::ActionEvent { source: id, action }),
            );
            self.dirty = self.dirty.max(frame.invalidation);
            if frame.invalidation != zui_api::widget::Invalidation::None {
                self.presentation_cache.invalidate();
                self.paint_dirty.insert(id);
            }
            update.invalidation = update.invalidation.max(frame.invalidation);
            if let Some(deadline) = frame.next_frame {
                if deadline <= elapsed {
                    return Err(TreeError::InvalidAnimationDeadline(id));
                }
                update.next_frame =
                    Some(update.next_frame.map_or(deadline, |old| old.min(deadline)));
            }
        }
        actions.extend(emitted);
        Ok(update)
    }
}
