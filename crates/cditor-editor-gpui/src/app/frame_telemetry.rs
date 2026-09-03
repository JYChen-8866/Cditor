use std::time::Duration;

use crate::diagnostics::frame_telemetry::{
    AppFrameTelemetryInput, FrameCacheSnapshot, FrameEntitySnapshot, FrameQueueSnapshot,
    FrameWindowSnapshot, record_app_frame,
};
use crate::editor_view::CditorV2View;
use cditor_viewport::scroll::ScrollInteractionState;
use gpui::WindowId;

impl CditorV2View {
    pub(crate) fn record_frame_telemetry(&self, window_id: WindowId, elapsed: Duration) {
        let scheduler_depths = self.scheduling.main_thread.lane_depths();
        let scheduler_pending = self.scheduling.main_thread.pending_len();
        let diagnostics = self
            .ready_session()
            .and_then(|session| session.diagnostics_snapshot().ok());
        let interaction = frame_interaction_label(
            self.interaction.scrollbar_drag.is_some(),
            self.interaction.gutter_block_drag.is_some()
                || self.interaction.table_interaction_mode.is_dragging()
                || self.interaction.image_resize_drag.is_some()
                || self.interaction.table_resize_drag.is_some(),
            diagnostics
                .as_ref()
                .is_some_and(|snapshot| snapshot.editing_active),
            self.interaction.scroll_accumulator.interaction_state,
        );
        let (queues, window, entities, payload_and_undo_bytes, payload_over_budget) = diagnostics
            .map(|snapshot| {
                (
                    FrameQueueSnapshot {
                        pending_layout_tasks: snapshot
                            .pending_layout_tasks
                            .saturating_add(scheduler_pending),
                        pending_payload_loads: snapshot.pending_payload_loads,
                        pending_saves: self
                            .ready_session()
                            .and_then(|session| session.persistence_snapshot().ok())
                            .map_or(0, |snapshot| snapshot.pending_operations),
                        scheduler_lanes_connected: true,
                        realtime_lane_depth: Some(scheduler_depths.realtime),
                        interactive_lane_depth: Some(scheduler_depths.interactive),
                        visible_lane_depth: Some(scheduler_depths.visible),
                        prefetch_lane_depth: Some(scheduler_depths.prefetch),
                        background_lane_depth: Some(scheduler_depths.background),
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
        let text_layout_cache = crate::text::text_layout_cache_stats();
        let platform_geometry_bytes = self
            .cache
            .text_layouts
            .estimated_metadata_bytes()
            .saturating_add(self.cache.table_cell_layouts.estimated_metadata_bytes())
            .saturating_add(self.cache.text_surface_layouts.estimated_metadata_bytes());
        let platform_layout_bytes = text_layout_cache
            .estimated_bytes
            .saturating_add(platform_geometry_bytes);
        let images = crate::image_loader::image_cache_diagnostics();
        let mermaid = self.cache.mermaid_renders.diagnostics();
        let video = self.cache.video_playbacks.diagnostics();
        let input = AppFrameTelemetryInput {
            elapsed,
            interaction,
            queues,
            window,
            entities,
            caches: FrameCacheSnapshot {
                payload_and_undo_bytes,
                platform_layout_bytes,
                image_cache_entries: images.tracked_entries,
                image_resident_decoded_bytes: images.resident_decoded_bytes,
                mermaid_cache_entries: mermaid.tracked_entries,
                mermaid_resident_image_bytes: mermaid.resident_image_bytes,
                mermaid_reserved_render_bytes: mermaid.reserved_render_bytes,
                video_resident_cpu_frame_bytes: video.resident_cpu_frame_bytes,
                video_resident_render_image_bytes: video.resident_render_image_bytes,
                image_cache_over_budget: images.resident_decoded_bytes > images.decoded_byte_budget,
                mermaid_cache_over_budget: mermaid
                    .resident_image_bytes
                    .saturating_add(mermaid.reserved_render_bytes)
                    > mermaid.render_byte_budget,
                payload_cache_over_budget: payload_over_budget,
                platform_layout_cache_over_budget: text_layout_cache.over_budget_due_to_pins
                    || self.cache.text_layouts.is_over_budget()
                    || self.cache.table_cell_layouts.is_over_budget()
                    || self.cache.text_surface_layouts.is_over_budget(),
            },
            text_geometry_fallback_rate: crate::text::text_geometry_telemetry().fallback_rate(),
        };
        crate::diagnostics::fps_trace::trace_gpui_frames(window_id, &input);
        record_app_frame(input);
    }
}

fn frame_interaction_label(
    scrollbar_dragging: bool,
    dragging: bool,
    editing: bool,
    scroll: ScrollInteractionState,
) -> String {
    if scrollbar_dragging {
        "scrollbar_drag".to_owned()
    } else if dragging {
        "drag".to_owned()
    } else if scroll != ScrollInteractionState::Idle {
        format!("{scroll:?}").to_lowercase()
    } else if editing {
        "editing".to_owned()
    } else {
        "idle".to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrolling_is_not_hidden_by_an_active_editing_session() {
        assert_eq!(
            frame_interaction_label(false, false, true, ScrollInteractionState::WheelActive),
            "wheelactive"
        );
        assert_eq!(
            frame_interaction_label(false, false, true, ScrollInteractionState::Momentum),
            "momentum"
        );
    }

    #[test]
    fn direct_drags_keep_priority_over_scroll_and_editing() {
        assert_eq!(
            frame_interaction_label(true, true, true, ScrollInteractionState::WheelActive),
            "scrollbar_drag"
        );
        assert_eq!(
            frame_interaction_label(false, true, true, ScrollInteractionState::WheelActive),
            "drag"
        );
        assert_eq!(
            frame_interaction_label(false, false, true, ScrollInteractionState::Idle),
            "editing"
        );
    }
}
