use std::ops::Range;

use crate::scroll::{ScrollAnchor, ScrollInput, ScrollOrigin};
use cditor_core::layout::HeightChange;

#[derive(Debug, Clone)]
pub struct ScrollTraceFrame {
    pub frame_id: u64,
    pub input: Option<TraceInput>,
    pub global_scroll_top_before: f64,
    pub global_scroll_top_after: f64,
    pub anchor: Option<ScrollAnchor>,
    pub window_range: Range<usize>,
    pub height_changes: Vec<HeightChange>,
    pub correction_applied: f64,
    pub model_total_height: f64,
    pub displayed_total_height: f64,
    pub frame_cost_ms: f64,
    pub anchor_jitter_px: f64,
    pub caret_jitter_px: f64,
    pub window_commit_count: usize,
    pub thumb_reverse_jump: bool,
}

#[derive(Debug, Clone)]
pub enum TraceInput {
    Wheel(ScrollInput),
    ScrollbarDrag {
        position_ratio: f64,
        timestamp: u64,
    },
    Typing {
        text: String,
        timestamp: u64,
    },
    Ime {
        phase: String,
        timestamp: u64,
    },
    Resize {
        width: f64,
        height: f64,
        timestamp: u64,
    },
    AsyncHeightCorrection {
        timestamp: u64,
    },
    WindowCommit {
        timestamp: u64,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScrollTraceReplayReport {
    pub frame_count: usize,
    pub thumb_reverse_jump_count: usize,
    pub monotonicity_violations: usize,
    pub anchor_jitter_p95: f64,
    pub anchor_jitter_p99: f64,
    pub caret_jitter_p95: f64,
    pub caret_jitter_p99: f64,
    pub max_height_corrections_per_frame: usize,
    pub max_window_commit_count: usize,
    pub max_frame_cost_ms: f64,
    pub total_height_correction_px: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RegressionGateConfig {
    pub anchor_jitter_p95_max: f64,
    pub anchor_jitter_p99_max: f64,
    pub caret_jitter_p95_max: f64,
    pub caret_jitter_p99_max: f64,
    pub max_height_corrections_per_frame: usize,
    pub max_window_commit_count: usize,
    pub max_frame_cost_ms: f64,
}

impl Default for RegressionGateConfig {
    fn default() -> Self {
        Self {
            anchor_jitter_p95_max: 1.0,
            anchor_jitter_p99_max: 1.5,
            caret_jitter_p95_max: 1.0,
            caret_jitter_p99_max: 1.5,
            max_height_corrections_per_frame: 8,
            max_window_commit_count: 2,
            max_frame_cost_ms: 16.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RegressionGateResult {
    pub passed: bool,
    pub failures: Vec<String>,
    pub report: ScrollTraceReplayReport,
}

#[derive(Debug, Clone, Default)]
pub struct ScrollTraceReplay {
    frames: Vec<ScrollTraceFrame>,
}

impl ScrollTraceReplay {
    pub fn new(frames: Vec<ScrollTraceFrame>) -> Self {
        Self { frames }
    }

    pub fn frames(&self) -> &[ScrollTraceFrame] {
        &self.frames
    }

    pub fn replay(&self) -> ScrollTraceReplayReport {
        let mut thumb_reverse_jump_count = 0;
        let mut monotonicity_violations = 0;
        let mut anchor_jitters = Vec::with_capacity(self.frames.len());
        let mut caret_jitters = Vec::with_capacity(self.frames.len());
        let mut max_height_corrections_per_frame = 0;
        let mut max_window_commit_count = 0;
        let mut max_frame_cost_ms = 0.0_f64;
        let mut total_height_correction_px = 0.0_f64;

        for frame in &self.frames {
            if frame.thumb_reverse_jump {
                thumb_reverse_jump_count += 1;
            }
            if violates_directional_monotonicity(frame) {
                monotonicity_violations += 1;
            }
            anchor_jitters.push(frame.anchor_jitter_px.abs());
            caret_jitters.push(frame.caret_jitter_px.abs());
            max_height_corrections_per_frame =
                max_height_corrections_per_frame.max(frame.height_changes.len());
            max_window_commit_count = max_window_commit_count.max(frame.window_commit_count);
            max_frame_cost_ms = max_frame_cost_ms.max(frame.frame_cost_ms);
            total_height_correction_px += frame.correction_applied.abs();
        }

        ScrollTraceReplayReport {
            frame_count: self.frames.len(),
            thumb_reverse_jump_count,
            monotonicity_violations,
            anchor_jitter_p95: percentile(anchor_jitters.clone(), 0.95),
            anchor_jitter_p99: percentile(anchor_jitters, 0.99),
            caret_jitter_p95: percentile(caret_jitters.clone(), 0.95),
            caret_jitter_p99: percentile(caret_jitters, 0.99),
            max_height_corrections_per_frame,
            max_window_commit_count,
            max_frame_cost_ms,
            total_height_correction_px,
        }
    }

    pub fn check_regression_gate(&self, config: RegressionGateConfig) -> RegressionGateResult {
        let report = self.replay();
        let mut failures = Vec::new();

        if report.thumb_reverse_jump_count != 0 {
            failures.push(format!(
                "thumb reverse-jump count must be 0, got {}",
                report.thumb_reverse_jump_count
            ));
        }
        if report.monotonicity_violations != 0 {
            failures.push(format!(
                "global_scroll_top monotonicity violations: {}",
                report.monotonicity_violations
            ));
        }
        if report.anchor_jitter_p95 > config.anchor_jitter_p95_max {
            failures.push(format!(
                "anchor jitter p95 {:.2}px exceeds {:.2}px",
                report.anchor_jitter_p95, config.anchor_jitter_p95_max
            ));
        }
        if report.anchor_jitter_p99 > config.anchor_jitter_p99_max {
            failures.push(format!(
                "anchor jitter p99 {:.2}px exceeds {:.2}px",
                report.anchor_jitter_p99, config.anchor_jitter_p99_max
            ));
        }
        if report.caret_jitter_p95 > config.caret_jitter_p95_max {
            failures.push(format!(
                "caret jitter p95 {:.2}px exceeds {:.2}px",
                report.caret_jitter_p95, config.caret_jitter_p95_max
            ));
        }
        if report.caret_jitter_p99 > config.caret_jitter_p99_max {
            failures.push(format!(
                "caret jitter p99 {:.2}px exceeds {:.2}px",
                report.caret_jitter_p99, config.caret_jitter_p99_max
            ));
        }
        if report.max_height_corrections_per_frame > config.max_height_corrections_per_frame {
            failures.push(format!(
                "height corrections per frame {} exceeds {}",
                report.max_height_corrections_per_frame, config.max_height_corrections_per_frame
            ));
        }
        if report.max_window_commit_count > config.max_window_commit_count {
            failures.push(format!(
                "window commit count {} exceeds {}",
                report.max_window_commit_count, config.max_window_commit_count
            ));
        }
        if report.max_frame_cost_ms > config.max_frame_cost_ms {
            failures.push(format!(
                "frame cost {:.2}ms exceeds {:.2}ms",
                report.max_frame_cost_ms, config.max_frame_cost_ms
            ));
        }

        RegressionGateResult {
            passed: failures.is_empty(),
            failures,
            report,
        }
    }
}

impl ScrollTraceFrame {
    pub fn new(
        frame_id: u64,
        input: Option<TraceInput>,
        before: f64,
        after: f64,
        window_range: Range<usize>,
    ) -> Self {
        Self {
            frame_id,
            input,
            global_scroll_top_before: before,
            global_scroll_top_after: after,
            anchor: None,
            window_range,
            height_changes: Vec::new(),
            correction_applied: 0.0,
            model_total_height: 0.0,
            displayed_total_height: 0.0,
            frame_cost_ms: 0.0,
            anchor_jitter_px: 0.0,
            caret_jitter_px: 0.0,
            window_commit_count: 0,
            thumb_reverse_jump: false,
        }
    }

    pub fn with_anchor(mut self, anchor: ScrollAnchor, jitter_px: f64) -> Self {
        self.anchor = Some(anchor);
        self.anchor_jitter_px = jitter_px.abs();
        self
    }

    pub fn with_height_changes(
        mut self,
        height_changes: Vec<HeightChange>,
        correction_applied: f64,
    ) -> Self {
        self.height_changes = height_changes;
        self.correction_applied = correction_applied;
        self
    }

    pub fn with_total_heights(
        mut self,
        model_total_height: f64,
        displayed_total_height: f64,
    ) -> Self {
        self.model_total_height = model_total_height;
        self.displayed_total_height = displayed_total_height;
        self
    }

    pub fn with_frame_cost(mut self, frame_cost_ms: f64) -> Self {
        self.frame_cost_ms = frame_cost_ms;
        self
    }

    pub fn with_caret_jitter(mut self, caret_jitter_px: f64) -> Self {
        self.caret_jitter_px = caret_jitter_px.abs();
        self
    }

    pub fn with_window_commit_count(mut self, window_commit_count: usize) -> Self {
        self.window_commit_count = window_commit_count;
        self
    }

    pub fn with_thumb_reverse_jump(mut self, thumb_reverse_jump: bool) -> Self {
        self.thumb_reverse_jump = thumb_reverse_jump;
        self
    }
}

fn violates_directional_monotonicity(frame: &ScrollTraceFrame) -> bool {
    let delta = frame.global_scroll_top_after - frame.global_scroll_top_before;
    match &frame.input {
        Some(TraceInput::Wheel(input)) => match input.origin() {
            ScrollOrigin::UserWheel if input.delta_y > 0.0 => delta < -0.5,
            ScrollOrigin::UserWheel if input.delta_y < 0.0 => delta > 0.5,
            _ => false,
        },
        Some(TraceInput::ScrollbarDrag { .. }) => frame.thumb_reverse_jump,
        _ => false,
    }
}

trait ScrollInputOriginExt {
    fn origin(&self) -> ScrollOrigin;
}

impl ScrollInputOriginExt for ScrollInput {
    fn origin(&self) -> ScrollOrigin {
        ScrollOrigin::UserWheel
    }
}

fn percentile(mut values: Vec<f64>, p: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|left, right| left.total_cmp(right));
    let index = ((values.len() - 1) as f64 * p).ceil() as usize;
    values[index.min(values.len() - 1)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    use crate::scroll::{ScrollDeltaMode, ScrollDevice, ScrollInput, ScrollPhase};
    use cditor_core::layout::HeightChange;

    fn wheel(delta_y: f64, _timestamp: u64) -> TraceInput {
        TraceInput::Wheel(ScrollInput {
            delta_y,
            mode: ScrollDeltaMode::Pixel,
            phase: ScrollPhase::Changed,
            device: ScrollDevice::Trackpad,
            timestamp: Instant::now(),
        })
    }

    #[test]
    fn wheel_trace_replay_passes_regression_gate() {
        let frames = (0..100)
            .map(|frame| {
                ScrollTraceFrame::new(
                    frame,
                    Some(wheel(12.0, frame)),
                    frame as f64 * 12.0,
                    (frame + 1) as f64 * 12.0,
                    0..3,
                )
                .with_anchor(
                    ScrollAnchor {
                        block_id: 1,
                        offset_in_block: 0.0,
                        viewport_y: 0.0,
                    },
                    0.4,
                )
                .with_total_heights(10_000.0, 10_000.0)
                .with_frame_cost(4.0)
            })
            .collect::<Vec<_>>();

        let result =
            ScrollTraceReplay::new(frames).check_regression_gate(RegressionGateConfig::default());

        assert!(result.passed, "{:?}", result.failures);
        assert_eq!(result.report.thumb_reverse_jump_count, 0);
        assert_eq!(result.report.monotonicity_violations, 0);
        assert!(result.report.anchor_jitter_p95 <= 1.0);
    }

    #[test]
    fn scrollbar_drag_replay_fails_on_reverse_jump() {
        let frames = vec![
            ScrollTraceFrame::new(
                1,
                Some(TraceInput::ScrollbarDrag {
                    position_ratio: 0.5,
                    timestamp: 1,
                }),
                5_000.0,
                4_990.0,
                10..12,
            )
            .with_thumb_reverse_jump(true),
        ];

        let result =
            ScrollTraceReplay::new(frames).check_regression_gate(RegressionGateConfig::default());

        assert!(!result.passed);
        assert_eq!(result.report.thumb_reverse_jump_count, 1);
    }

    #[test]
    fn typing_trace_replay_checks_caret_jitter_and_frame_cost() {
        let frames = vec![
            ScrollTraceFrame::new(
                1,
                Some(TraceInput::Typing {
                    text: "a".to_owned(),
                    timestamp: 1,
                }),
                100.0,
                100.0,
                0..2,
            )
            .with_caret_jitter(0.5)
            .with_frame_cost(3.0),
            ScrollTraceFrame::new(
                2,
                Some(TraceInput::Typing {
                    text: "b".to_owned(),
                    timestamp: 2,
                }),
                100.0,
                100.0,
                0..2,
            )
            .with_caret_jitter(0.7)
            .with_frame_cost(3.2),
        ];

        let report = ScrollTraceReplay::new(frames).replay();

        assert_eq!(report.frame_count, 2);
        assert!(report.caret_jitter_p95 <= 1.0);
        assert_eq!(report.max_frame_cost_ms, 3.2);
    }

    #[test]
    fn height_chaos_replay_detects_correction_budget_exceeded() {
        let changes = (0..10)
            .map(|index| HeightChange {
                index,
                old_height: 20.0,
                new_height: 25.0,
                delta: 5.0,
            })
            .collect::<Vec<_>>();
        let frames = vec![
            ScrollTraceFrame::new(
                1,
                Some(TraceInput::AsyncHeightCorrection { timestamp: 1 }),
                0.0,
                5.0,
                0..2,
            )
            .with_height_changes(changes, 50.0)
            .with_total_heights(10_050.0, 10_000.0)
            .with_frame_cost(8.0),
        ];

        let result =
            ScrollTraceReplay::new(frames).check_regression_gate(RegressionGateConfig::default());

        assert!(!result.passed);
        assert_eq!(result.report.max_height_corrections_per_frame, 10);
        assert_eq!(result.report.total_height_correction_px, 50.0);
    }
}
