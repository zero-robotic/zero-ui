use crate::{Tree, TreeError};
use core::time::Duration;
use zui_api::widget::AnimationUpdate;

impl Tree {
    /// Advances current widgets only, without retaining removed-node callbacks.
    /// Caller supplies monotonic elapsed time; this never reads a system clock.
    pub fn advance_animations(&mut self, elapsed: Duration) -> Result<AnimationUpdate, TreeError> {
        let mut update = AnimationUpdate::default();
        for (&id, node) in &mut self.nodes {
            let frame = node.widget.animate(elapsed);
            self.dirty = self.dirty.max(frame.invalidation);
            update.invalidation = update.invalidation.max(frame.invalidation);
            if let Some(deadline) = frame.next_frame {
                if deadline <= elapsed {
                    return Err(TreeError::InvalidAnimationDeadline(id));
                }
                update.next_frame =
                    Some(update.next_frame.map_or(deadline, |old| old.min(deadline)));
            }
        }
        Ok(update)
    }
}
