use std::time::Duration;

use crate::diagnostics::frame_telemetry::{
    AppFrameTelemetryInput, FrameCacheSnapshot, FrameEntitySnapshot, FrameQueueSnapshot,
    FrameWindowSnapshot, record_app_frame,
};
use crate::editor_view::CditorV2View;

impl CditorV2View {
    pub(crate) fn record_frame_telemetry(&self, elapsed: Duration) {
        let diagnostics = self
            .ready_session()
            .and_then(|session| session.diagnostics_snapshot().ok());
        let interaction = if self.interaction.scrollbar_drag.is_some() {
            "scrollbar_drag".to_owned()
        } else if self.interaction.gutter_block_drag.is_some()
            || self.interaction.table_interaction_mode.is_dragging()
            || self.interaction.image_resize_drag.is_some()
            || self.interaction.table_resize_drag.is_some()
        {
            "drag".to_owned()
        } else if diagnostics
            .as_ref()
            .is_some_and(|snapshot| snapshot.editing_active)
        {
            "editing".to_owned()
        } else {
            format!(
                "{:?}",
                self.interaction.scroll_accumulator.interaction_state
            )
            .to_lowercase()
        };
        let (queues, window, entities, payload_and_undo_bytes, payload_over_budget) = diagnostics
            .map(|snapshot| {
                (
                    FrameQueueSnapshot {
                        pending_layout_tasks: snapshot.pending_layout_tasks,
                        pending_payload_loads: snapshot.pending_payload_loads,
                        pending_saves: self
                            .ready_session()
                            .and_then(|session| session.persistence_snapshot().ok())
                            .map_or(0, |snapshot| snapshot.pending_operations),
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
                        document_blocks: snapshot.document_blocks,
                        payload_start: snapshot.payload_window.start,
                        payload_end: snapshot.payload_window.end,
                        page_start: snapshot.page_window.start,
                        page_end: snapshot.page_window.end,
                    },
                    FrameEntitySnapshot {
                        rendered_blocks: self.interaction.projected_block_rects.len(),
                        loaded_payloads: snapshot.loaded_payloads,
                        block_layouts: self.cache.text_layouts.len(),
                        table_cell_layouts: self.cache.table_cell_layouts.len(),
                        auxiliary_layouts: self.cache.text_surface_layouts.len(),
                    },
                    snapshot.payload_and_undo_bytes,
                    snapshot.payload_cache_over_budget,
                )
            })
            .unwrap_or_default();
        let platform_layout_bytes = self
            .cache
            .text_layouts
            .estimated_bytes()
            .saturating_add(self.cache.table_cell_layouts.estimated_bytes())
            .saturating_add(self.cache.text_surface_layouts.estimated_bytes());
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
                platform_layout_cache_over_budget: self.cache.text_layouts.is_over_budget()
                    || self.cache.table_cell_layouts.is_over_budget()
                    || self.cache.text_surface_layouts.is_over_budget(),
            },
            text_geometry_fallback_rate: crate::text::text_geometry_telemetry().fallback_rate(),
        });
    }
}
