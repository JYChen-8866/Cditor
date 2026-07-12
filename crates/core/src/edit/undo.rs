use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UndoGroupBoundary {
    TimeGap,
    SelectionChange,
    CompositionCommit,
    ExplicitCommand,
    BlockStructureChange,
    Paste,
    DragDrop,
    Format,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NonUndoableEditEvent {
    HeightCorrection,
    SyntaxHighlight,
    FtsUpdate,
    CacheWrite,
    AsyncPersistenceCallback,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UndoPayload {
    InlineSmall(EditTransaction),
    BlockRangeSnapshot {
        snapshot_id: SnapshotId,
        block_count: usize,
    },
    ExternalTempBlob {
        path: PathBuf,
        checksum: String,
    },
}

impl UndoPayload {
    pub fn block_count(&self) -> usize {
        match self {
            Self::InlineSmall(transaction) => transaction
                .ops
                .iter()
                .map(|op| match op {
                    EditOperation::InsertBlocks { blocks, .. } => blocks.len(),
                    EditOperation::DeleteBlockRange { range } => range.len(),
                    _ => 0,
                })
                .max()
                .unwrap_or(0),
            Self::BlockRangeSnapshot { block_count, .. } => *block_count,
            Self::ExternalTempBlob { .. } => 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UndoGroupingPolicy {
    pub typing_merge_window_ms: u64,
    pub inline_block_snapshot_limit: usize,
}

impl Default for UndoGroupingPolicy {
    fn default() -> Self {
        Self {
            typing_merge_window_ms: 1_000,
            inline_block_snapshot_limit: 1_024,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct UndoStep {
    pub payload: UndoPayload,
    pub boundary: Option<UndoGroupBoundary>,
    pub selection_restore_count: u8,
    pub anchor_restore_count: u8,
}

impl UndoStep {
    pub fn inline_transaction(&self) -> Option<&EditTransaction> {
        match &self.payload {
            UndoPayload::InlineSmall(transaction) => Some(transaction),
            _ => None,
        }
    }

    pub fn restore_user_position_once(&mut self) -> bool {
        if self.selection_restore_count > 0 || self.anchor_restore_count > 0 {
            return false;
        }
        self.selection_restore_count = 1;
        self.anchor_restore_count = 1;
        true
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct UndoStack {
    undo: VecDeque<UndoStep>,
    redo: VecDeque<UndoStep>,
    policy: UndoGroupingPolicy,
    next_snapshot_id: SnapshotId,
}

impl UndoStack {
    pub fn new(policy: UndoGroupingPolicy) -> Self {
        Self {
            undo: VecDeque::new(),
            redo: VecDeque::new(),
            policy,
            next_snapshot_id: 1,
        }
    }

    pub fn record_transaction(
        &mut self,
        transaction: EditTransaction,
    ) -> Option<UndoGroupBoundary> {
        self.redo.clear();
        let boundary = self.boundary_for(&transaction);
        if boundary.is_none() {
            if let Some(previous) = self.undo.back_mut() {
                if let Some(previous_transaction) = match &mut previous.payload {
                    UndoPayload::InlineSmall(transaction) => Some(transaction),
                    _ => None,
                } {
                    if previous_transaction
                        .can_merge_typing_with(&transaction, self.policy.typing_merge_window_ms)
                    {
                        previous_transaction.merge_typing(transaction);
                        return None;
                    }
                }
            }
        }

        let payload = self.payload_for(transaction);
        self.undo.push_back(UndoStep {
            payload,
            boundary,
            selection_restore_count: 0,
            anchor_restore_count: 0,
        });
        boundary
    }

    pub fn record_non_undoable_event(&mut self, _event: NonUndoableEditEvent) {
        // Intentionally ignored: background layout/cache/FTS/persistence events are not user undo.
    }

    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }

    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }

    pub fn last_undo_step(&self) -> Option<&UndoStep> {
        self.undo.back()
    }

    pub fn pop_undo(&mut self) -> Option<UndoStep> {
        let step = self.undo.pop_back()?;
        self.redo.push_back(step.clone());
        Some(step)
    }

    fn boundary_for(&self, transaction: &EditTransaction) -> Option<UndoGroupBoundary> {
        match transaction.kind {
            EditTransactionKind::Typing => {
                let Some(previous) = self.undo.back().and_then(UndoStep::inline_transaction) else {
                    return None;
                };
                if previous.kind != EditTransactionKind::Typing {
                    return Some(UndoGroupBoundary::ExplicitCommand);
                }
                if transaction.timestamp.saturating_sub(previous.timestamp)
                    > self.policy.typing_merge_window_ms
                {
                    return Some(UndoGroupBoundary::TimeGap);
                }
                if previous.after_selection != transaction.before_selection {
                    return Some(UndoGroupBoundary::SelectionChange);
                }
                None
            }
            EditTransactionKind::CompositionCommit => Some(UndoGroupBoundary::CompositionCommit),
            EditTransactionKind::Paste => Some(UndoGroupBoundary::Paste),
            EditTransactionKind::AiApply => Some(UndoGroupBoundary::ExplicitCommand),
            EditTransactionKind::DragDrop => Some(UndoGroupBoundary::DragDrop),
            EditTransactionKind::Format => Some(UndoGroupBoundary::Format),
            EditTransactionKind::ExplicitCommand => Some(UndoGroupBoundary::ExplicitCommand),
            EditTransactionKind::BlockStructureChange => {
                Some(UndoGroupBoundary::BlockStructureChange)
            }
        }
    }

    fn payload_for(&mut self, transaction: EditTransaction) -> UndoPayload {
        let touched_block_count = transaction
            .ops
            .iter()
            .map(|op| match op {
                EditOperation::InsertBlocks { blocks, .. } => blocks.len(),
                EditOperation::DeleteBlockRange { range } => range.len(),
                EditOperation::MoveBlockRange { range, .. } => range.len(),
                _ => 0,
            })
            .max()
            .unwrap_or(0);

        if touched_block_count > self.policy.inline_block_snapshot_limit {
            let snapshot_id = self.next_snapshot_id;
            self.next_snapshot_id = self.next_snapshot_id.saturating_add(1);
            UndoPayload::BlockRangeSnapshot {
                snapshot_id,
                block_count: touched_block_count,
            }
        } else {
            UndoPayload::InlineSmall(transaction)
        }
    }
}

impl Default for UndoStack {
    fn default() -> Self {
        Self::new(UndoGroupingPolicy::default())
    }
}
