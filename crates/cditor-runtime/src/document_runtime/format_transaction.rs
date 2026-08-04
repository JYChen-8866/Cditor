use super::*;

impl DocumentRuntime {
    pub(super) fn apply_text_surface_format_transaction(
        &mut self,
        surface_id: SurfaceId,
        before_spans: Vec<InlineSpan>,
        after_spans: Vec<InlineSpan>,
    ) -> Result<bool, String> {
        if before_spans == after_spans {
            return Ok(false);
        }
        let before_text = plain_text_from_spans(&before_spans);
        let after_text = plain_text_from_spans(&after_spans);
        let kind = if before_text == after_text {
            EditTransactionKind::Format
        } else {
            EditTransactionKind::Paste
        };
        self.apply_text_surface_transaction(
            surface_id,
            before_spans,
            after_spans,
            kind,
            cditor_core::edit::ChangeOrigin::User,
            None,
        )
    }

    pub(super) fn apply_text_surface_paste_transaction(
        &mut self,
        surface_id: SurfaceId,
        before_spans: Vec<InlineSpan>,
        after_spans: Vec<InlineSpan>,
        after_offset: usize,
    ) -> Result<bool, String> {
        self.apply_text_surface_transaction(
            surface_id,
            before_spans,
            after_spans,
            EditTransactionKind::Paste,
            cditor_core::edit::ChangeOrigin::Import,
            Some(after_offset),
        )
    }

    fn apply_text_surface_transaction(
        &mut self,
        surface_id: SurfaceId,
        before_spans: Vec<InlineSpan>,
        after_spans: Vec<InlineSpan>,
        kind: EditTransactionKind,
        origin: cditor_core::edit::ChangeOrigin,
        after_offset: Option<usize>,
    ) -> Result<bool, String> {
        if before_spans == after_spans {
            return Ok(false);
        }
        let before_text = plain_text_from_spans(&before_spans);
        let after_text = plain_text_from_spans(&after_spans);
        let block_id = surface_id
            .block_id()
            .ok_or_else(|| "ephemeral surfaces cannot create document transactions".to_owned())?;
        let content_version = self
            .block_content_version(block_id)
            .ok_or_else(|| format!("missing content version for block {block_id}"))?;
        // DocumentSelection currently encodes only block + offset. Auxiliary
        // surfaces keep their richer InputTarget selection in the live session
        // until P5 unifies text/block/inner anchors.
        let before_selection = matches!(surface_id, SurfaceId::Block(_))
            .then(|| self.document_selection_snapshot())
            .flatten();
        let after_selection = match (surface_id, after_offset) {
            (SurfaceId::Block(block_id), Some(offset)) => {
                Some(DocumentSelection::caret(TextPosition {
                    block_id,
                    offset,
                    affinity: TextAffinity::Downstream,
                }))
            }
            (SurfaceId::Block(_), None) => before_selection,
            _ => None,
        };
        let anchor = self.capture_undo_scroll_snapshot().anchor;
        let transaction_id = self.transactions.next_id;
        self.transactions.next_id = self.transactions.next_id.saturating_add(1);
        let before_operation_spans = if before_text.is_empty() {
            Vec::new()
        } else {
            before_spans
        };
        let after_operation_spans = if after_text.is_empty() {
            Vec::new()
        } else {
            after_spans
        };
        let forward = EditOperation::Text(TextEditOperation::ReplaceSpans {
            surface_id,
            range: 0..before_text.len(),
            old_spans: before_operation_spans.clone(),
            new_spans: after_operation_spans.clone(),
        });
        let inverse = EditOperation::Text(TextEditOperation::ReplaceSpans {
            surface_id,
            range: 0..after_text.len(),
            old_spans: after_operation_spans,
            new_spans: before_operation_spans,
        });
        let transaction = EditTransaction::new(
            transaction_id,
            kind,
            transaction_id,
            vec![forward],
            vec![inverse],
        )
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
}
