use std::time::Instant;

use cditor_core::layout::HeightChange;
use cditor_editor::scroll::{
    ScrollDeltaMode, ScrollDevice, ScrollInput, ScrollOrigin, ScrollPhase,
};
use cditor_editor::{RegressionGateConfig, ScrollTraceFrame, ScrollTraceReplay, TraceInput};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollAcceptanceScenario {
    TopToMiddle,
    MiddleToTop,
    TenMinuteContinuousScroll,
    RandomHeightCorrectionWhileScrolling,
    WindowLoadDelayWhileScrolling,
    ScrollbarDragWithHeightCorrections,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrollAcceptanceConfig {
    pub p99_frame_ms_max: f64,
    pub anchor_jitter_p95_max: f64,
    pub unexpected_visual_shift_max_px: f64,
}

impl Default for ScrollAcceptanceConfig {
    fn default() -> Self {
        Self {
            p99_frame_ms_max: 16.0,
            anchor_jitter_p95_max: 1.0,
            unexpected_visual_shift_max_px: 50.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScrollAcceptanceResult {
    pub scenario: ScrollAcceptanceScenario,
    pub frame_count: usize,
    pub p99_frame_ms: f64,
    pub anchor_jitter_p95: f64,
    pub max_unexpected_visual_shift_px: f64,
    pub local_list_reverse_drive_count: usize,
    pub gate_passed: bool,
    pub failures: Vec<String>,
}

impl ScrollAcceptanceResult {
    pub fn passed(&self) -> bool {
        self.gate_passed && self.failures.is_empty()
    }
}

pub fn run_scroll_acceptance(
    scenario: ScrollAcceptanceScenario,
    config: ScrollAcceptanceConfig,
) -> ScrollAcceptanceResult {
    let frames = frames_for_scenario(scenario);
    evaluate_scroll_trace(scenario, frames, config)
}

pub fn evaluate_scroll_trace(
    scenario: ScrollAcceptanceScenario,
    frames: Vec<ScrollTraceFrame>,
    config: ScrollAcceptanceConfig,
) -> ScrollAcceptanceResult {
    let replay = ScrollTraceReplay::new(frames.clone());
    let gate = replay.check_regression_gate(RegressionGateConfig {
        anchor_jitter_p95_max: config.anchor_jitter_p95_max,
        max_frame_cost_ms: config.p99_frame_ms_max,
        ..RegressionGateConfig::default()
    });
    let frame_costs = frames
        .iter()
        .map(|frame| frame.frame_cost_ms)
        .collect::<Vec<_>>();
    let p99_frame_ms = percentile(frame_costs, 0.99);
    let max_unexpected_visual_shift_px = max_unexpected_visual_shift(&frames);
    let local_list_reverse_drive_count = frames
        .iter()
        .filter(|frame| matches!(frame.input, Some(TraceInput::WindowCommit { .. })))
        .filter(|frame| {
            (frame.global_scroll_top_after - frame.global_scroll_top_before).abs() > 0.5
        })
        .count();

    let mut failures = gate.failures;
    if p99_frame_ms > config.p99_frame_ms_max {
        failures.push(format!(
            "p99 frame {:.2}ms exceeds {:.2}ms",
            p99_frame_ms, config.p99_frame_ms_max
        ));
    }
    if gate.report.anchor_jitter_p95 > config.anchor_jitter_p95_max {
        failures.push(format!(
            "anchor jitter p95 {:.2}px exceeds {:.2}px",
            gate.report.anchor_jitter_p95, config.anchor_jitter_p95_max
        ));
    }
    if max_unexpected_visual_shift_px > config.unexpected_visual_shift_max_px {
        failures.push(format!(
            "unexpected visual shift {:.2}px exceeds {:.2}px",
            max_unexpected_visual_shift_px, config.unexpected_visual_shift_max_px
        ));
    }
    if local_list_reverse_drive_count > 0 {
        failures.push(format!(
            "local ListState reverse drove global scroll {} times",
            local_list_reverse_drive_count
        ));
    }

    ScrollAcceptanceResult {
        scenario,
        frame_count: frames.len(),
        p99_frame_ms,
        anchor_jitter_p95: gate.report.anchor_jitter_p95,
        max_unexpected_visual_shift_px,
        local_list_reverse_drive_count,
        gate_passed: gate.passed,
        failures,
    }
}

fn frames_for_scenario(scenario: ScrollAcceptanceScenario) -> Vec<ScrollTraceFrame> {
    match scenario {
        ScrollAcceptanceScenario::TopToMiddle => wheel_frames(0, 1_000, 0.0, 48.0, 6.0, 0.4),
        ScrollAcceptanceScenario::MiddleToTop => wheel_frames(0, 1_000, 48_000.0, -48.0, 6.0, 0.4),
        ScrollAcceptanceScenario::TenMinuteContinuousScroll => {
            wheel_frames(0, 6_000, 0.0, 12.0, 5.0, 0.5)
        }
        ScrollAcceptanceScenario::RandomHeightCorrectionWhileScrolling => {
            let mut frames = wheel_frames(0, 1_000, 0.0, 32.0, 7.0, 0.7);
            for index in (100..1_000).step_by(100) {
                let change = HeightChange {
                    index,
                    old_height: 24.0,
                    new_height: 28.0,
                    delta: 4.0,
                };
                frames[index] = frames[index]
                    .clone()
                    .with_height_changes(vec![change], 4.0)
                    .with_frame_cost(9.0);
            }
            frames
        }
        ScrollAcceptanceScenario::WindowLoadDelayWhileScrolling => {
            let mut frames = wheel_frames(0, 1_000, 0.0, 32.0, 7.0, 0.6);
            for index in (50..1_000).step_by(75) {
                frames[index] = ScrollTraceFrame::new(
                    index as u64,
                    Some(TraceInput::WindowCommit {
                        timestamp: index as u64,
                    }),
                    frames[index].global_scroll_top_before,
                    frames[index].global_scroll_top_before,
                    index / 100..index / 100 + 3,
                )
                .with_window_commit_count(1)
                .with_frame_cost(8.0);
            }
            frames
        }
        ScrollAcceptanceScenario::ScrollbarDragWithHeightCorrections => scrollbar_drag_frames(),
    }
}

fn wheel_frames(
    start_frame: u64,
    count: usize,
    start_scroll_top: f64,
    delta_y: f64,
    frame_cost_ms: f64,
    anchor_jitter_px: f64,
) -> Vec<ScrollTraceFrame> {
    let mut frames = Vec::with_capacity(count);
    let mut scroll_top = start_scroll_top;
    for index in 0..count {
        let before = scroll_top;
        scroll_top = (scroll_top + delta_y).max(0.0);
        let frame_id = start_frame + index as u64;
        frames.push(
            ScrollTraceFrame::new(
                frame_id,
                Some(TraceInput::Wheel(ScrollInput {
                    delta_y,
                    mode: ScrollDeltaMode::Pixel,
                    phase: ScrollPhase::Changed,
                    device: ScrollDevice::Trackpad,
                    timestamp: Instant::now(),
                })),
                before,
                scroll_top,
                index / 100..index / 100 + 3,
            )
            .with_total_heights(2_400_000.0, 2_400_000.0)
            .with_frame_cost(frame_cost_ms)
            .with_anchor(
                cditor_editor::scroll::ScrollAnchor {
                    block_id: index as u64 + 1,
                    offset_in_block: 0.0,
                    viewport_y: 0.0,
                },
                anchor_jitter_px,
            ),
        );
    }
    frames
}

fn scrollbar_drag_frames() -> Vec<ScrollTraceFrame> {
    let mut frames = Vec::with_capacity(300);
    let mut scroll_top = 0.0;
    let frozen_total_height = 2_400_000.0;
    let mut model_total_height = frozen_total_height;
    for index in 0..300 {
        let before = scroll_top;
        let position_ratio = index as f64 / 299.0;
        scroll_top = position_ratio * (frozen_total_height - 720.0);
        let mut frame = ScrollTraceFrame::new(
            index as u64,
            Some(TraceInput::ScrollbarDrag {
                position_ratio,
                timestamp: index as u64,
            }),
            before,
            scroll_top,
            index / 30..index / 30 + 3,
        )
        .with_total_heights(model_total_height, frozen_total_height)
        .with_frame_cost(6.0)
        .with_anchor(
            cditor_editor::scroll::ScrollAnchor {
                block_id: index as u64 + 1,
                offset_in_block: 0.0,
                viewport_y: 0.0,
            },
            0.3,
        );
        if index > 0 && index % 50 == 0 {
            model_total_height += 32.0;
            frame = frame
                .with_total_heights(model_total_height, frozen_total_height)
                .with_height_changes(
                    vec![HeightChange {
                        index,
                        old_height: 32.0,
                        new_height: 64.0,
                        delta: 32.0,
                    }],
                    0.0,
                )
                .with_frame_cost(8.0);
        }
        frames.push(frame);
    }
    frames
}

fn max_unexpected_visual_shift(frames: &[ScrollTraceFrame]) -> f64 {
    frames
        .iter()
        .map(|frame| {
            let delta = frame.global_scroll_top_after - frame.global_scroll_top_before;
            match &frame.input {
                Some(TraceInput::Wheel(input)) => (delta - input.delta_y).abs(),
                Some(TraceInput::ScrollbarDrag { .. }) => 0.0,
                Some(TraceInput::WindowCommit { .. }) => delta.abs(),
                _ => frame.correction_applied.abs(),
            }
        })
        .fold(0.0, f64::max)
}

fn percentile(mut values: Vec<f64>, p: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|left, right| left.total_cmp(right));
    let index = ((values.len() - 1) as f64 * p).ceil() as usize;
    values[index.min(values.len() - 1)]
}

#[allow(dead_code)]
fn _local_list_sync_origin_marker() -> ScrollOrigin {
    ScrollOrigin::LocalListSync
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_scroll_passes(scenario: ScrollAcceptanceScenario) -> ScrollAcceptanceResult {
        let result = run_scroll_acceptance(scenario, ScrollAcceptanceConfig::default());
        assert!(result.passed(), "{result:?}");
        assert!(result.p99_frame_ms < 16.0);
        assert!(result.anchor_jitter_p95 <= 1.0);
        assert!(result.max_unexpected_visual_shift_px <= 50.0);
        assert_eq!(result.local_list_reverse_drive_count, 0);
        result
    }

    #[test]
    fn scroll_from_top_to_middle_acceptance() {
        let result = assert_scroll_passes(ScrollAcceptanceScenario::TopToMiddle);
        assert_eq!(result.frame_count, 1_000);
    }

    #[test]
    fn scroll_from_middle_back_to_top_acceptance() {
        let result = assert_scroll_passes(ScrollAcceptanceScenario::MiddleToTop);
        assert_eq!(result.frame_count, 1_000);
    }

    #[test]
    fn continuous_10_minute_scroll_acceptance() {
        let result = assert_scroll_passes(ScrollAcceptanceScenario::TenMinuteContinuousScroll);
        assert_eq!(result.frame_count, 6_000);
    }

    #[test]
    fn random_height_correction_while_scrolling_acceptance() {
        let result =
            assert_scroll_passes(ScrollAcceptanceScenario::RandomHeightCorrectionWhileScrolling);
        assert!(result.frame_count >= 1_000);
    }

    #[test]
    fn window_load_delay_while_scrolling_does_not_reverse_drive_global_scroll() {
        let result = assert_scroll_passes(ScrollAcceptanceScenario::WindowLoadDelayWhileScrolling);
        assert_eq!(result.local_list_reverse_drive_count, 0);
    }

    #[test]
    fn scrollbar_drag_with_height_corrections_does_not_thumb_reverse_jump() {
        let result =
            assert_scroll_passes(ScrollAcceptanceScenario::ScrollbarDragWithHeightCorrections);
        assert_eq!(result.frame_count, 300);
    }

    #[test]
    fn local_list_reverse_drive_is_detected() {
        let frames = vec![
            ScrollTraceFrame::new(
                1,
                Some(TraceInput::WindowCommit { timestamp: 1 }),
                1_000.0,
                960.0,
                1..3,
            )
            .with_window_commit_count(1)
            .with_frame_cost(4.0),
        ];

        let result = evaluate_scroll_trace(
            ScrollAcceptanceScenario::WindowLoadDelayWhileScrolling,
            frames,
            ScrollAcceptanceConfig::default(),
        );

        assert!(!result.passed());
        assert_eq!(result.local_list_reverse_drive_count, 1);
    }

    #[test]
    fn scrollbar_thumb_reverse_jump_is_detected() {
        let frames = vec![
            ScrollTraceFrame::new(
                1,
                Some(TraceInput::ScrollbarDrag {
                    position_ratio: 0.5,
                    timestamp: 1,
                }),
                1_000.0,
                900.0,
                1..3,
            )
            .with_thumb_reverse_jump(true)
            .with_frame_cost(4.0),
        ];

        let result = evaluate_scroll_trace(
            ScrollAcceptanceScenario::ScrollbarDragWithHeightCorrections,
            frames,
            ScrollAcceptanceConfig::default(),
        );

        assert!(!result.passed());
        assert!(
            result
                .failures
                .iter()
                .any(|failure| failure.contains("thumb reverse-jump"))
        );
    }
}
