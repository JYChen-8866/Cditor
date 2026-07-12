use super::*;

impl DocumentRuntime {
    pub fn indent_focused_block(&mut self) -> Result<bool, String> {
        let Some(block_id) = self.focused_block_id() else {
            return Ok(false);
        };
        let kind = self.kind_for_block(block_id);
        if uses_soft_tab(&kind) {
            return self.insert_soft_tab_in_focused_block();
        }
        self.indent_block(block_id)
    }

    pub fn outdent_focused_block(&mut self) -> Result<bool, String> {
        let Some(block_id) = self.focused_block_id() else {
            return Ok(false);
        };
        let kind = self.kind_for_block(block_id);
        if uses_soft_tab(&kind) {
            return self.outdent_soft_tab_in_focused_block();
        }
        self.outdent_block(block_id)
    }

    pub fn indent_block(&mut self, block_id: BlockId) -> Result<bool, String> {
        let Some(index) = self.index.index_of(block_id) else {
            return Ok(false);
        };
        let parent_id = self.index.parent_ids[index];
        let Some(sibling_index) = self.direct_child_position(parent_id, block_id) else {
            return Ok(false);
        };
        if sibling_index == 0 {
            return Ok(false);
        }
        let siblings = self.direct_children(parent_id);
        let Some(previous_sibling_id) = siblings.get(sibling_index - 1).copied() else {
            return Ok(false);
        };
        let Some(previous_sibling_index) = self.index.index_of(previous_sibling_id) else {
            return Ok(false);
        };
        let previous_kind = self.kind_at_index(previous_sibling_index);
        if !cditor_core::block::supports_list_children(&previous_kind) {
            return Ok(false);
        }
        let child_count = self.direct_children(Some(previous_sibling_id)).len();
        self.move_block_subtree_to_parent(block_id, Some(previous_sibling_id), child_count)
    }

    pub fn outdent_block(&mut self, block_id: BlockId) -> Result<bool, String> {
        let Some(index) = self.index.index_of(block_id) else {
            return Ok(false);
        };
        let Some(parent_id) = self.index.parent_ids[index] else {
            return Ok(false);
        };
        let Some(parent_index) = self.index.index_of(parent_id) else {
            return Ok(false);
        };
        let grandparent_id = self.index.parent_ids[parent_index];
        let Some(parent_sibling_index) = self.direct_child_position(grandparent_id, parent_id)
        else {
            return Ok(false);
        };
        self.move_block_subtree_to_parent(block_id, grandparent_id, parent_sibling_index + 1)
    }
}
