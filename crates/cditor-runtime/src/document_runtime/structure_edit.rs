use super::markdown_transaction::{
    editable_payload_spans, payload_for_kind_from_spans, payload_replace_operation,
};
use super::structure_payload::payload_for_converted_kind;
use super::*;

impl DocumentRuntime {
    pub(crate) fn convert_focused_block_kind(
        &mut self,
        kind: RichBlockKind,
    ) -> Result<bool, String> {
        let Some(block_id) = self.focused_block_id() else {
            return Ok(false);
        };
        let Some(record) = self.payload_window.get(block_id).cloned() else {
            return Ok(false);
        };
        if record.kind == kind {
            return Ok(false);
        }
        if !self.can_convert_block_kind(block_id, &kind) {
            return Ok(false);
        }
        let text = record.plain_text();
        let payload = payload_for_converted_kind(&kind, text);
        self.apply_local_block_payload_transaction(
            block_id,
            EditTransactionKind::ExplicitCommand,
            kind,
            payload,
        )
    }

    pub(crate) fn set_code_block_language(
        &mut self,
        block_id: BlockId,
        language: Option<String>,
    ) -> Result<bool, String> {
        let language = language.and_then(|language| {
            let trimmed = language.trim().to_lowercase();
            (!trimmed.is_empty()).then_some(trimmed)
        });
        let Some(record) = self.payload_window.get(block_id).cloned() else {
            return Ok(false);
        };
        let BlockPayload::Code { text, .. } = record.payload else {
            return Ok(false);
        };
        if matches!(&record.kind, RichBlockKind::Code { language: current } if current == &language)
            && matches!(self.payload_window.get(block_id).map(|record| &record.payload), Some(BlockPayload::Code { language: current, .. }) if current == &language)
        {
            return Ok(false);
        }
        self.apply_local_block_payload_transaction(
            block_id,
            EditTransactionKind::ExplicitCommand,
            RichBlockKind::Code {
                language: language.clone(),
            },
            BlockPayload::Code { language, text },
        )
    }

    pub(crate) fn toggle_todo_checked(&mut self, block_id: BlockId) -> Result<bool, String> {
        let Some(record) = self.payload_window.get(block_id).cloned() else {
            return Ok(false);
        };
        let checked = match &record.kind {
            RichBlockKind::Todo { checked } => *checked,
            _ => return Ok(false),
        };
        self.apply_local_block_payload_transaction(
            block_id,
            EditTransactionKind::ExplicitCommand,
            RichBlockKind::Todo { checked: !checked },
            record.payload,
        )
    }

