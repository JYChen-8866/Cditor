use std::collections::HashSet;
use std::ops::Range;

use cditor_core::ids::BlockId;
use cditor_runtime::PayloadCachePolicy;
use gpui::Context;

use super::cditor_v2_view::{CditorV2View, CditorViewState};

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

    pub(in crate::app) fn trim_persistent_payload_cache(&mut self) {
        if !self.ready_session().is_some_and(|session| {
            session
                .persistence_snapshot()
                .is_ok_and(|snapshot| snapshot.enabled)
        }) {
            return;
        }
        let pins = self.payload_cache_ui_pins();
        let CditorViewState::Ready(session) = &self.state else {
            return;
        };
        let Ok(report) = session.trim_payload_cache(PayloadCachePolicy::persistent_default(), pins)
        else {
            return;
        };
        for block_id in report.evicted_block_ids {
            self.cache.text_layouts.remove(&block_id);
            self.cache
                .table_cell_layouts
                .retain(|key, _| key.block_id != block_id);
            self.cache
                .text_surface_layouts
                .retain(|surface_id, _| surface_id.block_id() != Some(block_id));
        }
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
        pins.extend(
            self.interaction
                .table_hscroll_drag
                .as_ref()
                .map(|drag| drag.block_id),
        );
        pins.extend(self.interaction.table_interaction_mode.block_id());
        pins.into_iter().collect()
    }
}
