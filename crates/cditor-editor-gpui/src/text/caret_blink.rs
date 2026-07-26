use std::time::{Duration, Instant};

use gpui::Context;

const CARET_BLINK_INTERVAL: Duration = Duration::from_millis(500);

pub(crate) struct CaretBlink {
    generation: usize,
    enabled: bool,
    paused_until: Option<Instant>,
    visible: bool,
    timer_running: bool,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct TimerTransition {
    visual_changed: bool,
    next_delay: Option<Duration>,
}

impl CaretBlink {
    pub(crate) const fn new() -> Self {
        Self {
            generation: 0,
            enabled: false,
            paused_until: None,
            visible: true,
            timer_running: false,
        }
    }

    pub(crate) const fn visible(&self) -> bool {
        self.visible
    }

    pub(crate) fn set_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        if enabled == self.enabled {
            return;
        }

        self.generation = self.generation.wrapping_add(1);
        self.enabled = enabled;
        self.paused_until = None;
        self.visible = enabled;
        // A timer from the previous generation may still wake, but it no
        // longer owns this state and must not block the replacement timer.
        self.timer_running = false;
        cx.notify();

        if enabled {
            self.schedule_timer(CARET_BLINK_INTERVAL, cx);
        }
    }

    pub(crate) fn pause(&mut self, cx: &mut Context<Self>) {
        let transition = self.pause_at(Instant::now());
        if transition.visual_changed {
            cx.notify();
        }
        if let Some(delay) = transition.next_delay {
            self.schedule_timer(delay, cx);
        }
    }

    fn pause_at(&mut self, now: Instant) -> TimerTransition {
        if !self.enabled {
            return TimerTransition::default();
        }
        self.paused_until = Some(now + CARET_BLINK_INTERVAL);
        let visual_changed = !self.visible;
        self.visible = true;
        TimerTransition {
            visual_changed,
            next_delay: (!self.timer_running).then_some(CARET_BLINK_INTERVAL),
        }
    }

    fn schedule_timer(&mut self, delay: Duration, cx: &mut Context<Self>) {
        if !self.enabled || self.timer_running {
            return;
        }
        self.timer_running = true;
        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(delay).await;
            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| this.timer_elapsed(generation, cx));
            }
        })
        .detach();
    }

    fn timer_elapsed(&mut self, generation: usize, cx: &mut Context<Self>) {
        let transition = self.timer_transition(generation, Instant::now());
        if transition.visual_changed {
            cx.notify();
        }
        if let Some(delay) = transition.next_delay {
            self.schedule_timer(delay, cx);
        }
    }

    fn timer_transition(&mut self, generation: usize, now: Instant) -> TimerTransition {
        if generation != self.generation || !self.enabled {
            return TimerTransition::default();
        }
        self.timer_running = false;
        if let Some(paused_until) = self.paused_until {
            if now < paused_until {
                return TimerTransition {
                    visual_changed: false,
                    next_delay: Some(paused_until.duration_since(now)),
                };
            }
            self.paused_until = None;
            return TimerTransition {
                visual_changed: false,
                next_delay: Some(CARET_BLINK_INTERVAL),
            };
        }

        self.visible = !self.visible;
        TimerTransition {
            visual_changed: true,
            next_delay: Some(CARET_BLINK_INTERVAL),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_timer_cannot_toggle_a_restarted_caret() {
        let mut blink = CaretBlink::new();
        blink.enabled = true;
        blink.generation = 7;
        blink.timer_running = true;

        let transition = blink.timer_transition(6, Instant::now());

        assert_eq!(transition, TimerTransition::default());
        assert!(blink.visible);
        assert!(blink.timer_running);
        assert_eq!(CARET_BLINK_INTERVAL, Duration::from_millis(500));
    }

    #[test]
    fn repeated_pause_extends_one_timer_instead_of_spawning_per_input() {
        let now = Instant::now();
        let mut blink = CaretBlink::new();
        blink.enabled = true;
        blink.visible = false;
        blink.timer_running = true;

        let first = blink.pause_at(now);
        let second = blink.pause_at(now + Duration::from_millis(100));
        let early_wake = blink.timer_transition(0, now + CARET_BLINK_INTERVAL);

        assert!(first.visual_changed);
        assert_eq!(first.next_delay, None);
        assert!(!second.visual_changed);
        assert_eq!(second.next_delay, None);
        assert_eq!(early_wake.next_delay, Some(Duration::from_millis(100)));
        assert!(blink.visible);
    }

    #[test]
    fn caret_stays_visible_for_a_full_interval_after_input_stops() {
        let now = Instant::now();
        let mut blink = CaretBlink::new();
        blink.enabled = true;
        blink.timer_running = true;
        blink.pause_at(now);

        let resume = blink.timer_transition(0, now + CARET_BLINK_INTERVAL);
        blink.timer_running = true;
        let blink_after_resume =
            blink.timer_transition(0, now + CARET_BLINK_INTERVAL + CARET_BLINK_INTERVAL);

        assert_eq!(resume.next_delay, Some(CARET_BLINK_INTERVAL));
        assert!(blink_after_resume.visual_changed);
        assert!(!blink.visible);
    }
}
