use super::*;

impl DocumentRuntime {
    pub(in crate::document_runtime) fn apply_local_table_operation(
        &mut self,
        block_id: BlockId,
        kind: EditTransactionKind,
        forward: TableEditOperation,
        inverse: TableEditOperation,
    ) -> Result<AppliedTransaction, String> {
        self.apply_local_table_operations(block_id, kind, vec![forward], vec![inverse])
    }

    pub(in crate::document_runtime) fn apply_local_table_operations(
        &mut self,
        block_id: BlockId,
        kind: EditTransactionKind,
        forward: Vec<TableEditOperation>,
        inverse: Vec<TableEditOperation>,
    ) -> Result<AppliedTransaction, String> {
        self.break_typing_coalescing();
        let content_version = self
            .block_content_version(block_id)
            .ok_or_else(|| format!("missing payload for table block {block_id}"))?;
        // DocumentSelection cannot yet encode an inner table-cell anchor (P5).
        // Persisting a synthetic BlockText caret would make undo restore try to
        // hydrate a text model for the table block.
        let selection = None;
        let anchor = self.capture_undo_scroll_snapshot().anchor;
        let transaction_id = self.transactions.next_id;
        self.transactions.next_id = self.transactions.next_id.saturating_add(1);
        let transaction = EditTransaction::new(
            transaction_id,
            kind,
            transaction_id,
            forward.into_iter().map(EditOperation::Table).collect(),
            inverse.into_iter().map(EditOperation::Table).collect(),
        )
        .with_origin(cditor_core::edit::ChangeOrigin::User)
        .with_precondition(TransactionPrecondition::BlockContentVersion {
            block_id,
            version: content_version,
        })
        .with_selection(selection, selection)
        .with_anchor(anchor, anchor);
        self.apply_transaction(&transaction, TransactionPermissionSet::FULL_ACCESS)
            .map_err(|error| error.to_string())
    }

    pub(in crate::document_runtime) fn refresh_table_block_height(
        &mut self,
        block_id: BlockId,
    ) -> Result<bool, String> {
        let Some(payload) = self.document.payload_window.get(block_id) else {
            return Ok(false);
        };
        if !matches!(payload.kind, RichBlockKind::Table) {
            return Ok(false);
        }
        let Some(table_runtime) = self.table_runtime(block_id) else {
            return Ok(false);
        };
        let block_height = f64::from(table_payload_projected_height_px(table_runtime.table()));
        self.apply_measured_height(block_id, payload.content_version, block_height)
    }
}
