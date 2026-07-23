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
}
