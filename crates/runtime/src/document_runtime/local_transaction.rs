//! Commit records for mutations already applied by the allocation-sensitive
//! input fast path.

use super::*;

pub(super) struct PreappliedTextEdit {
    pub(super) transaction_id: u64,
    pub(super) kind: EditTransactionKind,
    pub(super) origin: cditor_core::edit::ChangeOrigin,
    pub(super) surface_id: SurfaceId,
    pub(super) replaced_range: Range<usize>,
    pub(super) inserted_text: String,
    pub(super) before_record: BlockPayloadRecord,
    pub(super) after_record: BlockPayloadRecord,
    pub(super) before_surface: TextSurfaceSnapshot,
    pub(super) after_surface: Option<TextSurfaceSnapshot>,
    pub(super) before_selection: Option<DocumentSelection>,
    pub(super) after_selection: Option<DocumentSelection>,
    pub(super) before_anchor: Option<ScrollAnchor>,
    pub(super) after_anchor: Option<ScrollAnchor>,
    pub(super) before_layout_version: u64,
}

pub(super) struct LocalInsertBlocksTransaction {
    pub(super) index: usize,
    pub(super) blocks: Vec<BlockIndexRecord>,
    pub(super) payloads: Vec<BlockPayloadRecord>,
    pub(super) kind: EditTransactionKind,
    pub(super) origin: cditor_core::edit::ChangeOrigin,
    pub(super) before_selection: Option<DocumentSelection>,
    pub(super) after_selection: Option<DocumentSelection>,
}

pub(super) struct LocalBlockOperationsTransaction {
    pub(super) block_id: BlockId,
    pub(super) kind: EditTransactionKind,
    pub(super) origin: cditor_core::edit::ChangeOrigin,
    pub(super) ops: Vec<EditOperation>,
    pub(super) inverse_ops: Vec<EditOperation>,
    pub(super) before_selection: Option<DocumentSelection>,
    pub(super) after_selection: Option<DocumentSelection>,
}

impl DocumentRuntime {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn apply_local_structure_transaction(
        &mut self,
        kind: EditTransactionKind,
        origin: cditor_core::edit::ChangeOrigin,
        ops: Vec<EditOperation>,
        inverse_ops: Vec<EditOperation>,
        preconditions: Vec<TransactionPrecondition>,
        before_selection: Option<DocumentSelection>,
        after_selection: Option<DocumentSelection>,
        before_selected_blocks: Vec<BlockId>,
        after_selected_blocks: Vec<BlockId>,
    ) -> Result<bool, String> {
        if ops.is_empty() {
            return Ok(false);
        }
        if inverse_ops.is_empty() {
            return Err("local structure transaction requires inverse operations".to_owned());
        }
        self.break_typing_coalescing();
        let before_anchor = self.capture_undo_scroll_snapshot().anchor;
        let transaction_id = self.next_transaction_id;
        self.next_transaction_id = self.next_transaction_id.saturating_add(1);
        let mut transaction =
            EditTransaction::new(transaction_id, kind, transaction_id, ops, inverse_ops)
                .with_origin(origin)
                .with_selection(before_selection, after_selection)
                .with_block_selection(before_selected_blocks, after_selected_blocks)
                .with_anchor(before_anchor, None);
        for precondition in preconditions {
            transaction = transaction.with_precondition(precondition);
        }
        self.apply_transaction(&transaction, TransactionPermissionSet::FULL_ACCESS)
            .map_err(|error| error.to_string())?;
        let after_anchor = self.capture_undo_scroll_snapshot().anchor;
        if let Some(recorded) = self
            .external_undo_stack
            .last_undo_transaction_mut()
            .filter(|recorded| recorded.id == transaction_id)
        {
            recorded.after_anchor = after_anchor;
        }
        if let Some(persistent) = self
            .pending_structure_transactions
            .last_mut()
            .filter(|persistent| persistent.id == transaction_id)
        {
            persistent.after_anchor = after_anchor;
        }
        Ok(true)
    }

