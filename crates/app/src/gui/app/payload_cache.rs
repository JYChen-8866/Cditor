use std::collections::HashSet;

use cditor_core::ids::BlockId;
use cditor_runtime::PayloadCachePolicy;

use super::cditor_v2_view::{CditorV2View, CditorViewState};

impl CditorV2View {
    pub(in crate::gui::app) fn trim_postgres_payload_cache(&mut self) {
        if !self.postgres_persistence.is_enabled() {
            return;
        }
        let pins = self.payload_cache_ui_pins();
        let CditorViewState::Ready(runtime) = &mut self.state else {
            return;
        };
        let report = runtime.trim_payload_cache(PayloadCachePolicy::postgres_default(), pins);
        for block_id in report.evicted_block_ids {
            self.text_layouts.remove(&block_id);
            self.table_cell_layouts
                .retain(|key, _| key.block_id != block_id);
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
