use super::*;

/// Monotonic transaction identity and the ordered persistence journal.
#[derive(Debug)]
pub(super) struct TransactionState {
    pub(super) pending: Vec<EditTransaction>,
    pub(super) last_committed_id: Option<u64>,
    pub(super) next_id: u64,
}

impl Default for TransactionState {
    fn default() -> Self {
        Self {
            pending: Vec::new(),
            last_committed_id: None,
            next_id: 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use cditor_core::edit::{ChangeOrigin, EditOperation, EditTransactionKind};

    use super::*;

    #[test]
    fn extraction_preserves_monotonic_ids_and_restore_order() {
        let mut runtime = DocumentRuntime::empty();
        runtime.focus_block_at_offset(1, 0).unwrap();

        runtime.insert_char('a').unwrap();
        let first = runtime.drain_pending_structure_transactions();
        assert_eq!(first.len(), 1);
        assert_eq!(runtime.last_committed_transaction_id(), Some(first[0].id));

        runtime.insert_char('b').unwrap();
        let second_id = runtime.last_committed_transaction_id().unwrap();
        assert!(second_id > first[0].id);

        runtime.restore_pending_structure_transactions(first.clone());
        let restored = runtime.drain_pending_structure_transactions();
        assert_eq!(
            restored
                .iter()
                .map(|transaction| transaction.id)
                .collect::<Vec<_>>(),
            vec![first[0].id, second_id]
        );
    }

    #[test]
    fn external_replay_advances_the_next_local_transaction_identity() {
        let mut runtime = DocumentRuntime::empty();
        let transaction = EditTransaction::new(
            20,
            EditTransactionKind::Typing,
            20,
            vec![EditOperation::InsertText {
                block_id: 1,
                offset: 0,
                text: "a".to_owned(),
            }],
            vec![EditOperation::DeleteText {
                block_id: 1,
                range: 0..1,
            }],
        );
        runtime
            .apply_external_transaction(&transaction, ChangeOrigin::Host)
            .unwrap();

        runtime.focus_block_at_offset(1, 1).unwrap();
        runtime.insert_char('b').unwrap();

        assert!(runtime.last_committed_transaction_id().unwrap() > 20);
    }
}
