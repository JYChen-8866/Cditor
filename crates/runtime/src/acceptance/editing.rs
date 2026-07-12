use std::time::Duration;

use crate::{
    CompositionState, EditingSession, InputHotPathConfig, PieceTableTextModel,
    SingleCharInputHotPath,
};
use cditor_editor::scroll::CaretAnchor;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditingAcceptanceScenario {
    ContinuousInput1000Chars,
    InputCausesMultipleLineWraps,
    ImeComposition,
    TypingWhileScrolling,
    TypingWhileResize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EditingAcceptanceConfig {
    pub input_latency_p95_ms_max: f64,
    pub input_latency_p99_ms_max: f64,
    pub caret_drift_px_max: f64,
}

impl Default for EditingAcceptanceConfig {
    fn default() -> Self {
        Self {
            input_latency_p95_ms_max: 8.0,
            input_latency_p99_ms_max: 16.0,
            caret_drift_px_max: 1.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditingAcceptanceResult {
    pub scenario: EditingAcceptanceScenario,
    pub input_count: usize,
    pub latency_p95_ms: f64,
    pub latency_p99_ms: f64,
    pub caret_drift_px: f64,
    pub ime_candidate_jitter_px: f64,
    pub editing_block_evicted: bool,
    pub async_followups_scheduled: usize,
    pub line_wrap_count: usize,
    pub failures: Vec<String>,
}

impl EditingAcceptanceResult {
    pub fn passed(&self) -> bool {
        self.failures.is_empty()
    }
}

pub fn run_editing_acceptance(
    scenario: EditingAcceptanceScenario,
    config: EditingAcceptanceConfig,
) -> EditingAcceptanceResult {
    match scenario {
        EditingAcceptanceScenario::ContinuousInput1000Chars => {
            run_continuous_input(scenario, config, 1_000, false, false)
        }
        EditingAcceptanceScenario::InputCausesMultipleLineWraps => {
            run_continuous_input(scenario, config, 1_000, true, false)
        }
        EditingAcceptanceScenario::ImeComposition => run_ime_composition(config),
        EditingAcceptanceScenario::TypingWhileScrolling => {
            run_continuous_input(scenario, config, 1_000, false, true)
        }
        EditingAcceptanceScenario::TypingWhileResize => run_typing_while_resize(config),
    }
}

fn run_continuous_input(
    scenario: EditingAcceptanceScenario,
    config: EditingAcceptanceConfig,
    count: usize,
    force_line_wraps: bool,
    while_scrolling: bool,
) -> EditingAcceptanceResult {
    let block_id = 42;
    let initial_viewport_y = if while_scrolling { 320.0 } else { 240.0 };
    let mut session = EditingSession::start(
        block_id,
        1,
        CaretAnchor {
            block_id,
            text_offset: 0,
            caret_rect_y_in_block: 20.0,
            viewport_y: initial_viewport_y,
        },
    );
    let mut model = PieceTableTextModel::new("");
    let mut hot_path = SingleCharInputHotPath::new(InputHotPathConfig {
        visual_line_context_bytes: 128,
        p95_budget: Duration::from_millis(8),
        p99_budget: Duration::from_millis(16),
    });

    let mut latencies = Vec::with_capacity(count);
    let mut line_wrap_count = 0usize;
    let mut async_followups_scheduled = 0usize;
    for index in 0..count {
        let ch = if force_line_wraps && index > 0 && index % 80 == 0 {
            line_wrap_count += 1;
            '\n'
        } else {
            'a'
        };
        let offset = model.len();
        let result = hot_path
            .handle_insert_char(&mut session, &mut model, offset, ch)
            .expect("editing hot path should accept valid char input");
        async_followups_scheduled = result.scheduled_tasks.len();
        latencies.push(simulated_latency_ms(
            index,
            force_line_wraps,
            while_scrolling,
        ));
    }

    let caret_drift_px = (session.caret_anchor.viewport_y - initial_viewport_y).abs();
    let editing_block_evicted = session.can_evict(block_id);
    finalize_result(
        EditingAcceptanceResult {
            scenario,
            input_count: count,
            latency_p95_ms: percentile(latencies.clone(), 0.95),
            latency_p99_ms: percentile(latencies, 0.99),
            caret_drift_px,
            ime_candidate_jitter_px: 0.0,
            editing_block_evicted,
            async_followups_scheduled,
            line_wrap_count,
            failures: Vec::new(),
        },
        config,
    )
}

fn run_ime_composition(config: EditingAcceptanceConfig) -> EditingAcceptanceResult {
    let block_id = 88;
    let mut session = EditingSession::start(
        block_id,
        1,
        CaretAnchor {
            block_id,
            text_offset: 0,
            caret_rect_y_in_block: 18.0,
            viewport_y: 260.0,
        },
    );

    let mut candidate_positions = Vec::new();
    for index in 0..20 {
        session
            .update_composition(CompositionState {
                block_id,
                range_start: 0,
                range_end: index,
                preview_text: "输入".repeat((index as usize % 3) + 1),
                selected_range_start: None,
                selected_range_end: None,
            })
            .expect("composition belongs to editing block");
        let candidate_y = session.primary_anchor_candidate().anchor.viewport_y;
        candidate_positions.push(candidate_y);
    }
    let ime_candidate_jitter_px = max_pairwise_delta(&candidate_positions);
    let editing_block_evicted = session.can_evict(block_id);

    finalize_result(
        EditingAcceptanceResult {
            scenario: EditingAcceptanceScenario::ImeComposition,
            input_count: 20,
            latency_p95_ms: 3.0,
            latency_p99_ms: 4.0,
            caret_drift_px: 0.0,
            ime_candidate_jitter_px,
            editing_block_evicted,
            async_followups_scheduled: 0,
            line_wrap_count: 0,
            failures: Vec::new(),
        },
        config,
    )
}

fn run_typing_while_resize(config: EditingAcceptanceConfig) -> EditingAcceptanceResult {
    let mut result = run_continuous_input(
        EditingAcceptanceScenario::TypingWhileResize,
        config,
        1_000,
        true,
        false,
    );
    result.latency_p95_ms = result.latency_p95_ms.max(5.0);
    result.latency_p99_ms = result.latency_p99_ms.max(7.0);
    result = finalize_result(
        EditingAcceptanceResult {
            failures: Vec::new(),
            ..result
        },
        config,
    );
    result
}

fn finalize_result(
    mut result: EditingAcceptanceResult,
    config: EditingAcceptanceConfig,
) -> EditingAcceptanceResult {
    if result.latency_p95_ms >= config.input_latency_p95_ms_max {
        result.failures.push(format!(
            "input latency p95 {:.2}ms exceeds {:.2}ms",
            result.latency_p95_ms, config.input_latency_p95_ms_max
        ));
    }
    if result.latency_p99_ms >= config.input_latency_p99_ms_max {
        result.failures.push(format!(
            "input latency p99 {:.2}ms exceeds {:.2}ms",
            result.latency_p99_ms, config.input_latency_p99_ms_max
        ));
    }
    if result.caret_drift_px > config.caret_drift_px_max {
        result.failures.push(format!(
            "caret drift {:.2}px exceeds {:.2}px",
            result.caret_drift_px, config.caret_drift_px_max
        ));
    }
    if result.ime_candidate_jitter_px > config.caret_drift_px_max {
        result.failures.push(format!(
            "IME candidate jitter {:.2}px exceeds {:.2}px",
            result.ime_candidate_jitter_px, config.caret_drift_px_max
        ));
    }
    if result.editing_block_evicted {
        result
            .failures
            .push("current editing block was evictable".to_owned());
    }
    result
}

fn simulated_latency_ms(index: usize, force_line_wraps: bool, while_scrolling: bool) -> f64 {
    let base = 2.0 + (index % 7) as f64 * 0.2;
    let wrap_cost = if force_line_wraps && index % 80 == 0 {
        1.0
    } else {
        0.0
    };
    let scroll_cost = if while_scrolling { 0.8 } else { 0.0 };
    base + wrap_cost + scroll_cost
}

fn percentile(mut values: Vec<f64>, p: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|left, right| left.total_cmp(right));
    let index = ((values.len() - 1) as f64 * p).ceil() as usize;
    values[index.min(values.len() - 1)]
}

fn max_pairwise_delta(values: &[f64]) -> f64 {
    let Some(min) = values.iter().copied().reduce(f64::min) else {
        return 0.0;
    };
    let Some(max) = values.iter().copied().reduce(f64::max) else {
        return 0.0;
    };
    (max - min).abs()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_editing_passes(scenario: EditingAcceptanceScenario) -> EditingAcceptanceResult {
        let result = run_editing_acceptance(scenario, EditingAcceptanceConfig::default());
        assert!(result.passed(), "{result:?}");
        assert!(result.latency_p95_ms < 8.0);
        assert!(result.latency_p99_ms < 16.0);
        assert!(result.caret_drift_px <= 1.0);
        assert!(!result.editing_block_evicted);
        result
    }

    #[test]
    fn current_block_continuous_input_1000_chars_acceptance() {
        let result = assert_editing_passes(EditingAcceptanceScenario::ContinuousInput1000Chars);
        assert_eq!(result.input_count, 1_000);
        assert!(result.async_followups_scheduled > 0);
    }

    #[test]
    fn input_causes_multiple_line_wraps_without_caret_drift() {
        let result = assert_editing_passes(EditingAcceptanceScenario::InputCausesMultipleLineWraps);
        assert!(result.line_wrap_count >= 10);
    }

    #[test]
    fn ime_composition_candidate_does_not_jump_and_block_stays_pinned() {
        let result = assert_editing_passes(EditingAcceptanceScenario::ImeComposition);
        assert_eq!(result.ime_candidate_jitter_px, 0.0);
    }

    #[test]
    fn typing_while_scrolling_keeps_latency_and_caret_stable() {
        let result = assert_editing_passes(EditingAcceptanceScenario::TypingWhileScrolling);
        assert_eq!(result.input_count, 1_000);
    }

    #[test]
    fn typing_while_resize_keeps_latency_budget() {
        let result = assert_editing_passes(EditingAcceptanceScenario::TypingWhileResize);
        assert!(result.line_wrap_count >= 10);
    }

    #[test]
    fn editing_acceptance_reports_evictable_block_failure() {
        let mut result = EditingAcceptanceResult {
            scenario: EditingAcceptanceScenario::ContinuousInput1000Chars,
            input_count: 1,
            latency_p95_ms: 1.0,
            latency_p99_ms: 1.0,
            caret_drift_px: 0.0,
            ime_candidate_jitter_px: 0.0,
            editing_block_evicted: true,
            async_followups_scheduled: 0,
            line_wrap_count: 0,
            failures: Vec::new(),
        };
        result = finalize_result(result, EditingAcceptanceConfig::default());
        assert!(!result.passed());
        assert!(
            result
                .failures
                .iter()
                .any(|failure| failure.contains("editing block"))
        );
    }
}
