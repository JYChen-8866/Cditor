use std::time::{Duration, Instant};

use crate::scroll::virtual_scroll::{
    LayoutPx, ScrollOrigin, VirtualScrollError, VirtualScrollState, VirtualScrollTarget,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollDeltaMode {
    Pixel,
    Line,
    Page,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollPhase {
    Began,
    Changed,
    Momentum,
    Ended,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollDevice {
    Wheel,
    Trackpad,
    Unknown,
}

#[derive(Debug, Clone, Copy)]
pub struct ScrollInput {
    pub delta_y: LayoutPx,
    pub mode: ScrollDeltaMode,
    pub phase: ScrollPhase,
    pub device: ScrollDevice,
    pub timestamp: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollInteractionState {
    Idle,
    WheelActive,
    Momentum,
    ScrollbarDragging,
    ProgrammaticJump,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeightCorrectionPriority {
    Normal,
    DeferRemote,
    DeferUntilIdle,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WheelPipelineConfig {
    pub line_height_px: LayoutPx,
    pub page_scroll_ratio: f64,
    pub idle_after: Duration,
}

impl Default for WheelPipelineConfig {
    fn default() -> Self {
        Self {
            line_height_px: 20.0,
            page_scroll_ratio: 0.85,
            idle_after: Duration::from_millis(80),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ScrollAccumulator {
    pub pending_delta_y: LayoutPx,
    pub phase: ScrollPhase,
    pub last_input_at: Option<Instant>,
    pub interaction_state: ScrollInteractionState,
    pub committed_frames: usize,
    pub received_inputs: usize,
    config: WheelPipelineConfig,
}

impl ScrollAccumulator {
    pub fn new(config: WheelPipelineConfig) -> Self {
        Self {
            pending_delta_y: 0.0,
            phase: ScrollPhase::Ended,
            last_input_at: None,
            interaction_state: ScrollInteractionState::Idle,
            committed_frames: 0,
            received_inputs: 0,
            config,
        }
    }

    pub fn push_input(&mut self, input: ScrollInput, viewport_height: LayoutPx) {
        self.pending_delta_y += normalize_scroll_delta(input, viewport_height, self.config);
        self.phase = input.phase;
        self.last_input_at = Some(input.timestamp);
        self.received_inputs += 1;
        self.interaction_state = match input.phase {
            ScrollPhase::Momentum => ScrollInteractionState::Momentum,
            ScrollPhase::Ended | ScrollPhase::Cancelled => {
                if self.pending_delta_y.abs() > f64::EPSILON {
                    self.interaction_state
                } else {
                    ScrollInteractionState::Idle
                }
            }
            ScrollPhase::Began | ScrollPhase::Changed => ScrollInteractionState::WheelActive,
        };
    }

    pub fn apply_frame(
        &mut self,
        state: &mut VirtualScrollState,
    ) -> Result<Option<VirtualScrollTarget>, VirtualScrollError> {
        if self.pending_delta_y.abs() <= f64::EPSILON {
            self.maybe_mark_idle(Instant::now());
            return Ok(None);
        }

        let delta = self.pending_delta_y;
        self.pending_delta_y = 0.0;
        self.committed_frames += 1;
        state.scroll_by_delta(delta, ScrollOrigin::UserWheel)
    }

    pub fn maybe_mark_idle(&mut self, now: Instant) {
        let Some(last_input_at) = self.last_input_at else {
            self.interaction_state = ScrollInteractionState::Idle;
            return;
        };
        if matches!(self.phase, ScrollPhase::Ended | ScrollPhase::Cancelled)
            || now.duration_since(last_input_at) >= self.config.idle_after
        {
            self.interaction_state = ScrollInteractionState::Idle;
        }
    }

    pub fn height_correction_priority(&self) -> HeightCorrectionPriority {
        match self.interaction_state {
            ScrollInteractionState::Idle => HeightCorrectionPriority::Normal,
            ScrollInteractionState::WheelActive => HeightCorrectionPriority::DeferRemote,
            ScrollInteractionState::Momentum => HeightCorrectionPriority::DeferUntilIdle,
            ScrollInteractionState::ScrollbarDragging => HeightCorrectionPriority::DeferUntilIdle,
            ScrollInteractionState::ProgrammaticJump => HeightCorrectionPriority::DeferRemote,
        }
    }
}

impl Default for ScrollAccumulator {
    fn default() -> Self {
        Self::new(WheelPipelineConfig::default())
    }
}

pub fn normalize_scroll_delta(
    input: ScrollInput,
    viewport_height: LayoutPx,
    config: WheelPipelineConfig,
) -> LayoutPx {
    match input.mode {
        ScrollDeltaMode::Pixel => input.delta_y,
        ScrollDeltaMode::Line => input.delta_y * config.line_height_px,
        ScrollDeltaMode::Page => input.delta_y * viewport_height * config.page_scroll_ratio,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_pixel_line_and_page_delta_modes() {
        let now = Instant::now();
        let config = WheelPipelineConfig::default();

        assert_eq!(
            normalize_scroll_delta(
                ScrollInput {
                    delta_y: 3.0,
                    mode: ScrollDeltaMode::Pixel,
                    phase: ScrollPhase::Changed,
                    device: ScrollDevice::Wheel,
                    timestamp: now,
                },
                1_000.0,
                config,
            ),
            3.0
        );
        assert_eq!(
            normalize_scroll_delta(
                ScrollInput {
                    delta_y: 3.0,
                    mode: ScrollDeltaMode::Line,
                    phase: ScrollPhase::Changed,
                    device: ScrollDevice::Wheel,
                    timestamp: now,
                },
                1_000.0,
                config,
            ),
            60.0
        );
        assert_eq!(
            normalize_scroll_delta(
                ScrollInput {
                    delta_y: 1.0,
                    mode: ScrollDeltaMode::Page,
                    phase: ScrollPhase::Changed,
                    device: ScrollDevice::Wheel,
                    timestamp: now,
                },
                1_000.0,
                config,
            ),
            850.0
        );
    }

    #[test]
    fn merges_multiple_inputs_into_one_frame_commit() {
        let now = Instant::now();
        let mut state = VirtualScrollState::new(100.0, 1_000.0).unwrap();
        let mut accumulator = ScrollAccumulator::default();

        for _ in 0..5 {
            accumulator.push_input(
                ScrollInput {
                    delta_y: 10.0,
                    mode: ScrollDeltaMode::Pixel,
                    phase: ScrollPhase::Changed,
                    device: ScrollDevice::Trackpad,
                    timestamp: now,
                },
                state.viewport_height,
            );
        }

        let target = accumulator.apply_frame(&mut state).unwrap().unwrap();

        assert_eq!(target.global_scroll_top, 50.0);
        assert_eq!(state.global_scroll_top, 50.0);
        assert_eq!(accumulator.received_inputs, 5);
        assert_eq!(accumulator.committed_frames, 1);
        assert_eq!(accumulator.pending_delta_y, 0.0);
    }

    #[test]
    fn scroll_jitter_trace_preserves_direction_and_clamps() {
        let start = Instant::now();
        let mut state = VirtualScrollState::new(100.0, 10_000.0).unwrap();
        let mut accumulator = ScrollAccumulator::default();
        let mut previous = state.global_scroll_top;

        for frame in 0..120 {
            accumulator.push_input(
                ScrollInput {
                    delta_y: 7.5,
                    mode: ScrollDeltaMode::Pixel,
                    phase: ScrollPhase::Changed,
                    device: ScrollDevice::Trackpad,
                    timestamp: start + Duration::from_millis(frame),
                },
                state.viewport_height,
            );
            accumulator.apply_frame(&mut state).unwrap();
            assert!(state.global_scroll_top >= previous);
            previous = state.global_scroll_top;
        }

        for frame in 120..240 {
            accumulator.push_input(
                ScrollInput {
                    delta_y: -7.5,
                    mode: ScrollDeltaMode::Pixel,
                    phase: ScrollPhase::Changed,
                    device: ScrollDevice::Trackpad,
                    timestamp: start + Duration::from_millis(frame),
                },
                state.viewport_height,
            );
            accumulator.apply_frame(&mut state).unwrap();
        }

        assert_eq!(state.global_scroll_top, 0.0);
    }

    #[test]
    fn momentum_trace_defers_remote_height_correction_until_idle() {
        let start = Instant::now();
        let mut accumulator = ScrollAccumulator::default();

        accumulator.push_input(
            ScrollInput {
                delta_y: 100.0,
                mode: ScrollDeltaMode::Pixel,
                phase: ScrollPhase::Began,
                device: ScrollDevice::Trackpad,
                timestamp: start,
            },
            1_000.0,
        );
        assert_eq!(
            accumulator.interaction_state,
            ScrollInteractionState::WheelActive
        );
        assert_eq!(
            accumulator.height_correction_priority(),
            HeightCorrectionPriority::DeferRemote
        );

        accumulator.push_input(
            ScrollInput {
                delta_y: 30.0,
                mode: ScrollDeltaMode::Pixel,
                phase: ScrollPhase::Momentum,
                device: ScrollDevice::Trackpad,
                timestamp: start + Duration::from_millis(16),
            },
            1_000.0,
        );
        assert_eq!(
            accumulator.interaction_state,
            ScrollInteractionState::Momentum
        );
        assert_eq!(
            accumulator.height_correction_priority(),
            HeightCorrectionPriority::DeferUntilIdle
        );

        accumulator.push_input(
            ScrollInput {
                delta_y: 0.0,
                mode: ScrollDeltaMode::Pixel,
                phase: ScrollPhase::Ended,
                device: ScrollDevice::Trackpad,
                timestamp: start + Duration::from_millis(120),
            },
            1_000.0,
        );
        accumulator.pending_delta_y = 0.0;
        accumulator.maybe_mark_idle(start + Duration::from_millis(121));
        assert_eq!(accumulator.interaction_state, ScrollInteractionState::Idle);
        assert_eq!(
            accumulator.height_correction_priority(),
            HeightCorrectionPriority::Normal
        );
    }

    #[test]
    fn p99_wheel_frame_pipeline_is_under_16ms_without_hydration_work() {
        let mut durations = Vec::with_capacity(240);
        let mut state = VirtualScrollState::new(800.0, 20_000_000.0).unwrap();
        let mut accumulator = ScrollAccumulator::default();
        let start = Instant::now();

        for frame in 0..240 {
            let before = Instant::now();
            for event in 0..8 {
                accumulator.push_input(
                    ScrollInput {
                        delta_y: 3.0,
                        mode: ScrollDeltaMode::Line,
                        phase: ScrollPhase::Changed,
                        device: ScrollDevice::Wheel,
                        timestamp: start + Duration::from_millis(frame * 16 + event),
                    },
                    state.viewport_height,
                );
            }
            accumulator.apply_frame(&mut state).unwrap();
            durations.push(before.elapsed());
        }

        durations.sort_unstable();
        let p99 = durations[(durations.len() as f64 * 0.99).floor() as usize];
        assert!(
            p99 < Duration::from_millis(16),
            "p99 wheel frame was {p99:?}"
        );
        assert_eq!(accumulator.committed_frames, 240);
        assert_eq!(accumulator.received_inputs, 1_920);
    }
}