    pub(super) fn apply_local_insert_blocks_transaction(
        &mut self,
        request: LocalInsertBlocksTransaction,
    ) -> Result<bool, String> {
        let LocalInsertBlocksTransaction {
            index,
            blocks,
            payloads,
            kind,
            origin,
            before_selection,
            after_selection,
        } = request;
        if blocks.is_empty() {
            return Ok(false);
        }
        if blocks.len() != payloads.len() {
            return Err(format!(
                "block insertion carries {} index records but {} payloads",
                blocks.len(),
                payloads.len()
            ));
        }
        let end = index.saturating_add(blocks.len());
        let mut preconditions = Vec::with_capacity(blocks.len() + 1);
        preconditions.push(TransactionPrecondition::StructureVersion(
            self.structure_version(),
        ));
        preconditions.extend(
            blocks
                .iter()
                .map(|block| TransactionPrecondition::BlockAbsent(block.id)),
        );
        self.apply_local_structure_transaction(
            kind,
            origin,
            vec![EditOperation::InsertBlocks {
                index,
                blocks,
                payloads,
            }],
            vec![EditOperation::DeleteBlockRange { range: index..end }],
            preconditions,
            before_selection,
            after_selection,
            self.selected_block_ids_snapshot(),
            Vec::new(),
        )
    }

    pub(super) fn apply_local_block_operations_transaction(
        &mut self,
        request: LocalBlockOperationsTransaction,
    ) -> Result<bool, String> {
        let LocalBlockOperationsTransaction {
            block_id,
            kind,
            origin,
            ops,
            inverse_ops,
            before_selection,
            after_selection,
        } = request;
        if ops.is_empty() {
            return Ok(false);
        }
        if inverse_ops.is_empty() {
            return Err("local block transaction requires inverse operations".to_owned());
        }
        let content_version = self
            .block_content_version(block_id)
            .ok_or_else(|| format!("missing content version for block {block_id}"))?;
        self.break_typing_coalescing();
        let anchor = self.capture_undo_scroll_snapshot().anchor;
        let transaction_id = self.next_transaction_id;
        self.next_transaction_id = self.next_transaction_id.saturating_add(1);
        let transaction =
            EditTransaction::new(transaction_id, kind, transaction_id, ops, inverse_ops)
                .with_origin(origin)
                .with_precondition(TransactionPrecondition::BlockContentVersion {
                    block_id,
                    version: content_version,
                })
                .with_selection(before_selection, after_selection)
                .with_anchor(anchor, anchor);
        self.apply_transaction(&transaction, TransactionPermissionSet::FULL_ACCESS)
            .map_err(|error| error.to_string())?;
        Ok(true)
    }

    pub(super) fn apply_local_block_payload_transaction(
        &mut self,
        block_id: BlockId,
        kind: EditTransactionKind,
        after_kind: RichBlockKind,
        after_payload: BlockPayload,
    ) -> Result<bool, String> {
        self.apply_local_block_payload_transaction_with_origin(
            block_id,
            kind,
            cditor_core::edit::ChangeOrigin::User,
            after_kind,
            after_payload,
        )
    }

    pub(super) fn apply_local_block_payload_transaction_with_origin(
        &mut self,
        block_id: BlockId,
        kind: EditTransactionKind,
        origin: cditor_core::edit::ChangeOrigin,
        after_kind: RichBlockKind,
        after_payload: BlockPayload,
    ) -> Result<bool, String> {
        let before = self
            .payload_window
            .get(block_id)
            .cloned()
            .ok_or_else(|| format!("missing payload for block {block_id}"))?;
        if before.kind == after_kind && before.payload == after_payload {
            return Ok(false);
        }
        let selection = matches!(
            &before.payload,
            BlockPayload::RichText { .. } | BlockPayload::Code { .. } | BlockPayload::Html { .. }
        )
        .then(|| self.document_selection_snapshot())
        .flatten();
        self.apply_local_block_operations_transaction(LocalBlockOperationsTransaction {
            block_id,
            kind,
            origin,
            ops: vec![EditOperation::Block(
                cditor_core::edit::BlockEditOperation::ReplacePayload {
                    block_id,
                    before_kind: before.kind.clone(),
                    before_payload: before.payload.clone(),
                    after_kind: after_kind.clone(),
                    after_payload: after_payload.clone(),
                },
            )],
            inverse_ops: vec![EditOperation::Block(
                cditor_core::edit::BlockEditOperation::ReplacePayload {
                    block_id,
                    before_kind: after_kind,
                    before_payload: after_payload,
                    after_kind: before.kind,
                    after_payload: before.payload,
                },
            )],
            before_selection: selection,
            after_selection: selection,
        })
    }

