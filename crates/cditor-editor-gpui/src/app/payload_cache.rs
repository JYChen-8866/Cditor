use std::collections::HashSet;
use std::ops::Range;
use std::time::Duration;

use cditor_core::ids::BlockId;
use cditor_runtime::{InteractionMode, PayloadCacheMaintenanceBudget, PayloadCachePolicy};
use gpui::Context;

use crate::editor_view::{CditorV2View, CditorViewState};

const PAYLOAD_CACHE_TRIM_IDLE_DELAY: Duration = Duration::from_millis(150);
const PAYLOAD_CACHE_MAINTENANCE_SLICE_YIELD: Duration = Duration::from_millis(1);

impl CditorV2View {
    pub(crate) fn retry_payload_window(
        &mut self,
        block_range: Range<usize>,
        cx: &mut Context<Self>,
    ) {
        let CditorViewState::Ready(session) = &self.state else {
            return;
        };
        if session
            .retry_failed_payload_window(block_range)
            .unwrap_or_default()
            == 0
        {
            return;
        }
        let _ = session.reset_payload_window_tasks();
        cx.notify();
    }

    pub(crate) fn trim_persistent_payload_cache(&mut self) -> bool {
        if !self.ready_session().is_some_and(|session| {
            session
                .persistence_snapshot()
                .is_ok_and(|snapshot| snapshot.enabled)
        }) {
            return false;
        }
        let pins = self.payload_cache_ui_pins();
        let CditorViewState::Ready(session) = &self.state else {
            return false;
        };
        let Ok(report) = session.maintain_payload_cache(
            PayloadCachePolicy::persistent_default(),
            pins,
            PayloadCacheMaintenanceBudget::idle_slice(),
        ) else {
            return false;
        };
        let maintenance_pending = report.maintenance_pending;
        let evicted_block_ids = report.evicted_block_ids.into_iter().collect::<HashSet<_>>();
        if evicted_block_ids.is_empty() {
            return maintenance_pending;
        }
        for block_id in &evicted_block_ids {
            self.cache.text_layouts.remove(block_id);
        }
        self.cache
            .table_cell_layouts
            .retain(|key, _| !evicted_block_ids.contains(&key.block_id));
        self.cache.text_surface_layouts.retain(|surface_id, _| {
            surface_id
                .block_id()
                .is_none_or(|block_id| !evicted_block_ids.contains(&block_id))
        });
        maintenance_pending
    }

    pub(crate) fn schedule_persistent_payload_cache_trim(&mut self, cx: &mut Context<Self>) {
        self.schedule_payload_cache_maintenance_after(PAYLOAD_CACHE_TRIM_IDLE_DELAY, cx);
    }

    fn schedule_payload_cache_maintenance_after(
        &mut self,
        delay: Duration,
        cx: &mut Context<Self>,
    ) {
        if !self.ready_session().is_some_and(|session| {
            session
                .persistence_snapshot()
                .is_ok_and(|snapshot| snapshot.enabled)
        }) || !self.scheduling.request_payload_cache_trim()
        {
            return;
        }

        let wake = cx.background_executor().timer(delay);
        cx.spawn(async move |view, cx| {
            wake.await;
            let _ = view.update(cx, |view, cx| {
                let follow_up_requested = view.scheduling.finish_payload_cache_trim_wait();
                let idle = view.payload_cache_maintenance_is_idle();
                let maintenance_pending = idle && view.trim_persistent_payload_cache();
                if let Some(delay) =
                    payload_cache_follow_up_delay(idle, maintenance_pending, follow_up_requested)
                {
                    view.schedule_payload_cache_maintenance_after(delay, cx);
                }
            });
        })
        .detach();
    }

    fn payload_cache_maintenance_is_idle(&self) -> bool {
        payload_cache_trim_allowed(
            self.main_thread_interaction_mode(),
            self.interaction.block_drag_selection.is_dragging()
                || self.interaction.gutter_block_drag.is_some()
                || self.interaction.gutter_drag_auto_scroll_scheduled
                || self.interaction.image_resize_drag.is_some()
                || self.interaction.table_resize_drag.is_some()
                || self.interaction.table_reorder_drag.is_some()
                || self.interaction.table_interaction_mode.is_dragging(),
        )
    }

    fn payload_cache_ui_pins(&self) -> Vec<BlockId> {
        let mut pins = HashSet::new();
        pins.extend(self.interaction.action_block_id);
        pins.extend(self.overlay.gutter_toolbar_block_id);
        pins.extend(self.overlay.code_theme_menu_block_id);
        pins.extend(
            self.overlay
                .ai_prompt
                .as_ref()
                .map(|prompt| prompt.block_id),
        );
        pins.extend(self.overlay.slash_menu.as_ref().map(|menu| menu.block_id));
        pins.extend(
            self.overlay
                .code_language_edit
                .as_ref()
                .map(|edit| edit.block_id),
        );
        pins.extend(
            self.features
                .whiteboard_editor
                .as_ref()
                .map(|session| session.block_id),
        );
        pins.extend(
            self.interaction
                .text_drag_selection
                .as_ref()
                .map(|drag| drag.anchor_block_id),
        );
        pins.extend(
            self.interaction
                .gutter_block_drag
                .as_ref()
                .map(|drag| drag.block_id),
        );
        pins.extend(
            self.interaction
                .image_resize_drag
                .as_ref()
                .map(|drag| drag.block_id),
        );
        pins.extend(
            self.interaction
                .table_resize_drag
                .as_ref()
                .map(|drag| drag.block_id),
        );
        pins.extend(
            self.interaction
                .table_reorder_drag
                .as_ref()
                .map(|drag| drag.block_id),
        );
        pins.extend(self.interaction.table_interaction_mode.block_id());
        pins.into_iter().collect()
    }
}

fn payload_cache_trim_allowed(mode: InteractionMode, document_drag_active: bool) -> bool {
    mode == InteractionMode::Idle && !document_drag_active
}

fn payload_cache_follow_up_delay(
    editor_is_idle: bool,
    maintenance_pending: bool,
    follow_up_requested: bool,
) -> Option<Duration> {
    if !editor_is_idle {
        return Some(PAYLOAD_CACHE_TRIM_IDLE_DELAY);
    }
    (maintenance_pending || follow_up_requested).then_some(PAYLOAD_CACHE_MAINTENANCE_SLICE_YIELD)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_cache_trim_requires_a_fully_idle_editor() {
        assert!(payload_cache_trim_allowed(InteractionMode::Idle, false));
        assert!(!payload_cache_trim_allowed(InteractionMode::Idle, true));
        for mode in [
            InteractionMode::Typing,
            InteractionMode::Composing,
            InteractionMode::WheelScrolling,
            InteractionMode::ScrollbarDragging,
            InteractionMode::Selecting,
            InteractionMode::Pasting,
        ] {
            assert!(!payload_cache_trim_allowed(mode, false), "{mode:?}");
        }
    }

    #[test]
    fn payload_cache_slices_yield_but_blocked_capacity_does_not_spin() {
        assert_eq!(
            payload_cache_follow_up_delay(false, true, false),
            Some(PAYLOAD_CACHE_TRIM_IDLE_DELAY)
        );
        assert_eq!(
            payload_cache_follow_up_delay(true, true, false),
            Some(PAYLOAD_CACHE_MAINTENANCE_SLICE_YIELD)
        );
        assert_eq!(
            payload_cache_follow_up_delay(true, false, true),
            Some(PAYLOAD_CACHE_MAINTENANCE_SLICE_YIELD)
        );
        assert_eq!(payload_cache_follow_up_delay(true, false, false), None);
    }
}