    pub(crate) fn handle_enter(&mut self) -> Result<(), String> {
        let Some(block_id) = self.focused_block_id() else {
            self.insert_paragraph_after_focused()?;
            return Ok(());
        };
        if self.handle_auxiliary_text_surface_enter(block_id)? {
            return Ok(());
        }

        // Get block kind and input capability
        let kind = self
            .payload_window
            .get(block_id)
            .map(|payload| payload.kind.clone())
            .unwrap_or_else(|| RichBlockKind::Paragraph);

        if let RichBlockKind::Heading { level } = &kind
            && self.visible_index.is_folded(block_id)
        {
            self.insert_heading_after_folded_section(block_id, *level)?;
            return Ok(());
        }

        let keyboard_policy = cditor_core::block::BlockKeyboardPolicy::for_kind(&kind);
        if matches!(
            keyboard_policy.enter,
            cditor_core::block::EnterKeyBehavior::InsertParagraphAfter
        ) {
            trace_input(
                "handle_enter_complex_block",
                format_args!("block={block_id} kind={kind:?} - inserting paragraph after"),
            );
            self.insert_paragraph_after_block(block_id)?;
            return Ok(());
        }
        if matches!(
            keyboard_policy.enter,
            cditor_core::block::EnterKeyBehavior::TableCellSoftBreak
        ) {
            if self.focused_table_cell.is_some() {
                self.insert_soft_line_break()?;
            }
            return Ok(());
        }

        // Get text for shortcut detection
        let text = self
            .text_models
            .get(&block_id)
            .map(|model| model.text().to_owned())
            .unwrap_or_default();

        // Handle code fence shortcut
        if matches!(kind, RichBlockKind::Paragraph)
            && let Some(RichBlockKind::Code { language }) = code_fence_shortcut(&text)
        {
            self.apply_local_block_payload_transaction(
                block_id,
                EditTransactionKind::ExplicitCommand,
                RichBlockKind::Code {
                    language: language.clone(),
                },
                BlockPayload::Code {
                    language,
                    text: String::new(),
                },
            )?;
            return Ok(());
        }

        // Handle list item empty state
        if cditor_core::block::is_list_item_kind(&kind) && text.trim().is_empty() {
            let depth = self
                .index
                .index_of(block_id)
                .and_then(|index| self.index.depths.get(index).copied())
                .unwrap_or_default();
            if depth == 0 {
                self.apply_local_block_payload_transaction(
                    block_id,
                    EditTransactionKind::BlockStructureChange,
                    RichBlockKind::Paragraph,
                    BlockPayload::RichText { spans: Vec::new() },
                )?;
            } else {
                let _ = self.outdent_block(block_id)?;
            }
            return Ok(());
        }

        match keyboard_policy.enter {
            cditor_core::block::EnterKeyBehavior::InsertSoftBreak => {
                self.insert_soft_line_break()?;
                self.refresh_focused_text_block_height()?;
            }
            cditor_core::block::EnterKeyBehavior::SplitText => {
                self.split_focused_block_at_caret(EnterSplitMode::InheritV1Kind)?;
            }
            cditor_core::block::EnterKeyBehavior::TableCellSoftBreak
            | cditor_core::block::EnterKeyBehavior::InsertParagraphAfter => {
                unreachable!("handled before text shortcut processing")
            }
        }
        Ok(())
    }

    pub fn pending_structure_transaction_count(&self) -> usize {
        self.pending_structure_transactions.len()
    }

    pub fn structure_version(&self) -> u64 {
        self.index.structure_version
    }

    pub fn index_records_snapshot(&self) -> Vec<BlockIndexRecord> {
        self.index_records()
    }

    pub fn loaded_payload_records_snapshot(&self) -> Vec<BlockPayloadRecord> {
        self.payload_window
            .payloads
            .values()
            .cloned()
            .map(|payload| self.table_runtime_payload_record(payload.block_id, payload))
            .collect()
    }

    pub fn drain_pending_structure_transactions(&mut self) -> Vec<EditTransaction> {
        self.pending_structure_transactions.drain(..).collect()
    }

    pub fn restore_pending_structure_transactions(
        &mut self,
        mut transactions: Vec<EditTransaction>,
    ) {
        if transactions.is_empty() {
            return;
        }
        transactions.append(&mut self.pending_structure_transactions);
        self.pending_structure_transactions = transactions;
    }

    pub(crate) fn move_block_subtree_before(
        &mut self,
        block_id: BlockId,
        before_block_id: Option<BlockId>,
    ) -> Result<bool, String> {
        let Some(source_start) = self.index.index_of(block_id) else {
            return Ok(false);
        };
        let source_parent = self.index.parent_ids[source_start];
        let source_sibling_index = self.direct_child_position(source_parent, block_id);
        let target_parent = before_block_id
            .and_then(|before_block_id| self.index.index_of(before_block_id))
            .map(|index| self.index.parent_ids[index])
            .unwrap_or(source_parent);
        let sibling_index = match before_block_id {
            Some(before_block_id) => {
                let before_position = self
                    .direct_child_position(target_parent, before_block_id)
                    .unwrap_or_else(|| self.direct_children(target_parent).len());
                if target_parent == source_parent
                    && source_sibling_index.is_some_and(|source| source < before_position)
                {
                    before_position.saturating_sub(1)
                } else {
                    before_position
                }
            }
            None => {
                let len = self.direct_children(target_parent).len();
                if target_parent == source_parent {
                    len.saturating_sub(1)
                } else {
                    len
                }
            }
        };
        self.move_block_subtree_to_parent(block_id, target_parent, sibling_index)
    }

