use std::time::{Duration, Instant};

pub const POSTGRES_VIEWPORT_LOAD_DEBOUNCE: Duration = Duration::from_millis(75);
pub const POSTGRES_VIEWPORT_LOAD_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PayloadWindowLoadSchedule {
    DispatchNow,
    WakeAfter(Duration),
    WakeAlreadyScheduled,
}

#[derive(Debug, Default)]
pub(crate) struct PayloadWindowLoadScheduler {
    last_dispatched_at: Option<Instant>,
    wake_scheduled: bool,
}

impl PayloadWindowLoadScheduler {
    pub(crate) fn request(&mut self, now: Instant) -> PayloadWindowLoadSchedule {
        let Some(last_dispatched_at) = self.last_dispatched_at else {
            self.last_dispatched_at = Some(now);
            return PayloadWindowLoadSchedule::DispatchNow;
        };
        let elapsed = now.saturating_duration_since(last_dispatched_at);
        if elapsed >= POSTGRES_VIEWPORT_LOAD_DEBOUNCE {
            self.last_dispatched_at = Some(now);
            self.wake_scheduled = false;
            return PayloadWindowLoadSchedule::DispatchNow;
        }
        if self.wake_scheduled {
            return PayloadWindowLoadSchedule::WakeAlreadyScheduled;
        }
        self.wake_scheduled = true;
        PayloadWindowLoadSchedule::WakeAfter(POSTGRES_VIEWPORT_LOAD_DEBOUNCE - elapsed)
    }

    pub(crate) fn wake(&mut self) {
        self.wake_scheduled = false;
    }

    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewport_loads_immediately_then_coalesces_fast_scroll_events() {
        let mut scheduler = PayloadWindowLoadScheduler::default();
        let start = Instant::now();

        assert_eq!(
            scheduler.request(start),
            PayloadWindowLoadSchedule::DispatchNow
        );
        assert_eq!(
            scheduler.request(start + Duration::from_millis(25)),
            PayloadWindowLoadSchedule::WakeAfter(Duration::from_millis(50))
        );
        assert_eq!(
            scheduler.request(start + Duration::from_millis(30)),
            PayloadWindowLoadSchedule::WakeAlreadyScheduled
        );

        scheduler.wake();
        assert_eq!(
            scheduler.request(start + POSTGRES_VIEWPORT_LOAD_DEBOUNCE),
            PayloadWindowLoadSchedule::DispatchNow
        );
        assert!(POSTGRES_VIEWPORT_LOAD_TIMEOUT > POSTGRES_VIEWPORT_LOAD_DEBOUNCE);
    }
}
