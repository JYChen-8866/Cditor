use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;

#[cfg(test)]
const SCHEMA_VERSION: u32 = 1;
const DEFAULT_FRAME_BUDGET: Duration = Duration::from_micros(16_667);
const RECENT_FRAME_CAPACITY: usize = 240;
const LONG_FRAME_CAPACITY: usize = 64;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FrameTelemetrySample {
    pub frame_id: u64,
    pub captured_at_unix_ms: u64,
    pub elapsed_ms: f64,
    pub deadline_ms: f64,
    pub overrun_ms: f64,
    pub interaction: String,
    pub queues: FrameQueueSnapshot,
    pub window: FrameWindowSnapshot,
    pub entities: FrameEntitySnapshot,
    pub caches: FrameCacheSnapshot,
    pub text_geometry_fallback_rate: f64,
    pub reasons: Vec<LongFrameReason>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct FrameQueueSnapshot {
    pub pending_layout_tasks: usize,
    pub pending_payload_loads: usize,
    pub pending_saves: usize,
    pub scheduler_lanes_connected: bool,
    pub realtime_lane_depth: Option<usize>,
    pub interactive_lane_depth: Option<usize>,
    pub visible_lane_depth: Option<usize>,
    pub prefetch_lane_depth: Option<usize>,
    pub background_lane_depth: Option<usize>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct FrameWindowSnapshot {
    pub document_blocks: usize,
    pub payload_start: usize,
    pub payload_end: usize,
    pub page_start: usize,
    pub page_end: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct FrameEntitySnapshot {
    pub rendered_blocks: usize,
    pub loaded_payloads: usize,
    pub block_layouts: usize,
    pub table_cell_layouts: usize,
    pub auxiliary_layouts: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct FrameCacheSnapshot {
    pub payload_and_undo_bytes: usize,
    pub platform_layout_bytes: usize,
    pub payload_cache_over_budget: bool,
    pub platform_layout_cache_over_budget: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LongFrameReason {
    LayoutBacklog,
    PayloadLoadBacklog,
    PersistenceBacklog,
    EntityPressure,
    PayloadMemoryPressure,
    PlatformLayoutMemoryPressure,
    TextGeometryFallback,
    Unattributed,
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FrameTelemetrySnapshot {
    pub schema_version: u32,
    pub total_frames: u64,
    pub total_long_frames: u64,
    pub dropped_recent_frames: u64,
    pub dropped_long_frames: u64,
    pub recent_frames: Vec<FrameTelemetrySample>,
    pub long_frames: Vec<FrameTelemetrySample>,
}

#[derive(Debug, Clone)]
pub(crate) struct AppFrameTelemetryInput {
    pub elapsed: Duration,
    pub interaction: String,
    pub queues: FrameQueueSnapshot,
    pub window: FrameWindowSnapshot,
    pub entities: FrameEntitySnapshot,
    pub caches: FrameCacheSnapshot,
    pub text_geometry_fallback_rate: f64,
}

#[derive(Default)]
struct FrameTelemetryStore {
    total_frames: u64,
    total_long_frames: u64,
    dropped_recent_frames: u64,
    dropped_long_frames: u64,
    recent_frames: VecDeque<FrameTelemetrySample>,
    long_frames: VecDeque<FrameTelemetrySample>,
}

pub(crate) fn record_app_frame(input: AppFrameTelemetryInput) {
    let mut store = telemetry_store()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    store.total_frames = store.total_frames.saturating_add(1);
    let elapsed_ms = input.elapsed.as_secs_f64() * 1_000.0;
    let deadline_ms = DEFAULT_FRAME_BUDGET.as_secs_f64() * 1_000.0;
    let overrun_ms = (elapsed_ms - deadline_ms).max(0.0);
    let reasons = if overrun_ms > 0.0 {
        classify_long_frame(&input)
    } else {
        Vec::new()
    };
    let sample = FrameTelemetrySample {
        frame_id: store.total_frames,
        captured_at_unix_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX),
        elapsed_ms,
        deadline_ms,
        overrun_ms,
        interaction: input.interaction,
        queues: input.queues,
        window: input.window,
        entities: input.entities,
        caches: input.caches,
        text_geometry_fallback_rate: input.text_geometry_fallback_rate,
        reasons,
    };
    if push_bounded(
        &mut store.recent_frames,
        sample.clone(),
        RECENT_FRAME_CAPACITY,
    ) {
        store.dropped_recent_frames = store.dropped_recent_frames.saturating_add(1);
    }
    if overrun_ms > 0.0 {
        store.total_long_frames = store.total_long_frames.saturating_add(1);
        if push_bounded(&mut store.long_frames, sample, LONG_FRAME_CAPACITY) {
            store.dropped_long_frames = store.dropped_long_frames.saturating_add(1);
        }
    }
}

#[cfg(test)]
pub fn frame_telemetry_snapshot() -> FrameTelemetrySnapshot {
    let store = telemetry_store()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    FrameTelemetrySnapshot {
        schema_version: SCHEMA_VERSION,
        total_frames: store.total_frames,
        total_long_frames: store.total_long_frames,
        dropped_recent_frames: store.dropped_recent_frames,
        dropped_long_frames: store.dropped_long_frames,
        recent_frames: store.recent_frames.iter().cloned().collect(),
        long_frames: store.long_frames.iter().cloned().collect(),
    }
}

#[cfg(test)]
pub fn export_frame_telemetry_json() -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(&frame_telemetry_snapshot())
}

fn telemetry_store() -> &'static Mutex<FrameTelemetryStore> {
    static STORE: OnceLock<Mutex<FrameTelemetryStore>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(FrameTelemetryStore::default()))
}

fn classify_long_frame(input: &AppFrameTelemetryInput) -> Vec<LongFrameReason> {
    let mut reasons = Vec::new();
    if input.queues.pending_layout_tasks > 0 {
        reasons.push(LongFrameReason::LayoutBacklog);
    }
    if input.queues.pending_payload_loads > 0 {
        reasons.push(LongFrameReason::PayloadLoadBacklog);
    }
    if input.queues.pending_saves > 0 {
        reasons.push(LongFrameReason::PersistenceBacklog);
    }
    if input.entities.rendered_blocks > 256 {
        reasons.push(LongFrameReason::EntityPressure);
    }
    if input.caches.payload_cache_over_budget {
        reasons.push(LongFrameReason::PayloadMemoryPressure);
    }
    if input.caches.platform_layout_cache_over_budget {
        reasons.push(LongFrameReason::PlatformLayoutMemoryPressure);
    }
    if input.text_geometry_fallback_rate > 0.0 {
        reasons.push(LongFrameReason::TextGeometryFallback);
    }
    if reasons.is_empty() {
        reasons.push(LongFrameReason::Unattributed);
    }
    reasons
}

fn push_bounded(
    queue: &mut VecDeque<FrameTelemetrySample>,
    sample: FrameTelemetrySample,
    capacity: usize,
) -> bool {
    let dropped = queue.len() == capacity;
    if dropped {
        queue.pop_front();
    }
    queue.push_back(sample);
    dropped
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_guard() -> std::sync::MutexGuard<'static, ()> {
        static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        TEST_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }

    fn reset() {
        *telemetry_store().lock().unwrap() = FrameTelemetryStore::default();
    }

    #[test]
    fn long_frame_captures_queue_window_entity_cache_and_reason_snapshot() {
        let _guard = test_guard();
        reset();
        record_app_frame(AppFrameTelemetryInput {
            elapsed: Duration::from_millis(25),
            interaction: "scrollbar_drag".to_owned(),
            queues: FrameQueueSnapshot {
                pending_layout_tasks: 3,
                pending_payload_loads: 2,
                pending_saves: 1,
                scheduler_lanes_connected: false,
                realtime_lane_depth: None,
                interactive_lane_depth: None,
                visible_lane_depth: None,
                prefetch_lane_depth: None,
                background_lane_depth: None,
            },
            window: FrameWindowSnapshot {
                document_blocks: 100_000,
                payload_start: 50_000,
                payload_end: 50_320,
                page_start: 100,
                page_end: 103,
            },
            entities: FrameEntitySnapshot {
                rendered_blocks: 300,
                ..FrameEntitySnapshot::default()
            },
            caches: FrameCacheSnapshot {
                payload_cache_over_budget: true,
                ..FrameCacheSnapshot::default()
            },
            text_geometry_fallback_rate: 0.25,
        });

        let snapshot = frame_telemetry_snapshot();
        assert_eq!(snapshot.total_frames, 1);
        assert_eq!(snapshot.total_long_frames, 1);
        let frame = &snapshot.long_frames[0];
        assert!(frame.overrun_ms > 8.0);
        assert!(frame.reasons.contains(&LongFrameReason::LayoutBacklog));
        assert!(frame.reasons.contains(&LongFrameReason::EntityPressure));
        assert!(
            frame
                .reasons
                .contains(&LongFrameReason::PayloadMemoryPressure)
        );
        assert!(
            frame
                .reasons
                .contains(&LongFrameReason::TextGeometryFallback)
        );
        assert!(
            export_frame_telemetry_json()
                .unwrap()
                .contains("long_frames")
        );
    }

    #[test]
    fn telemetry_buffers_are_bounded() {
        let _guard = test_guard();
        reset();
        for _ in 0..RECENT_FRAME_CAPACITY + 5 {
            record_app_frame(AppFrameTelemetryInput {
                elapsed: Duration::from_millis(20),
                interaction: "idle".to_owned(),
                queues: FrameQueueSnapshot::default(),
                window: FrameWindowSnapshot::default(),
                entities: FrameEntitySnapshot::default(),
                caches: FrameCacheSnapshot::default(),
                text_geometry_fallback_rate: 0.0,
            });
        }
        let snapshot = frame_telemetry_snapshot();
        assert_eq!(snapshot.recent_frames.len(), RECENT_FRAME_CAPACITY);
        assert_eq!(snapshot.long_frames.len(), LONG_FRAME_CAPACITY);
        assert_eq!(snapshot.dropped_recent_frames, 5);
        assert_eq!(
            snapshot.dropped_long_frames,
            (RECENT_FRAME_CAPACITY + 5 - LONG_FRAME_CAPACITY) as u64
        );
    }
}
