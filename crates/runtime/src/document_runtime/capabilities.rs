use super::*;

impl DocumentRuntime {
    /// A conversion is offered only when the source payload has a defined,
    /// non-destructive text export. Complex asset payloads keep their metadata
    /// instead of being silently flattened by a menu click.
    pub fn can_convert_block_kind(&self, block_id: BlockId, target: &RichBlockKind) -> bool {
        let Some(record) = self.payload_window.get(block_id) else {
            return false;
        };
        if &record.kind == target {
            return false;
        }
        matches!(
            &record.payload,
            BlockPayload::RichText { .. }
                | BlockPayload::Code { .. }
                | BlockPayload::Table(_)
                | BlockPayload::Html { .. }
        ) || (matches!(&record.payload, BlockPayload::Empty)
            && matches!(
                record.kind,
                RichBlockKind::Divider | RichBlockKind::Separator
            ))
    }

    /// Whether the current implementation can apply rich inline marks/colors to
    /// the complete contents of `block_id` from a block action menu.
    pub fn supports_block_rich_text_actions(&self, block_id: BlockId) -> bool {
        self.payload_window.get(block_id).is_some_and(|record| {
            matches!(&record.payload, BlockPayload::RichText { .. })
                && !record.plain_text().is_empty()
        })
    }

    /// Mirrors the preconditions of `begin_ai_request_with_presentation`
    /// without mutating the document. Menus use this to disable commands that
    /// would otherwise open a prompt which can never be submitted.
    pub fn can_begin_ai_request(&self) -> bool {
        if self.active_composition().is_some() || !self.selected_block_ids.is_empty() {
            return false;
        }
        if let Some(selection) = self
            .document_selection
            .as_ref()
            .filter(|selection| !selection.is_caret())
        {
            let Ok(normalized) = selection.normalize(&self.index) else {
                return false;
            };
            let Some(start) = self.index.index_of(normalized.start.block_id) else {
                return false;
            };
            let Some(end) = self.index.index_of(normalized.end.block_id) else {
                return false;
            };
            return self.index.block_ids[start..=end]
                .iter()
                .all(|block_id| self.text_models.contains_key(block_id));
        }

        let Some(block_id) = self.focused_block_id() else {
            return false;
        };
        self.focused_table_cell.is_none()
            && self.text_models.contains_key(&block_id)
            && self.caret_offset_for_block(block_id).is_some()
    }

    /// Mirrors `delete_block_by_id`: the final visible block can be reset, but
    /// a block owning a subtree cannot currently be deleted by this command.
    pub fn can_delete_block(&self, block_id: BlockId) -> bool {
        if self.visible_index.total_visible_count() <= 1 {
            return self.index.index_of(block_id).is_some();
        }
        let Some(index) = self.index.index_of(block_id) else {
            return false;
        };
        self.subtree_end(index) <= index + 1
    }
}
