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
    InlineSmall(Box<EditTransaction>),
    BlockRangeSnapshot {
        snapshot_id: SnapshotId,
        block_count: usize,
        transaction: Box<EditTransaction>,
    },
    Externalizing {
        snapshot_id: SnapshotId,
        block_count: usize,
    },
    ExternalBlob(ExternalUndoBlobRef),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalUndoBlobRef {
    pub snapshot_id: SnapshotId,
    pub storage_key: String,
    pub checksum: String,
    pub encoded_len: usize,
    pub block_count: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct UndoExternalizationCandidate<'a> {
    pub snapshot_id: SnapshotId,
    pub block_count: usize,
    pub transaction: &'a EditTransaction,
}

#[derive(Debug)]
pub struct UndoExternalizationJob {
    pub snapshot_id: SnapshotId,
    pub block_count: usize,
    pub transaction: Box<EditTransaction>,
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
            Self::Externalizing { block_count, .. } => *block_count,
            Self::ExternalBlob(reference) => reference.block_count,
        }
    }

    pub fn transaction(&self) -> Option<&EditTransaction> {
        match self {
            Self::InlineSmall(transaction) | Self::BlockRangeSnapshot { transaction, .. } => {
                Some(transaction.as_ref())
            }
            Self::Externalizing { .. } | Self::ExternalBlob(_) => None,
        }
    }

    pub fn transaction_mut(&mut self) -> Option<&mut EditTransaction> {
        match self {
            Self::InlineSmall(transaction) | Self::BlockRangeSnapshot { transaction, .. } => {
                Some(transaction.as_mut())
            }
            Self::Externalizing { .. } | Self::ExternalBlob(_) => None,
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
        matches!(&self.payload, UndoPayload::InlineSmall(_))
            .then(|| self.payload.transaction())
            .flatten()
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
    orphaned_external_blobs: Vec<ExternalUndoBlobRef>,
    policy: UndoGroupingPolicy,
    next_snapshot_id: SnapshotId,
}

impl UndoStack {
    pub fn new(policy: UndoGroupingPolicy) -> Self {
        Self {
            undo: VecDeque::new(),
            redo: VecDeque::new(),
            orphaned_external_blobs: Vec::new(),
            policy,
            next_snapshot_id: 1,
        }
    }

    pub fn record_transaction(
        &mut self,
        transaction: EditTransaction,
    ) -> Option<UndoGroupBoundary> {
        while let Some(step) = self.redo.pop_front() {
            self.collect_orphaned_blob(step);
        }
        let boundary = self.boundary_for(&transaction);
        if boundary.is_none()
            && let Some(previous) = self.undo.back_mut()
            && let Some(previous_transaction) = match &mut previous.payload {
                UndoPayload::InlineSmall(transaction) => Some(transaction.as_mut()),
                _ => None,
            }
            && previous_transaction
                .can_merge_typing_with(&transaction, self.policy.typing_merge_window_ms)
        {
            previous_transaction.merge_typing(transaction);
            return None;
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

    pub fn len(&self) -> usize {
        self.undo.len()
    }

    pub fn is_empty(&self) -> bool {
        self.undo.is_empty()
    }

    pub fn last(&self) -> Option<&EditTransaction> {
        self.last_undo_step()?.payload.transaction()
    }

    pub fn next_undo_external_blob(&self) -> Option<&ExternalUndoBlobRef> {
        match &self.undo.back()?.payload {
            UndoPayload::ExternalBlob(reference) => Some(reference),
            _ => None,
        }
    }

    pub fn next_redo_external_blob(&self) -> Option<&ExternalUndoBlobRef> {
        match &self.redo.back()?.payload {
            UndoPayload::ExternalBlob(reference) => Some(reference),
            _ => None,
        }
    }

    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }

    pub fn last_undo_step(&self) -> Option<&UndoStep> {
        self.undo.back()
    }

    pub fn last_undo_transaction_mut(&mut self) -> Option<&mut EditTransaction> {
        self.undo.back_mut()?.payload.transaction_mut()
    }

    pub fn clear(&mut self) {
        while let Some(step) = self.undo.pop_front() {
            self.collect_orphaned_blob(step);
        }
        while let Some(step) = self.redo.pop_front() {
            self.collect_orphaned_blob(step);
        }
    }

    pub fn clear_redo(&mut self) {
        while let Some(step) = self.redo.pop_front() {
            self.collect_orphaned_blob(step);
        }
    }

    pub fn truncate_undo_to(&mut self, max_steps: usize) {
        while self.undo.len() > max_steps {
            if let Some(step) = self.undo.pop_front() {
                self.collect_orphaned_blob(step);
            }
        }
    }

    pub fn drain_orphaned_external_blobs(&mut self) -> Vec<ExternalUndoBlobRef> {
        std::mem::take(&mut self.orphaned_external_blobs)
    }

    pub fn restore_orphaned_external_blobs(
        &mut self,
        references: impl IntoIterator<Item = ExternalUndoBlobRef>,
    ) {
        for reference in references {
            if !self
                .orphaned_external_blobs
                .iter()
                .any(|current| current.snapshot_id == reference.snapshot_id)
            {
                self.orphaned_external_blobs.push(reference);
            }
        }
    }

    pub fn take_undo_step(&mut self) -> Option<UndoStep> {
        self.undo.pop_back()
    }

    pub fn commit_undo_step(&mut self, step: UndoStep) {
        self.redo.push_back(step);
    }

    pub fn rollback_undo_step(&mut self, step: UndoStep) {
        self.undo.push_back(step);
    }

    pub fn take_redo_step(&mut self) -> Option<UndoStep> {
        self.redo.pop_back()
    }

    pub fn commit_redo_step(&mut self, step: UndoStep) {
        self.undo.push_back(step);
    }

    pub fn rollback_redo_step(&mut self, step: UndoStep) {
        self.redo.push_back(step);
    }

    pub fn pop_undo(&mut self) -> Option<&UndoStep> {
        let step = self.undo.pop_back()?;
        self.redo.push_back(step);
        self.redo.back()
    }

    pub fn pop_redo(&mut self) -> Option<&UndoStep> {
        let step = self.redo.pop_back()?;
        self.undo.push_back(step);
        self.undo.back()
    }

    pub fn pending_externalization(&self) -> Option<UndoExternalizationCandidate<'_>> {
        self.undo.iter().chain(&self.redo).find_map(|step| {
            let UndoPayload::BlockRangeSnapshot {
                snapshot_id,
                block_count,
                transaction,
            } = &step.payload
            else {
                return None;
            };
            Some(UndoExternalizationCandidate {
                snapshot_id: *snapshot_id,
                block_count: *block_count,
                transaction,
            })
        })
    }

    pub fn mark_externalized(&mut self, reference: ExternalUndoBlobRef) -> bool {
        let snapshot_id = reference.snapshot_id;
        let step = self.undo.iter_mut().chain(&mut self.redo).find(|step| {
            matches!(
                &step.payload,
                UndoPayload::BlockRangeSnapshot {
                    snapshot_id: current,
                    ..
                } if *current == snapshot_id
            )
        });
        let Some(step) = step else {
            return false;
        };
        let UndoPayload::BlockRangeSnapshot { block_count, .. } = &step.payload else {
            unreachable!("candidate was matched above");
        };
        if *block_count != reference.block_count {
            return false;
        }
        step.payload = UndoPayload::ExternalBlob(reference);
        true
    }

    pub fn begin_externalization(&mut self) -> Option<UndoExternalizationJob> {
        let step = self
            .undo
            .iter_mut()
            .chain(&mut self.redo)
            .find(|step| matches!(step.payload, UndoPayload::BlockRangeSnapshot { .. }))?;
        let placeholder = UndoPayload::Externalizing {
            snapshot_id: 0,
            block_count: 0,
        };
        let UndoPayload::BlockRangeSnapshot {
            snapshot_id,
            block_count,
            transaction,
        } = std::mem::replace(&mut step.payload, placeholder)
        else {
            unreachable!("pending snapshot was matched above");
        };
        step.payload = UndoPayload::Externalizing {
            snapshot_id,
            block_count,
        };
        Some(UndoExternalizationJob {
            snapshot_id,
            block_count,
            transaction,
        })
    }

    pub fn complete_externalization(
        &mut self,
        job: UndoExternalizationJob,
        reference: ExternalUndoBlobRef,
    ) -> Result<(), UndoExternalizationJob> {
        if job.snapshot_id != reference.snapshot_id || job.block_count != reference.block_count {
            return Err(job);
        }
        let step = self.undo.iter_mut().chain(&mut self.redo).find(|step| {
            matches!(
                step.payload,
                UndoPayload::Externalizing {
                    snapshot_id,
                    block_count,
                } if snapshot_id == job.snapshot_id && block_count == job.block_count
            )
        });
        let Some(step) = step else {
            return Err(job);
        };
        step.payload = UndoPayload::ExternalBlob(reference);
        Ok(())
    }

    pub fn abort_externalization(&mut self, job: UndoExternalizationJob) -> bool {
        let step = self.undo.iter_mut().chain(&mut self.redo).find(|step| {
            matches!(
                step.payload,
                UndoPayload::Externalizing {
                    snapshot_id,
                    block_count,
                } if snapshot_id == job.snapshot_id && block_count == job.block_count
            )
        });
        let Some(step) = step else {
            return false;
        };
        step.payload = UndoPayload::BlockRangeSnapshot {
            snapshot_id: job.snapshot_id,
            block_count: job.block_count,
            transaction: job.transaction,
        };
        true
    }

    pub fn hydrate_externalized(
        &mut self,
        snapshot_id: SnapshotId,
        transaction: EditTransaction,
    ) -> bool {
        let step = self.undo.iter_mut().chain(&mut self.redo).find(|step| {
            matches!(
                &step.payload,
                UndoPayload::ExternalBlob(reference)
                    if reference.snapshot_id == snapshot_id
            )
        });
        let Some(step) = step else {
            return false;
        };
        let UndoPayload::ExternalBlob(reference) = &step.payload else {
            unreachable!("external reference was matched above");
        };
        step.payload = UndoPayload::BlockRangeSnapshot {
            snapshot_id,
            block_count: reference.block_count,
            transaction: Box::new(transaction),
        };
        true
    }

    fn boundary_for(&self, transaction: &EditTransaction) -> Option<UndoGroupBoundary> {
        match transaction.kind {
            EditTransactionKind::Typing => {
                let previous = self.undo.back().and_then(UndoStep::inline_transaction)?;
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
                transaction: Box::new(transaction),
            }
        } else {
            UndoPayload::InlineSmall(Box::new(transaction))
        }
    }

    fn collect_orphaned_blob(&mut self, step: UndoStep) {
        if let UndoPayload::ExternalBlob(reference) = step.payload {
            self.restore_orphaned_external_blobs([reference]);
        }
    }
}

impl Default for UndoStack {
    fn default() -> Self {
        Self::new(UndoGroupingPolicy::default())
    }
}
