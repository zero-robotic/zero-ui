//! Caret phase uses injected elapsed time, never a platform clock or timer.
use core::time::Duration;
use zui_api::widget::{AnimationUpdate, Invalidation};

pub(super) struct CaretBlink {
    start: Option<Duration>,
    pub visible: bool,
}
impl Default for CaretBlink {
    fn default() -> Self {
        Self {
            start: None,
            visible: true,
        }
    }
}
impl CaretBlink {
    pub fn reset(&mut self) -> bool {
        self.start = None;
        let changed = !self.visible;
        self.visible = true;
        changed
    }
    pub fn advance(
        &mut self,
        now: Duration,
        interval: Option<Duration>,
        active: bool,
    ) -> AnimationUpdate {
        let previous = self.visible;
        let next_frame =
            if let Some(interval) = interval.filter(|interval| !interval.is_zero() && active) {
                let start = *self.start.get_or_insert(now);
                let elapsed = now.saturating_sub(start).as_nanos();
                let period = interval.as_nanos();
                self.visible = (elapsed / period).is_multiple_of(2);
                let remainder = elapsed % period;
                let remainder = Duration::new(
                    (remainder / 1_000_000_000) as u64,
                    (remainder % 1_000_000_000) as u32,
                );
                now.checked_add(interval - remainder)
            } else {
                self.reset();
                None
            };
        AnimationUpdate {
            invalidation: if previous != self.visible {
                Invalidation::Paint
            } else {
                Invalidation::None
            },
            next_frame,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ms(value: u64) -> Duration {
        Duration::from_millis(value)
    }
    #[test]
    fn phase_skips_missed_frames_resets_on_input_and_stops_when_inactive() {
        let mut blink = CaretBlink::default();
        assert_eq!(
            blink.advance(ms(0), Some(ms(500)), true).next_frame,
            Some(ms(500))
        );
        assert!(blink.visible);
        assert_eq!(
            blink.advance(ms(1700), Some(ms(500)), true),
            AnimationUpdate {
                invalidation: Invalidation::Paint,
                next_frame: Some(ms(2000))
            }
        );
        assert!(!blink.visible);
        assert_eq!(
            blink.advance(ms(1900), Some(ms(500)), true).invalidation,
            Invalidation::None
        );
        assert!(blink.reset());
        assert_eq!(
            blink.advance(ms(1900), Some(ms(500)), true).next_frame,
            Some(ms(2400))
        );
        assert!(blink.visible);
        assert_eq!(
            blink.advance(ms(2000), Some(ms(500)), false).next_frame,
            None
        );
        assert_eq!(blink.advance(ms(2000), None, true).next_frame, None);
        assert_eq!(
            blink
                .advance(ms(2000), Some(Duration::ZERO), true)
                .next_frame,
            None
        );
        blink.reset();
        assert_eq!(
            blink.advance(Duration::MAX, Some(ms(500)), true).next_frame,
            None
        );
    }
}