    pub(crate) fn move_block_subtree_to_parent(
        &mut self,
        block_id: BlockId,
        new_parent_id: Option<BlockId>,
        sibling_index: usize,
    ) -> Result<bool, String> {
        let before_selection = self.document_selection_snapshot();
        let before_selected_blocks = self.selected_block_ids_snapshot();
        let Some(source_start) = self.index.index_of(block_id) else {
            return Ok(false);
        };
        let old_parent_id = self.index.parent_ids[source_start];
        let Some(old_sibling_index) = self.direct_child_position(old_parent_id, block_id) else {
            return Ok(false);
        };
        if old_parent_id == new_parent_id && old_sibling_index == sibling_index {
            return Ok(false);
        }
        let source_end = self.subtree_end(source_start);
        if let Some(parent_id) = new_parent_id {
            let Some(parent_index) = self.index.index_of(parent_id) else {
                return Ok(false);
            };
            if (source_start..source_end).contains(&parent_index)
                || !cditor_core::block::supports_list_children(&self.kind_at_index(parent_index))
            {
                return Ok(false);
            }
        }
        self.apply_local_structure_transaction(
            EditTransactionKind::BlockStructureChange,
            cditor_core::edit::ChangeOrigin::User,
            vec![EditOperation::MoveBlockToParent {
                block_id,
                parent_id: new_parent_id,
                sibling_index,
            }],
            vec![EditOperation::MoveBlockToParent {
                block_id,
                parent_id: old_parent_id,
                sibling_index: old_sibling_index,
            }],
            vec![TransactionPrecondition::StructureVersion(
                self.structure_version(),
            )],
            before_selection,
            before_selection,
            before_selected_blocks.clone(),
            before_selected_blocks,
        )
    }

    pub(super) fn replace_preapplied_block_kind_and_spans(
        &mut self,
        block_id: BlockId,
        kind: RichBlockKind,
        spans: Vec<InlineSpan>,
    ) -> Result<(), String> {
        self.replace_block_kind_and_payload_internal(
            block_id,
            kind,
            BlockPayload::RichText { spans },
            false,
        )
    }

    pub(super) fn replace_block_kind_and_payload(
        &mut self,
        block_id: BlockId,
        kind: RichBlockKind,
        payload: BlockPayload,
    ) -> Result<(), String> {
        self.replace_block_kind_and_payload_internal(block_id, kind, payload, true)
    }

    fn replace_block_kind_and_payload_internal(
        &mut self,
        block_id: BlockId,
        kind: RichBlockKind,
        payload: BlockPayload,
        advance_versions: bool,
    ) -> Result<(), String> {
        let payload = ensure_table_payload_for_kind(&kind, payload);
        let editable_text = editable_text_for_payload(&payload);
        if let Some(index) = self.index.index_of(block_id) {
            let height_estimate = estimate_block_height(&kind, &payload, DEFAULT_LAYOUT_WIDTH_PX);
            self.index.kind_tags[index] = kind_tag_for_rich_block_kind(&kind);
            if !matches!(kind, RichBlockKind::Heading { .. } | RichBlockKind::Toggle) {
                self.index.flags[index] &= !cditor_core::document::BLOCK_FLAG_FOLDED;
            }
            self.index.layout_meta[index].estimated_height = height_estimate.height;
            self.index.layout_meta[index].measured_height = None;
            self.index.layout_meta[index].dirty = true;
            if advance_versions {
                self.index.layout_meta[index].layout_version = self.index.layout_meta[index]
                    .layout_version
                    .saturating_add(1);
            }
            self.pending_measured_heights.remove(&block_id);
            self.layout_dirty = true;
            self.visible_index = VisibleDocumentIndex::from_document_index(&self.index);
            self.rebuild_height_indexes_from_layout_meta()?;
            self.list_projection_cache = ListProjectionCache::build(&self.index);
        }
        let content_version = match self.payload_window.get(block_id) {
            Some(payload) if advance_versions => payload.content_version.saturating_add(1),
            Some(payload) => payload.content_version,
            None => 1,
        };
        let mut updated_record = None;
        if let Some(record) = self.payload_window.payloads.get_mut(&block_id) {
            record.kind = kind;
            record.payload = payload;
            record.content_version = content_version;
            updated_record = Some(record.clone());
        }
        if let Some(mut record) = updated_record {
            self.sync_table_runtime_from_loaded_record(&mut record);
            self.payload_window.insert(record);
        }
        if let Some(editing) = self
            .editing
            .as_mut()
            .filter(|editing| editing.block_id == block_id)
        {
            let caret = editable_text.as_deref().map(str::len).unwrap_or(0);
            editing.content_version = content_version;
            editing.set_input_target(InputTarget::BlockText { block_id });
            editing.set_collapsed_selection(caret);
        }
        Ok(())
    }

