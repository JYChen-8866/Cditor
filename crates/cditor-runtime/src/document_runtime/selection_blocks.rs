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
                if self.is_document_title_block(block_id) {
                    continue;
                }
                self.selection.selected_block_ids.insert(block_id);
            }
        }
        self.selection.document_selection = None;
        self.selection.focused_text_selection = None;
        self.editing.session = None;
        !self.selection.selected_block_ids.is_empty()
    }

    /// Extend a block selection across visible block boundaries, matching the
    /// same `selected_block_ids` truth used by mouse whole-block selection.
    pub(crate) fn extend_visible_block_selection(&mut self, direction: i32) -> bool {
        if direction == 0 {
            return false;
        }
        let selected_indices = self
            .selection
            .selected_block_ids
            .iter()
            .filter_map(|block_id| self.document.visible_index.visible_index_of(*block_id))
            .collect::<Vec<_>>();
        let anchor_block_id = if selected_indices.is_empty() {
            self.selection
                .document_selection
                .map(|selection| selection.anchor.block_id)
                .or_else(|| self.focused_block_id())
        } else {
            let anchor_index = if direction < 0 {
                selected_indices.iter().copied().max()
            } else {
                selected_indices.iter().copied().min()
            };
            let Some(anchor_index) = anchor_index else {
                return false;
            };
            self.document
                .visible_index
                .id_at_visible_index(anchor_index)
        };
        let Some(anchor_block_id) = anchor_block_id else {
            return false;
        };
        let current_index = if selected_indices.is_empty() {
            self.focused_block_id()
                .and_then(|block_id| self.document.visible_index.visible_index_of(block_id))
        } else if direction < 0 {
            selected_indices.iter().copied().min()
        } else {
            selected_indices.iter().copied().max()
        };
        let Some(current_index) = current_index else {
            return false;
        };
        let target_index = if direction < 0 {
            let Some(index) = current_index.checked_sub(1) else {
                return false;
            };
            index
        } else {
            current_index.saturating_add(1)
        };
        let Some(target_block_id) = self
            .document
            .visible_index
            .id_at_visible_index(target_index)
        else {
            return false;
        };
        if self.is_document_title_block(target_block_id) {
            return false;
        }
        self.select_visible_block_range(anchor_block_id, target_block_id)
    }
}
