use std::time::Duration;
use zui_platform::WindowAction;

/// Multiplexes independent owners onto the platform's single wake deadline.
#[derive(Default)]
pub(crate) struct WakeSchedule {
    application: Option<Duration>,
    retry: Option<Duration>,
    animation: Option<Duration>,
}

impl WakeSchedule {
    pub fn animation_at(&mut self, deadline: Option<Duration>) {
        self.animation = deadline;
    }
    pub fn application_action(&mut self, mut action: WindowAction) -> WindowAction {
        self.application = earliest(self.application, action.wake_at);
        action.wake_at = self.next();
        action
    }

    #[cfg(any(feature = "gpu", test))]
    pub fn retry_at(&mut self, deadline: Duration) {
        self.retry = Some(deadline);
    }

    pub fn presented(&mut self) {
        self.retry = None;
    }

    pub fn next(&self) -> Option<Duration> {
        earliest(earliest(self.application, self.retry), self.animation)
    }

    /// Returns (application wake, presentation retry, animation). Future work stays pending.
    pub fn take_due(&mut self, now: Duration) -> (bool, bool, bool) {
        (
            take_due(&mut self.application, now),
            take_due(&mut self.retry, now),
            take_due(&mut self.animation, now),
        )
    }
}

fn earliest(a: Option<Duration>, b: Option<Duration>) -> Option<Duration> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

fn take_due(deadline: &mut Option<Duration>, now: Duration) -> bool {
    if deadline.is_some_and(|value| value <= now) {
        *deadline = None;
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_does_not_consume_application_timer() {
        let mut schedule = WakeSchedule::default();
        let timer = Duration::from_secs(2);
        let retry = Duration::from_millis(16);
        schedule.application_action(WindowAction {
            wake_at: Some(timer),
            ..Default::default()
        });
        schedule.retry_at(retry);
        assert_eq!(schedule.next(), Some(retry));
        assert_eq!(schedule.take_due(retry), (false, true, false));
        schedule.presented();
        assert_eq!(schedule.next(), Some(timer));
        assert_eq!(schedule.take_due(timer), (true, false, false));
        assert_eq!(schedule.next(), None);
    }

    #[test]
    fn application_timer_does_not_consume_later_retry() {
        let mut schedule = WakeSchedule::default();
        schedule.application_action(WindowAction {
            wake_at: Some(Duration::from_millis(5)),
            ..Default::default()
        });
        schedule.retry_at(Duration::from_millis(16));
        assert_eq!(
            schedule.take_due(Duration::from_millis(5)),
            (true, false, false)
        );
        let action = schedule.application_action(WindowAction::redraw());
        assert!(action.redraw);
        assert_eq!(action.wake_at, Some(Duration::from_millis(16)));
        assert_eq!(
            schedule.take_due(Duration::from_millis(16)),
            (false, true, false)
        );
    }
}

#[cfg(test)]
#[test]
fn animation_rescheduling_and_cancellation_preserve_other_deadline_owners() {
    let mut schedule = WakeSchedule::default();
    schedule.application_action(WindowAction {
        wake_at: Some(Duration::from_secs(3)),
        ..Default::default()
    });
    schedule.retry_at(Duration::from_millis(16));
    schedule.animation_at(Some(Duration::from_millis(500)));
    assert_eq!(
        schedule.take_due(Duration::from_millis(16)),
        (false, true, false)
    );
    schedule.presented();
    assert_eq!(schedule.next(), Some(Duration::from_millis(500)));
    schedule.animation_at(Some(Duration::from_secs(1)));
    assert_eq!(
        schedule.take_due(Duration::from_millis(500)),
        (false, false, false)
    );
    assert_eq!(
        schedule.take_due(Duration::from_secs(1)),
        (false, false, true)
    );
    schedule.animation_at(Some(Duration::from_secs(2)));
    schedule.animation_at(None);
    assert_eq!(schedule.next(), Some(Duration::from_secs(3)));
    assert_eq!(
        schedule.take_due(Duration::from_secs(3)),
        (true, false, false)
    );
    assert_eq!(schedule.next(), None);
}
