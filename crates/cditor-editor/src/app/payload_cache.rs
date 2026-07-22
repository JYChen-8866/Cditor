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
        let CditorViewState::Ready(runtime) = &mut self.state else {
            return;
        };
        if runtime.retry_failed_payload_window(block_range) == 0 {
            return;
        }
        self.payload_window_load_scheduler.reset();
        cx.notify();
    }

    pub(in crate::app) fn trim_persistent_payload_cache(&mut self) {
        if !self.storage_persistence.is_enabled() {
            return;
        }
        let pins = self.payload_cache_ui_pins();
        let CditorViewState::Ready(runtime) = &mut self.state else {
            return;
        };
        let report = runtime.trim_payload_cache(PayloadCachePolicy::persistent_default(), pins);
        for block_id in report.evicted_block_ids {
            self.text_layouts.remove(&block_id);
            self.table_cell_layouts
                .retain(|key, _| key.block_id != block_id);
            self.text_surface_layouts
                .retain(|surface_id, _| surface_id.block_id() != Some(block_id));
        }
    }

    fn payload_cache_ui_pins(&self) -> Vec<BlockId> {
        let mut pins = HashSet::new();
        pins.extend(self.action_block_id);
        pins.extend(self.gutter_toolbar_block_id);
        pins.extend(self.code_theme_menu_block_id);
        pins.extend(self.ai_prompt.as_ref().map(|prompt| prompt.block_id));
        pins.extend(self.slash_menu.as_ref().map(|menu| menu.block_id));
        pins.extend(self.code_language_edit.as_ref().map(|edit| edit.block_id));
        pins.extend(
            self.whiteboard_editor
                .as_ref()
                .map(|session| session.block_id),
        );
        pins.extend(
            self.text_drag_selection
                .as_ref()
                .map(|drag| drag.anchor_block_id),
        );
        pins.extend(self.gutter_block_drag.as_ref().map(|drag| drag.block_id));
        pins.extend(self.image_resize_drag.as_ref().map(|drag| drag.block_id));
        pins.extend(self.table_resize_drag.as_ref().map(|drag| drag.block_id));
        pins.extend(self.table_reorder_drag.as_ref().map(|drag| drag.block_id));
        pins.extend(self.table_hscroll_drag.as_ref().map(|drag| drag.block_id));
        pins.extend(self.table_interaction_mode.block_id());
        pins.into_iter().collect()
    }
}