    pub(crate) fn delete_document_selection(&mut self) -> Result<bool, String> {
        let before_selection = self.document_selection_snapshot();
        let before_selected_blocks = self.selected_block_ids_snapshot();
        let Some(selection) = self.document_selection else {
            return Ok(false);
        };
        if selection.is_caret() {
            return Ok(false);
        }
        let normalized = selection
            .normalize(&self.index)
            .map_err(|error| format!("{error:?}"))?;
        if normalized.start.block_id == normalized.end.block_id {
            let range = normalized.start.offset..normalized.end.offset;
            trace_input(
                "delete_document_selection.single_block",
                format_args!(
                    "block={} explicit_range={range:?} focus_before={:?}",
                    normalized.start.block_id,
                    self.focused_block_id()
                ),
            );
            self.focus_block_at_offset(normalized.start.block_id, normalized.start.offset)?;
            return self.replace_text_in_focused_range(Some(range), "");
        }
        let plan = self.plan_cross_block_replacement(selection)?;
        let start_block_id = normalized.start.block_id;
        let before_current_payload = self
            .payload_window
            .get(start_block_id)
            .cloned()
            .ok_or_else(|| format!("missing payload for block {start_block_id}"))?;
        let end_payload = self
            .payload_window
            .get(normalized.end.block_id)
            .ok_or_else(|| "selection end payload is not hydrated".to_owned())?;
        let start_spans = editable_payload_spans(&before_current_payload.payload)?;
        let end_spans = editable_payload_spans(&end_payload.payload)?;
        let start_text = plain_text_from_spans(&start_spans);
        let end_text = plain_text_from_spans(&end_spans);
        let start_offset = safe_char_range(
            &start_text,
            normalized.start.offset..normalized.start.offset,
        )
        .start;
        let end_offset =
            safe_char_range(&end_text, normalized.end.offset..normalized.end.offset).start;
        let mut surviving_spans = slice_rich_text_spans(&start_spans, 0..start_offset);
        surviving_spans.extend(slice_rich_text_spans(
            &end_spans,
            end_offset..end_text.len(),
        ));
        let after_payload =
            payload_for_kind_from_spans(&before_current_payload.kind, surviving_spans);
        let mut ops = vec![payload_replace_operation(
            start_block_id,
            before_current_payload.kind.clone(),
            before_current_payload.payload.clone(),
            before_current_payload.kind.clone(),
            after_payload.clone(),
        )];
        let (structure_ops, mut inverse_ops) = plan.structure_operations(Vec::new(), Vec::new())?;
        ops.extend(structure_ops);
        inverse_ops.push(payload_replace_operation(
            start_block_id,
            before_current_payload.kind.clone(),
            after_payload,
            before_current_payload.kind.clone(),
            before_current_payload.payload,
        ));
        let after_selection = Some(DocumentSelection::caret(TextPosition::downstream(
            start_block_id,
            start_offset,
        )));
        self.cancel_composition();
        self.apply_local_structure_transaction(
            EditTransactionKind::BlockStructureChange,
            cditor_core::edit::ChangeOrigin::User,
            ops,
            inverse_ops,
            vec![
                TransactionPrecondition::StructureVersion(self.structure_version()),
                TransactionPrecondition::BlockContentVersion {
                    block_id: start_block_id,
                    version: before_current_payload.content_version,
                },
            ],
            before_selection,
            after_selection,
            before_selected_blocks,
            Vec::new(),
        )?;
        self.focus_block_at_offset(start_block_id, start_offset)?;
        Ok(true)
    }
}
