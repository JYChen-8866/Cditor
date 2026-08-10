use std::collections::HashMap;
use std::fmt;
use std::sync::{Mutex, OnceLock};

use cditor_core::ids::SurfaceId;
use cditor_text::TextLayoutCacheStats;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResolutionOutcome {
    Exact,
    CompatibleAccepted,
    CompatibleRejected,
    /// No snapshot matched the current shape identity; a stale snapshot for
    /// the same surface was painted for this frame while the real shape is
    /// pending in the scheduler queue.
    StaleAccepted,
    Missing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ResolutionState {
    pub(crate) requested_width_bits: Option<u32>,
    pub(crate) source_width_bits: Option<u32>,
    pub(crate) outcome: ResolutionOutcome,
    pub(crate) entries: usize,
    pub(crate) estimated_bytes: usize,
    pub(crate) pinned_entries: usize,
    pub(crate) misses: u64,
    pub(crate) reflows: u64,
    pub(crate) evictions: u64,
}

impl ResolutionState {
    pub(crate) fn new(
        requested_width_bits: Option<u32>,
        source_width_bits: Option<u32>,
        outcome: ResolutionOutcome,
        stats: TextLayoutCacheStats,
    ) -> Self {
        Self {
            requested_width_bits,
            source_width_bits,
            outcome,
            entries: stats.entries,
            estimated_bytes: stats.estimated_bytes,
            pinned_entries: stats.pinned_entries,
            misses: stats.misses,
            reflows: stats.reflows,
            evictions: stats.evictions,
        }
    }
}

pub(crate) fn trace_resolution(surface_id: SurfaceId, state: ResolutionState) {
    if !enabled() {
        return;
    }
    static LAST_STATE: OnceLock<Mutex<HashMap<SurfaceId, ResolutionState>>> = OnceLock::new();
    let changed = LAST_STATE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .map(|mut states| {
            let changed = states
                .get(&surface_id)
                .is_none_or(|previous| !equivalent_for_trace(*previous, state));
            states.insert(surface_id, state);
            changed
        })
        .unwrap_or(true);
    if !changed {
        return;
    }
    super::stderr::write(format_args!(
        "[cditor][text-layout][resolution] surface={surface_id:?} outcome={:?} requested_width={:?} source_width={:?} cache_entries={} cache_bytes={} pinned={} misses={} reflows={} evictions={}",
        state.outcome,
        width_from_bits(state.requested_width_bits),
        width_from_bits(state.source_width_bits),
        state.entries,
        state.estimated_bytes,
        state.pinned_entries,
        state.misses,
        state.reflows,
        state.evictions,
    ));
}

pub(crate) fn trace(event: &str, details: fmt::Arguments<'_>) {
    if enabled() {
        super::stderr::write(format_args!("[cditor][text-layout][{event}] {details}"));
    }
}

fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("CDITOR_TRACE_TEXT_LAYOUT")
            .ok()
            .as_deref()
            .is_some_and(env_value_enabled)
    })
}

fn env_value_enabled(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn width_from_bits(bits: Option<u32>) -> Option<f32> {
    bits.map(f32::from_bits)
}

fn equivalent_for_trace(previous: ResolutionState, current: ResolutionState) -> bool {
    if previous.outcome != ResolutionOutcome::Exact || current.outcome != ResolutionOutcome::Exact {
        return previous == current;
    }
    previous.entries == current.entries
        && previous.estimated_bytes == current.estimated_bytes
        && previous.pinned_entries == current.pinned_entries
        && previous.misses == current.misses
        && previous.reflows == current.reflows
        && previous.evictions == current.evictions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_flag_accepts_only_explicit_truthy_values() {
        for value in ["1", "true", "TRUE", "yes", "on"] {
            assert!(env_value_enabled(value));
        }
        for value in ["", "0", "false", "off", "anything"] {
            assert!(!env_value_enabled(value));
        }
    }

    #[test]
    fn resolution_state_preserves_cache_liveness_counters() {
        let stats = TextLayoutCacheStats {
            entries: 12,
            pinned_entries: 2,
            misses: 7,
            reflows: 5,
            evictions: 3,
            ..TextLayoutCacheStats::default()
        };
        let state = ResolutionState::new(
            Some(420.0_f32.to_bits()),
            Some(1.0_f32.to_bits()),
            ResolutionOutcome::CompatibleRejected,
            stats,
        );

        assert_eq!(width_from_bits(state.requested_width_bits), Some(420.0));
        assert_eq!(width_from_bits(state.source_width_bits), Some(1.0));
        assert_eq!(state.entries, 12);
        assert_eq!(state.evictions, 3);
    }

    #[test]
    fn exact_width_jitter_is_suppressed_until_cache_counters_change() {
        let stats = TextLayoutCacheStats {
            entries: 2,
            reflows: 1,
            ..TextLayoutCacheStats::default()
        };
        let first = ResolutionState::new(
            Some(781.0_f32.to_bits()),
            Some(781.0_f32.to_bits()),
            ResolutionOutcome::Exact,
            stats,
        );
        let second = ResolutionState::new(
            Some(782.0_f32.to_bits()),
            Some(782.0_f32.to_bits()),
            ResolutionOutcome::Exact,
            stats,
        );
        assert!(equivalent_for_trace(first, second));

        let changed = ResolutionState {
            evictions: 1,
            ..second
        };
        assert!(!equivalent_for_trace(first, changed));
    }
}
