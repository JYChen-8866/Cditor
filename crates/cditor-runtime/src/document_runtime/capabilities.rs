use super::*;

impl DocumentRuntime {
    pub fn can_handle_enter(&self) -> bool {
        let Some(block_id) = self.focused_block_id() else {
            return false;
        };
        match cditor_core::block::BlockKeyboardPolicy::for_kind(&self.kind_for_block(block_id))
            .enter
        {
            cditor_core::block::EnterKeyBehavior::TableCellSoftBreak => {
                self.selection.focused_table_cell.is_some()
            }
            _ => true,
        }
    }

    pub fn can_insert_soft_line_break(&self) -> bool {
        if self.selection.focused_table_cell.is_some() {
            return true;
        }
        let Some(block_id) = self.focused_block_id() else {
            return false;
        };
        cditor_core::schema::builtin_block_registry()
            .descriptor_for_kind(&self.kind_for_block(block_id))
            .capabilities
            .text_surface
    }

    /// A conversion is offered only when the source payload has a defined,
    /// non-destructive text export. Complex asset payloads keep their metadata
    /// instead of being silently flattened by a menu click.
    pub fn can_convert_block_kind(&self, block_id: BlockId, target: &RichBlockKind) -> bool {
        let Some(record) = self.document.payload_window.get(block_id) else {
            return false;
        };
        if record.kind.is_document_title() || target.is_document_title() {
            return false;
        }
        if &record.kind == target {
            return false;
        }
        // Existing tables own structured cell content and must never be
        // flattened through a block-kind conversion.
        if matches!(record.kind, RichBlockKind::Table) {
            return false;
        }
        // Table is a creation target, not a general transform target. A plain
        // paragraph may explicitly become a fresh table (for example through
        // `/table`), while headings and other semantic text blocks stay
        // incompatible with table conversion.
        if matches!(target, RichBlockKind::Table) {
            return matches!(record.kind, RichBlockKind::Paragraph)
                && matches!(&record.payload, BlockPayload::RichText { .. });
        }
        if !cditor_core::schema::builtin_block_registry()
            .descriptor_for_kind(target)
            .capabilities
            .plain_text_conversion_target
        {
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
        self.document
            .payload_window
            .get(block_id)
            .is_some_and(|record| {
                cditor_core::schema::builtin_block_registry()
                    .descriptor_for_kind(&record.kind)
                    .capabilities
                    .inline_marks
                    && matches!(&record.payload, BlockPayload::RichText { .. })
                    && !record.plain_text().is_empty()
            })
    }

    /// Mirrors the preconditions of `begin_ai_request_with_presentation`
    /// without mutating the document. Menus use this to disable commands that
    /// would otherwise open a prompt which can never be submitted.
    pub fn can_begin_ai_request(&self) -> bool {
        if self.active_composition().is_some() || !self.selection.selected_block_ids.is_empty() {
            return false;
        }
        if let Some(selection) = self
            .selection
            .document_selection
            .as_ref()
            .filter(|selection| !selection.is_caret())
        {
            let Ok(normalized) = selection.normalize(&self.document.index) else {
                return false;
            };
            let Some(start) = self.document.index.index_of(normalized.start.block_id) else {
                return false;
            };
            let Some(end) = self.document.index.index_of(normalized.end.block_id) else {
                return false;
            };
            return self.document.index.block_ids[start..=end]
                .iter()
                .all(|block_id| self.document.text_models.contains_key(block_id));
        }

        let Some(block_id) = self.focused_block_id() else {
            return false;
        };
        self.selection.focused_table_cell.is_none()
            && self.document.text_models.contains_key(&block_id)
            && self.caret_offset_for_block(block_id).is_some()
    }

    /// Mirrors `delete_block_by_id`: the final visible block can be reset, but
    /// a block owning a subtree cannot currently be deleted by this command.
    pub fn can_delete_block(&self, block_id: BlockId) -> bool {
        if self.is_document_title_block(block_id) {
            return false;
        }
        if self.document.visible_index.total_visible_count() <= 1 {
            return self.document.index.index_of(block_id).is_some();
        }
        let Some(index) = self.document.index.index_of(block_id) else {
            return false;
        };
        self.subtree_end(index) <= index + 1
    }
}
