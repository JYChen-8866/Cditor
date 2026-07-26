use std::time::Instant;

use cditor_core::rich_text::{BlockPayloadRecord, RichBlockKind};
use cditor_editor_protocol::command::{CommandEnvelope, CommandSource, EditorCommand};
use cditor_runtime::content::payload_window::PayloadWindowLoadResult;
use cditor_runtime::document_runtime::{DocumentRuntimeColdStartData, DocumentRuntimeIndexSource};
use cditor_runtime::{DocumentRuntime, PayloadCachePolicy, RealtimeInput, RealtimeInputRequest};
use cditor_viewport::scroll::ScrollbarPolicy;

use super::open::AcceptanceFixture;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MixedAcceptanceConfig {
    pub iterations: usize,
    pub frame_p95_ms_max: f64,
    pub frame_max_ms_max: f64,
    pub rendered_blocks_max: usize,
    pub resident_payloads_max: usize,
    pub resident_memory_max_bytes: usize,
}

impl Default for MixedAcceptanceConfig {
    fn default() -> Self {
        Self {
            iterations: 256,
            frame_p95_ms_max: 16.0,
            frame_max_ms_max: 50.0,
            rendered_blocks_max: 320,
            resident_payloads_max: 512,
            resident_memory_max_bytes: 48 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MixedAcceptanceResult {
    pub total_blocks: usize,
    pub iterations: usize,
    pub scroll_operations: usize,
    pub jump_operations: usize,
    pub edit_operations: usize,
    pub drag_operations: usize,
    pub frame_p50_ms: f64,
    pub frame_p95_ms: f64,
    pub frame_max_ms: f64,
    pub peak_rendered_blocks: usize,
    pub peak_resident_payloads: usize,
    pub peak_resident_memory_bytes: usize,
    pub failures: Vec<String>,
}

impl MixedAcceptanceResult {
    pub fn passed(&self) -> bool {
        self.failures.is_empty()
    }
}

pub fn run_mixed_acceptance(
    fixture: &AcceptanceFixture,
    config: MixedAcceptanceConfig,
) -> Result<MixedAcceptanceResult, String> {
    let initial_window_end = fixture.records.len().min(64);
    let initial_payloads = payload_records_for_ids(
        fixture
            .records
            .iter()
            .take(initial_window_end)
            .map(|record| record.id),
    );
    let (mut runtime, _) = DocumentRuntime::from_cold_start_data(
        DocumentRuntimeColdStartData {
            document_id: fixture.document_id,
            document_title: "100k mixed acceptance".to_owned(),
            structure_version: 1,
            records: fixture.records.clone(),
            block_attrs: Vec::new(),
            initial_payloads,
            initial_payload_window_end: initial_window_end,
            index_source: DocumentRuntimeIndexSource::Snapshot,
            layout_cache_hits: fixture.records.len(),
        },
        720.0,
    )?;

    let mut frame_samples = Vec::with_capacity(config.iterations);
    let mut scroll_operations = 0;
    let mut jump_operations = 0;
    let mut edit_operations = 0;
    let mut drag_operations = 0;
    let mut peak_rendered_blocks = 0;
    let mut peak_resident_payloads = runtime.loaded_payload_count();
    let mut peak_resident_memory_bytes = resident_memory_bytes(&runtime);
    let payload_policy = PayloadCachePolicy {
        max_entries: config.resident_payloads_max,
        max_estimated_bytes: config.resident_memory_max_bytes,
    };

    for iteration in 0..config.iterations {
        let frame_started = Instant::now();
        runtime.scroll_by_delta(if iteration.is_multiple_of(7) {
            -96.0
        } else {
            72.0
        })?;
        scroll_operations += 1;

        if iteration.is_multiple_of(8) {
            let target_index = mixed_target_index(iteration, fixture.records.len());
            let target_id = fixture.records[target_index].id;
            runtime.scroll_to_block_with_alignment(target_id, Some(0.5))?;
            jump_operations += 1;

            load_window_around(&mut runtime, fixture, target_index)?;
            runtime
                .dispatch(CommandEnvelope::new(
                    EditorCommand::FocusBlock {
                        block_id: target_id,
                    },
                    CommandSource::Automation,
                ))
                .map_err(|error| error.to_string())?;
            let expected = runtime
                .input_session_identity()
                .ok_or_else(|| format!("focused block {target_id} has no input session"))?;
            runtime
                .apply_realtime_input(RealtimeInputRequest {
                    expected,
                    input: RealtimeInput::ReplaceText {
                        range: None,
                        text: "x",
                    },
                })
                .map_err(|error| error.to_string())?;
            runtime.end_input_batch();
            let version = runtime
                .block_content_version(target_id)
                .ok_or_else(|| format!("edited block {target_id} lost its payload"))?;
            runtime.mark_payload_versions_persisted(&[(target_id, version)]);
            edit_operations += 1;
        }

        if iteration.is_multiple_of(16) {
            let policy = ScrollbarPolicy::default();
            let visual = runtime.begin_scrollbar_drag(policy);
            if visual.enabled {
                let ratio = ((iteration * 37) % 101) as f64 / 100.0;
                let max_thumb_top = (visual.track_height - visual.thumb_height).max(0.0);
                runtime.drag_scrollbar_to_thumb_top(policy, ratio * max_thumb_top)?;
                runtime.finish_scrollbar_drag()?;
                drag_operations += 1;
            }
        }

        let projection = runtime.projection_for_window_planned();
        peak_rendered_blocks = peak_rendered_blocks.max(projection.blocks.len());
        runtime.trim_payload_cache(payload_policy, []);
        peak_resident_payloads = peak_resident_payloads.max(runtime.loaded_payload_count());
        peak_resident_memory_bytes =
            peak_resident_memory_bytes.max(resident_memory_bytes(&runtime));
        frame_samples.push(frame_started.elapsed().as_secs_f64() * 1_000.0);
    }

    frame_samples.sort_by(f64::total_cmp);
    let mut result = MixedAcceptanceResult {
        total_blocks: fixture.records.len(),
        iterations: config.iterations,
        scroll_operations,
        jump_operations,
        edit_operations,
        drag_operations,
        frame_p50_ms: percentile(&frame_samples, 50),
        frame_p95_ms: percentile(&frame_samples, 95),
        frame_max_ms: frame_samples.last().copied().unwrap_or_default(),
        peak_rendered_blocks,
        peak_resident_payloads,
        peak_resident_memory_bytes,
        failures: Vec::new(),
    };
    validate_result(&mut result, config);
    Ok(result)
}

fn load_window_around(
    runtime: &mut DocumentRuntime,
    fixture: &AcceptanceFixture,
    target_index: usize,
) -> Result<(), String> {
    let start = target_index.saturating_sub(32);
    let end = (target_index + 33).min(fixture.records.len());
    let request = runtime.plan_payload_window_load(start..end);
    let records = payload_records_for_ids(request.block_ids.iter().copied());
    let decision = runtime.apply_payload_window_result(PayloadWindowLoadResult::prepare(
        request,
        records,
        Vec::new(),
    ));
    if matches!(
        decision,
        cditor_runtime::content::payload_window::PayloadWindowApplyDecision::Applied
    ) {
        Ok(())
    } else {
        Err("mixed benchmark payload generation became stale".to_owned())
    }
}

fn payload_records_for_ids(ids: impl IntoIterator<Item = u64>) -> Vec<BlockPayloadRecord> {
    ids.into_iter()
        .map(|block_id| {
            BlockPayloadRecord::rich_text(
                block_id,
                RichBlockKind::Paragraph,
                payload_text(block_id),
            )
        })
        .collect()
}

fn payload_text(block_id: u64) -> String {
    format!("benchmark block {block_id}")
}

fn mixed_target_index(iteration: usize, block_count: usize) -> usize {
    iteration
        .saturating_mul(7_919)
        .saturating_add(37)
        .checked_rem(block_count)
        .unwrap_or(0)
}

fn resident_memory_bytes(runtime: &DocumentRuntime) -> usize {
    runtime
        .estimated_payload_memory_bytes()
        .saturating_add(runtime.estimated_text_undo_memory_bytes())
}

fn percentile(sorted_samples: &[f64], percentile: usize) -> f64 {
    if sorted_samples.is_empty() {
        return 0.0;
    }
    let rank = (sorted_samples.len() * percentile).div_ceil(100);
    sorted_samples[rank.saturating_sub(1).min(sorted_samples.len() - 1)]
}

fn validate_result(result: &mut MixedAcceptanceResult, config: MixedAcceptanceConfig) {
    if result.frame_p95_ms > config.frame_p95_ms_max {
        result.failures.push(format!(
            "mixed frame p95 {:.2}ms exceeds {:.2}ms",
            result.frame_p95_ms, config.frame_p95_ms_max
        ));
    }
    if result.frame_max_ms > config.frame_max_ms_max {
        result.failures.push(format!(
            "mixed frame max {:.2}ms exceeds {:.2}ms",
            result.frame_max_ms, config.frame_max_ms_max
        ));
    }
    if result.peak_rendered_blocks > config.rendered_blocks_max {
        result.failures.push(format!(
            "peak rendered blocks {} exceeds {}",
            result.peak_rendered_blocks, config.rendered_blocks_max
        ));
    }
    if result.peak_resident_payloads > config.resident_payloads_max {
        result.failures.push(format!(
            "peak resident payloads {} exceeds {}",
            result.peak_resident_payloads, config.resident_payloads_max
        ));
    }
    if result.peak_resident_memory_bytes > config.resident_memory_max_bytes {
        result.failures.push(format!(
            "peak resident memory {} exceeds {} bytes",
            result.peak_resident_memory_bytes, config.resident_memory_max_bytes
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acceptance::open::fixture_100k_uneven_heights;

    #[test]
    fn real_100k_mixed_sequence_stays_bounded() {
        let fixture = fixture_100k_uneven_heights(77);
        let result = run_mixed_acceptance(
            &fixture,
            MixedAcceptanceConfig {
                iterations: 24,
                frame_p95_ms_max: 100.0,
                frame_max_ms_max: 250.0,
                ..MixedAcceptanceConfig::default()
            },
        )
        .unwrap();

        assert!(result.passed(), "{result:?}");
        assert_eq!(result.total_blocks, 100_000);
        assert_eq!(result.scroll_operations, 24);
        assert!(result.jump_operations > 0);
        assert_eq!(result.edit_operations, result.jump_operations);
        assert!(result.drag_operations > 0);
    }
}
