use std::time::Duration;

use super::CditorV2View;
use crate::diagnostics::frame_telemetry::{
    AppFrameTelemetryInput, FrameCacheSnapshot, FrameEntitySnapshot, FrameQueueSnapshot,
    FrameWindowSnapshot, record_app_frame,
};

impl CditorV2View {
    pub(in crate::app) fn record_frame_telemetry(&self, elapsed: Duration) {
        let interaction = if self.scrollbar_drag.is_some() {
            "scrollbar_drag".to_owned()
        } else if self.gutter_block_drag.is_some()
            || self.table_interaction_mode.is_dragging()
            || self.image_resize_drag.is_some()
            || self.table_resize_drag.is_some()
        {
            "drag".to_owned()
        } else if self
            .ready_runtime_ref()
            .is_some_and(|runtime| runtime.input_session_target().is_some())
        {
            "editing".to_owned()
        } else {
            format!("{:?}", self.scroll_accumulator.interaction_state).to_lowercase()
        };
        let (queues, window, entities, payload_and_undo_bytes, payload_over_budget) = self
            .ready_runtime_ref()
            .map(|runtime| {
                let payload_range = runtime.payload_window.block_range.clone();
                let page_range = runtime.current_page_window();
                let resident_bytes = runtime
                    .estimated_payload_memory_bytes()
                    .saturating_add(runtime.estimated_text_undo_memory_bytes());
                (
                    FrameQueueSnapshot {
                        pending_layout_tasks: runtime.pending_layout_task_count(),
                        pending_payload_loads: runtime.payload_window.loading.len(),
                        pending_saves: self.storage_persistence.pending_operation_count(),
                        // P6-006 keeps this false until all GPUI dispatch is routed
                        // through the five production scheduler lanes.
                        scheduler_lanes_connected: false,
                        realtime_lane_depth: None,
                        interactive_lane_depth: None,
                        visible_lane_depth: None,
                        prefetch_lane_depth: None,
                        background_lane_depth: None,
                    },
                    FrameWindowSnapshot {
                        document_blocks: runtime.document_block_count(),
                        payload_start: payload_range.start,
                        payload_end: payload_range.end,
                        page_start: page_range.start,
                        page_end: page_range.end,
                    },
                    FrameEntitySnapshot {
                        rendered_blocks: self.projected_block_rects.len(),
                        loaded_payloads: runtime.loaded_payload_count(),
                        block_layouts: self.text_layouts.len(),
                        table_cell_layouts: self.table_cell_layouts.len(),
                        auxiliary_layouts: self.text_surface_layouts.len(),
                    },
                    resident_bytes,
                    runtime.loaded_payload_count()
                        > cditor_runtime::DEFAULT_POSTGRES_PAYLOAD_CACHE_MAX_ENTRIES
                        || runtime.estimated_payload_memory_bytes()
                            > cditor_runtime::DEFAULT_POSTGRES_PAYLOAD_CACHE_MAX_BYTES,
                )
            })
            .unwrap_or_default();
        let platform_layout_bytes = self
            .text_layouts
            .estimated_bytes()
            .saturating_add(self.table_cell_layouts.estimated_bytes())
            .saturating_add(self.text_surface_layouts.estimated_bytes());
        record_app_frame(AppFrameTelemetryInput {
            elapsed,
            interaction,
            queues,
            window,
            entities,
            caches: FrameCacheSnapshot {
                payload_and_undo_bytes,
                platform_layout_bytes,
                payload_cache_over_budget: payload_over_budget,
                platform_layout_cache_over_budget: self.text_layouts.is_over_budget()
                    || self.table_cell_layouts.is_over_budget()
                    || self.text_surface_layouts.is_over_budget(),
            },
            text_geometry_fallback_rate: crate::text::text_geometry_telemetry().fallback_rate(),
        });
    }
}
