use super::*;

impl DocumentRuntime {
    #[cfg(test)]
    pub(crate) fn select_all_visible_blocks(&mut self) -> bool {
        self.break_typing_coalescing();
        self.selection.focused_table_cell = None;
        self.selection.selected_block_ids = self
            .document
            .visible_index
            .visible_block_ids
            .iter()
            .copied()
            .collect();
        true
    }

    pub fn has_selected_blocks(&self) -> bool {
        !self.selection.selected_block_ids.is_empty()
    }

    pub(crate) fn delete_selected_block_selection(&mut self) -> Result<bool, String> {
        self.delete_selected_blocks()
    }

    pub(crate) fn select_visible_block_range(&mut self, anchor: BlockId, focus: BlockId) -> bool {
        self.break_typing_coalescing();
        let Some(anchor_index) = self.document.visible_index.visible_index_of(anchor) else {
            return false;
        };
        let Some(focus_index) = self.document.visible_index.visible_index_of(focus) else {
            return false;
        };
        let start = anchor_index.min(focus_index);
        let end = anchor_index.max(focus_index);
        self.selection.focused_table_cell = None;
        self.selection.selected_block_ids.clear();
        for index in start..=end {
            if let Some(block_id) = self.document.visible_index.id_at_visible_index(index) {
                self.selection.selected_block_ids.insert(block_id);
            }
        }
        self.editing = None;
        true
    }
}
