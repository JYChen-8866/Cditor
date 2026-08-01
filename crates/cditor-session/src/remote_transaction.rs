use cditor_core::edit::{ChangeOrigin, EditTransaction};
use cditor_editor_protocol::{ProtocolError, ProtocolErrorCode};
use cditor_runtime::DocumentRuntime;

use crate::EditorSessionHandle;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemoteTransactionSnapshot {
    pub transaction_id: u64,
    pub revision: u64,
}

pub fn project_remote_transaction(
    runtime: &mut DocumentRuntime,
    transaction: &EditTransaction,
) -> Result<RemoteTransactionSnapshot, ProtocolError> {
    let mut transaction = transaction.clone();
    transaction.id = runtime
        .last_committed_transaction_id()
        .unwrap_or_default()
        .saturating_add(1);
    transaction.origin = ChangeOrigin::Remote;
    transaction.preconditions.clear();
    transaction.before_selection = None;
    transaction.after_selection = None;
    transaction.before_selected_blocks.clear();
    transaction.after_selected_blocks.clear();
    transaction.before_anchor = None;
    transaction.after_anchor = None;
    runtime
        .apply_external_transaction(&transaction, ChangeOrigin::Remote)
        .map_err(|error| {
            ProtocolError::new(
                ProtocolErrorCode::ApplyFailed,
                format!("remote transaction failed: {error}"),
            )
            .with_document(runtime.document_id())
        })?;
    Ok(RemoteTransactionSnapshot {
        transaction_id: transaction.id,
        revision: runtime.revision(),
    })
}

impl EditorSessionHandle {
    pub fn apply_remote_transaction(
        &self,
        transaction: &EditTransaction,
    ) -> Result<RemoteTransactionSnapshot, ProtocolError> {
        project_remote_transaction(&mut self.try_session_mut()?.runtime, transaction)
    }
}

#[cfg(test)]
mod tests {
    use cditor_core::{
        edit::{EditOperation, EditTransaction, EditTransactionKind},
        rich_text::{BlockPayloadRecord, RichBlockKind},
    };

    use super::*;

    #[test]
    fn remote_transaction_is_persistable_but_not_undoable() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord::rich_text(
                1,
                RichBlockKind::Paragraph,
                "a",
            )],
            720.0,
        );
        let transaction = EditTransaction::new(
            900,
            EditTransactionKind::Typing,
            1,
            vec![EditOperation::InsertText {
                block_id: 1,
                offset: 1,
                text: "b".into(),
            }],
            vec![],
        );
        let applied = project_remote_transaction(&mut runtime, &transaction).unwrap();
        assert_eq!(applied.transaction_id, 1);
        assert!(!runtime.can_undo());
        assert_eq!(runtime.pending_structure_transaction_count(), 1);
        assert_eq!(
            runtime.loaded_payload_records_snapshot()[0].plain_text(),
            "ab"
        );
    }
}
