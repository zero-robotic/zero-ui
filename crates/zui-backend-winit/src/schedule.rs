use std::time::Duration;
use zui_platform::WindowAction;

/// Tracks invalidation separately from OS redraw requests, preserving work while
/// minimized and preventing duplicate requests during an event burst.
#[derive(Default)]
pub(crate) struct Schedule {
    dirty: bool,
    queued: bool,
    deadline: Option<Duration>,
}

impl Schedule {
    pub fn apply(&mut self, action: WindowAction) {
        self.dirty |= action.redraw;
        if let Some(deadline) = action.wake_at {
            self.deadline = Some(self.deadline.map_or(deadline, |old| old.min(deadline)));
        }
    }
    pub fn deadline(&self) -> Option<Duration> {
        self.deadline
    }
    pub fn take_wake(&mut self, now: Duration) -> bool {
        if self.deadline.is_some_and(|deadline| deadline <= now) {
            self.deadline = None;
            true
        } else {
            false
        }
    }
    pub fn request_redraw(&mut self, drawable: bool) -> bool {
        if drawable && self.dirty && !self.queued {
            self.queued = true;
            true
        } else {
            false
        }
    }
    pub fn begin_redraw(&mut self, drawable: bool) -> bool {
        self.queued = false;
        if drawable {
            self.dirty = false;
            true
        } else {
            self.dirty = true;
            false
        }
    }
    pub fn suspended(&mut self) {
        self.queued = false;
        self.dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redraws_coalesce_and_idle_does_not_poll() {
        let mut state = Schedule::default();
        state.apply(WindowAction::redraw());
        assert!(state.request_redraw(true));
        state.apply(WindowAction::redraw());
        assert!(!state.request_redraw(true));
        assert!(state.begin_redraw(true));
        assert!(!state.request_redraw(true));
        assert_eq!(state.deadline(), None);
    }

    #[test]
    fn hidden_damage_survives_until_visible() {
        let mut state = Schedule::default();
        state.apply(WindowAction::redraw());
        assert!(!state.request_redraw(false));
        assert!(!state.begin_redraw(false));
        assert!(state.request_redraw(true));
        state.suspended();
        assert!(state.request_redraw(true));
    }

    #[test]
    fn deadlines_are_not_cancelled_by_unrelated_events() {
        let mut state = Schedule::default();
        state.apply(WindowAction {
            wake_at: Some(Duration::from_secs(2)),
            ..Default::default()
        });
        state.apply(WindowAction {
            wake_at: Some(Duration::from_secs(4)),
            ..Default::default()
        });
        state.apply(WindowAction::redraw());
        assert!(!state.take_wake(Duration::from_secs(1)));
        assert!(state.take_wake(Duration::from_secs(2)));
        assert!(!state.take_wake(Duration::from_secs(2)));
    }
}