    pub(super) fn record_preapplied_insert_transaction(
        &mut self,
        mut transaction: EditTransaction,
        surface_id: SurfaceId,
        offset: usize,
        inserted_len: usize,
    ) {
        let Some(block_id) = surface_id.block_id() else {
            panic!("pre-applied document text cannot target an ephemeral surface");
        };
        let inserted_range = offset..offset.saturating_add(inserted_len);
        let inserted_spans = self
            .payload_window
            .get(block_id)
            .and_then(|payload| match &payload.payload {
                BlockPayload::RichText { spans } => {
                    Some(slice_rich_text_spans(spans, inserted_range.clone()))
                }
                BlockPayload::Code { text, .. } | BlockPayload::Html { html: text, .. } => Some(
                    vec![InlineSpan::plain(text[inserted_range.clone()].to_owned())],
                ),
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing typed text surface for block {block_id}"));
        transaction.ops = vec![EditOperation::Text(TextEditOperation::ReplaceSpans {
            surface_id,
            range: offset..offset,
            old_spans: Vec::new(),
            new_spans: inserted_spans.clone(),
        })]
        .into();
        transaction.inverse_ops = vec![EditOperation::Text(TextEditOperation::ReplaceSpans {
            surface_id,
            range: inserted_range,
            old_spans: inserted_spans,
            new_spans: Vec::new(),
        })]
        .into();
        let before_layout_version = self.index.index_of(block_id).map_or_else(
            || panic!("missing layout metadata for block {block_id}"),
            |index| self.index.layout_meta[index].layout_version,
        );
        self.queue_preapplied_transaction(transaction, block_id, before_layout_version);
    }

    pub(super) fn record_preapplied_text_replacement(&mut self, edit: PreappliedTextEdit) {
        debug_assert!(validate_preapplied_text_edit(&edit).is_ok());
        let block_id = edit
            .surface_id
            .block_id()
            .expect("pre-applied document text cannot target an ephemeral surface");
        let (forward, inverse) = if must_replace_whole_payload(&edit) {
            (
                EditOperation::Block(cditor_core::edit::BlockEditOperation::ReplacePayload {
                    block_id,
                    before_kind: edit.before_record.kind.clone(),
                    before_payload: edit.before_record.payload.clone(),
                    after_kind: edit.after_record.kind.clone(),
                    after_payload: edit.after_record.payload.clone(),
                }),
                EditOperation::Block(cditor_core::edit::BlockEditOperation::ReplacePayload {
                    block_id,
                    before_kind: edit.after_record.kind.clone(),
                    before_payload: edit.after_record.payload.clone(),
                    after_kind: edit.before_record.kind.clone(),
                    after_payload: edit.before_record.payload.clone(),
                }),
            )
        } else {
            text_replacement_operations(&edit)
        };
        let transaction = EditTransaction::new(
            edit.transaction_id,
            edit.kind,
            edit.transaction_id,
            vec![forward],
            vec![inverse],
        )
        .with_origin(edit.origin)
        .with_precondition(TransactionPrecondition::BlockContentVersion {
            block_id,
            version: edit.before_record.content_version,
        })
        .with_selection(edit.before_selection, edit.after_selection)
        .with_anchor(edit.before_anchor, edit.after_anchor);
        self.queue_preapplied_transaction(transaction, block_id, edit.before_layout_version);
    }

    fn queue_preapplied_transaction(
        &mut self,
        transaction: EditTransaction,
        block_id: BlockId,
        before_layout_version: u64,
    ) {
        let transaction_id = transaction.id;
        self.pending_structure_transactions.push(transaction);
        self.last_committed_transaction_id = Some(transaction_id);
        if let Some(position) = self.index.index_of(block_id) {
            let layout = &mut self.index.layout_meta[position];
            if layout.layout_version == before_layout_version {
                layout.layout_version = layout.layout_version.saturating_add(1);
            }
            layout.dirty = true;
        }
        self.layout_dirty = true;
    }
}

fn validate_preapplied_text_edit(edit: &PreappliedTextEdit) -> Result<(), String> {
    validate_preapplied_text_preflight(
        edit.surface_id,
        &edit.before_record,
        &edit.before_surface,
        edit.replaced_range.clone(),
    )?;
    let block_id = edit
        .surface_id
        .block_id()
        .expect("preflight rejected ephemeral surface");
    if edit.after_record.block_id != block_id {
        return Err(format!(
            "text transaction surface {block_id} does not match before/after payload owners"
        ));
    }
    if let Some(after) = &edit.after_surface
        && (after.identity.surface_id != edit.surface_id
            || after.identity.content_version != edit.after_record.content_version)
    {
        return Err("after text surface identity does not match payload version".to_owned());
    }
    Ok(())
}

pub(super) fn validate_preapplied_text_preflight(
    surface_id: SurfaceId,
    before_record: &BlockPayloadRecord,
    before_surface: &TextSurfaceSnapshot,
    replaced_range: Range<usize>,
) -> Result<(), String> {
    let block_id = surface_id
        .block_id()
        .ok_or_else(|| "ephemeral surface cannot create a document transaction".to_owned())?;
    if before_record.block_id != block_id {
        return Err(format!(
            "text transaction surface {block_id} does not match before payload owner {}",
            before_record.block_id
        ));
    }
    if before_surface.identity.surface_id != surface_id
        || before_surface.identity.content_version != before_record.content_version
    {
        return Err("before text surface identity does not match payload version".to_owned());
    }
    let before_text = before_surface.plain_text();
    if replaced_range.start > replaced_range.end
        || replaced_range.end > before_text.len()
        || !before_text.is_char_boundary(replaced_range.start)
        || !before_text.is_char_boundary(replaced_range.end)
    {
        return Err(format!(
            "pre-applied text range {replaced_range:?} is invalid for {} bytes",
            before_text.len()
        ));
    }
    Ok(())
}

fn must_replace_whole_payload(edit: &PreappliedTextEdit) -> bool {
    edit.before_record.kind != edit.after_record.kind
        || edit.after_surface.is_none()
        || matches!(
            (&edit.before_record.payload, &edit.after_record.payload),
            (BlockPayload::Html { .. }, BlockPayload::Html { .. })
        )
}

fn text_replacement_operations(edit: &PreappliedTextEdit) -> (EditOperation, EditOperation) {
    let after = edit
        .after_surface
        .as_ref()
        .expect("partial text replacement requires an after surface");
    let expected_after = replace_rich_text_spans_preserving_marks(
        &edit.before_surface.spans,
        edit.replaced_range.clone(),
        &edit.inserted_text,
    );
    let (before_range, after_range, old_spans, new_spans) = if expected_after == after.spans {
        let after_range = edit.replaced_range.start
            ..edit
                .replaced_range
                .start
                .saturating_add(edit.inserted_text.len());
        (
            edit.replaced_range.clone(),
            after_range.clone(),
            spans_for_range(&edit.before_surface.spans, edit.replaced_range.clone()),
            spans_for_range(&after.spans, after_range),
        )
    } else {
        let before_range = 0..edit.before_surface.len();
        let after_range = 0..after.len();
        (
            before_range.clone(),
            after_range.clone(),
            spans_for_range(&edit.before_surface.spans, before_range),
            spans_for_range(&after.spans, after_range),
        )
    };
    (
        EditOperation::Text(TextEditOperation::ReplaceSpans {
            surface_id: edit.surface_id,
            range: before_range,
            old_spans: old_spans.clone(),
            new_spans: new_spans.clone(),
        }),
        EditOperation::Text(TextEditOperation::ReplaceSpans {
            surface_id: edit.surface_id,
            range: after_range,
            old_spans: new_spans,
            new_spans: old_spans,
        }),
    )
}

fn spans_for_range(spans: &[InlineSpan], range: Range<usize>) -> Vec<InlineSpan> {
    if range.is_empty() {
        Vec::new()
    } else {
        slice_rich_text_spans(spans, range)
    }
}
