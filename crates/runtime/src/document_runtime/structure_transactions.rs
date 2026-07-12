use super::*;

impl DocumentRuntime {
    pub(super) fn queue_merge_delete_transaction(
        &mut self,
        survivor_block_id: BlockId,
        deleted_block_id: BlockId,
        merge: bool,
    ) {
        let transaction_id = self.next_transaction_id;
        self.next_transaction_id = self.next_transaction_id.saturating_add(1);
        let operation = if merge {
            EditOperation::MergeBlocks {
                previous: survivor_block_id,
                current: deleted_block_id,
            }
        } else {
            EditOperation::DeleteBlock {
                block_id: deleted_block_id,
            }
        };
        self.pending_structure_transactions
            .push(EditTransaction::new(
                transaction_id,
                EditTransactionKind::BlockStructureChange,
                transaction_id,
                vec![operation],
                vec![EditOperation::InsertBlock {
                    index: self.index.index_of(survivor_block_id).unwrap_or(0),
                    block: self
                        .index_record_for_block(survivor_block_id)
                        .unwrap_or_else(|_| {
                            BlockIndexRecord::new(survivor_block_id, None, 0, 0, 0)
                        }),
                }],
            ));
    }

    pub(super) fn queue_delete_selection_transaction(&mut self, survivor_block_id: BlockId) {
        let transaction_id = self.next_transaction_id;
        self.next_transaction_id = self.next_transaction_id.saturating_add(1);
        self.pending_structure_transactions
            .push(EditTransaction::new(
                transaction_id,
                EditTransactionKind::BlockStructureChange,
                transaction_id,
                vec![EditOperation::DeleteBlockRange { range: 0..0 }],
                vec![EditOperation::InsertBlock {
                    index: self.index.index_of(survivor_block_id).unwrap_or(0),
                    block: self
                        .index_record_for_block(survivor_block_id)
                        .unwrap_or_else(|_| {
                            BlockIndexRecord::new(survivor_block_id, None, 0, 0, 0)
                        }),
                }],
            ));
    }
}
