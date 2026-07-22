//! external EditTransaction 的原子应用（P4-005/P4-006）。
//!
//! journal replay、远端 operation、SDK 批量写入的统一消费入口：
//!
//! - **前置校验 + 原子性**：全部 op 先应用到 staging（records 副本 +
//!   copy-on-touch payload），任一 op 违反前置条件则整个事务拒绝、runtime
//!   状态零改动；提交前做整树 preorder 校验。
//! - **版本统一（P4-006）**：结构变化经 `rebuild_structure_index` 单次推进
//!   structure_version；每个被触碰 payload 的 content_version +1；revision
//!   经 `note_content_changed` 推进；返回受影响块的 preorder dirty range。
//! - **来源语义（P4-007/P4-011）**：`origin.records_local_undo()` 决定是否
//!   进入本地 undo（remote/migration/undo/redo 不进）；undo 应用
//!   `inverse_ops`，redo 重放 `ops`，双向都走本入口保证语义一致。
//!
//! UI 热路径（逐键输入）不经过本入口；它们的事务化收敛属于后续 P4-005 扩展。

use std::collections::{BTreeSet, HashMap};

use cditor_core::edit::ChangeOrigin;

use super::transaction_apply_domain::{
    apply_asset_operation, apply_block_operation, apply_collection_operation,
    apply_comment_operation, apply_text_operation,
};
use super::transaction_apply_payload::{
    apply_table_op, delete_text_from_payload, insert_text_into_payload, split_payload_text,
};
use super::transaction_apply_structure::{
    ensure_complete_subtrees, position_of, stage_insert_blocks, stage_move_to_parent, subtree_end,
    validate_preorder,
};
use super::*;

/// 应用失败；失败时 runtime 状态保证未被修改。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransactionApplyError {
    PermissionDenied(TransactionPermission),
    StalePrecondition {
        index: usize,
        reason: String,
    },
    /// 第 `op_index` 个 op 违反前置条件。
    Precondition {
        op_index: usize,
        reason: String,
    },
    /// 全部 op 应用后整树校验失败（事务整体非法）。
    InvalidResultingStructure(String),
}

impl std::fmt::Display for TransactionApplyError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PermissionDenied(permission) => {
                write!(formatter, "transaction requires {permission:?} permission")
            }
            Self::StalePrecondition { index, reason } => {
                write!(
                    formatter,
                    "transaction precondition #{index} failed: {reason}"
                )
            }
            Self::Precondition { op_index, reason } => {
                write!(
                    formatter,
                    "transaction op #{op_index} precondition failed: {reason}"
                )
            }
            Self::InvalidResultingStructure(reason) => {
                write!(
                    formatter,
                    "transaction produces invalid structure: {reason}"
                )
            }
        }
    }
}

/// 成功应用的结果摘要。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedTransaction {
    pub transaction_id: u64,
    pub document_revision: u64,
    pub structure_version: u64,
    pub affected_blocks: Vec<BlockId>,
    pub content_versions: Vec<(BlockId, u64)>,
    pub layout_versions: Vec<(BlockId, u64)>,
    /// 受影响块在新 preorder 中的 dirty range（min..max+1）。
    pub dirty_range: Option<Range<usize>>,
    pub structure_changed: bool,
    pub recorded_undo: bool,
}

/// staging：所有变更先落在这里，成功才提交回 runtime。
pub(super) struct StagingState {
    pub(super) records: Vec<BlockIndexRecord>,
    pub(super) payloads: HashMap<BlockId, BlockPayloadRecord>,
    pub(super) touched: BTreeSet<BlockId>,
    pub(super) dirty_blocks: BTreeSet<BlockId>,
    pub(super) deleted: BTreeSet<BlockId>,
    pub(super) inserted: BTreeSet<BlockId>,
    pub(super) block_attrs: HashMap<BlockId, BlockAttrs>,
    pub(super) collection_records: HashMap<CollectionId, Vec<CollectionRecordSnapshot>>,
    pub(super) comment_threads: HashMap<CommentThreadId, Option<CommentThreadSnapshot>>,
    pub(super) assets: HashMap<AssetId, Option<AssetSnapshot>>,
    pub(super) block_asset_ids: HashMap<BlockId, BTreeSet<AssetId>>,
    pub(super) structure_changed: bool,
}

impl StagingState {
    pub(super) fn mark_dirty(&mut self, block_id: BlockId) {
        self.dirty_blocks.insert(block_id);
    }
}

impl DocumentRuntime {
    pub fn apply_transaction(
        &mut self,
        transaction: &EditTransaction,
        permissions: TransactionPermissionSet,
    ) -> Result<AppliedTransaction, TransactionApplyError> {
        self.apply_transaction_authorized(transaction, transaction.origin, permissions)
    }

    /// 原子应用一个外部事务。失败时状态零改动。
    pub fn apply_external_transaction(
        &mut self,
        transaction: &EditTransaction,
        origin: ChangeOrigin,
    ) -> Result<AppliedTransaction, TransactionApplyError> {
        self.apply_transaction_authorized(
            transaction,
            origin,
            TransactionPermissionSet::FULL_ACCESS,
        )
    }

    fn apply_transaction_authorized(
        &mut self,
        transaction: &EditTransaction,
        origin: ChangeOrigin,
        permissions: TransactionPermissionSet,
    ) -> Result<AppliedTransaction, TransactionApplyError> {
        for operation in transaction.ops.iter() {
            let required = operation.required_permission();
            if !permissions.allows(required) {
                return Err(TransactionApplyError::PermissionDenied(required));
            }
        }
        self.validate_transaction_preconditions(transaction)?;

        let mut staging = StagingState {
            records: self.index_records(),
            payloads: HashMap::new(),
            touched: BTreeSet::new(),
            dirty_blocks: BTreeSet::new(),
            deleted: BTreeSet::new(),
            inserted: BTreeSet::new(),
            block_attrs: HashMap::new(),
            collection_records: HashMap::new(),
            comment_threads: HashMap::new(),
            assets: HashMap::new(),
            block_asset_ids: HashMap::new(),
            structure_changed: false,
        };

        for (op_index, op) in transaction.ops.iter().enumerate() {
            self.apply_op_to_staging(&mut staging, op)
                .map_err(|reason| TransactionApplyError::Precondition { op_index, reason })?;
        }
        for block_id in staging.dirty_blocks.clone() {
            if staging.deleted.contains(&block_id) {
                continue;
            }
            let Some(position) = position_of(&staging.records, block_id) else {
                continue;
            };
            let layout = &mut staging.records[position].layout_meta;
            layout.layout_version = layout.layout_version.saturating_add(1);
            layout.dirty = true;
        }
        validate_preorder(&staging.records)
            .map_err(TransactionApplyError::InvalidResultingStructure)?;

        // ---- 提交（不再失败的区域） ----
        let StagingState {
            records,
            mut payloads,
            touched,
            dirty_blocks,
            deleted,
            inserted,
            block_attrs,
            collection_records,
            comment_threads,
            assets,
            block_asset_ids,
            structure_changed,
        } = staging;

        if structure_changed {
            if let Err(error) = self.rebuild_structure_index(records) {
                // records 已通过 preorder 校验，DocumentIndex 只会因重复 ID 失败，
                // 而校验已覆盖唯一性；到达此处属于内部不变量破坏。
                return Err(TransactionApplyError::InvalidResultingStructure(error));
            }
        } else {
            for block_id in &dirty_blocks {
                let Some(position) = self.index.index_of(*block_id) else {
                    continue;
                };
                let Some(staged_position) = position_of(&records, *block_id) else {
                    continue;
                };
                self.index.layout_meta[position] = records[staged_position].layout_meta;
            }
        }

        for block_id in &touched {
            if deleted.contains(block_id) {
                continue;
            }
            let Some(mut record) = payloads.remove(block_id) else {
                continue;
            };
            record.content_version = record.content_version.saturating_add(1);
            if matches!(record.kind, RichBlockKind::Table) {
                self.sync_table_runtime_from_loaded_record(&mut record);
            } else {
                self.table_runtimes.remove(block_id);
                self.table_horizontal_scroll_offsets.remove(block_id);
            }
            if !inserted.contains(block_id) {
                sync_text_model_for_payload(&mut self.text_models, &record);
            }
            self.payload_window.insert(record);
            if let Some(editing) = self
                .editing
                .as_mut()
                .filter(|editing| editing.block_id == *block_id)
            {
                editing.content_version = self
                    .payload_window
                    .get(*block_id)
                    .map(|payload| payload.content_version)
                    .unwrap_or(editing.content_version);
            }
        }
        for block_id in &deleted {
            self.payload_window.remove(*block_id);
            self.text_models.remove(block_id);
            self.table_runtimes.remove(block_id);
            self.block_attrs.remove(block_id);
            self.block_asset_ids.remove(block_id);
        }
        for (block_id, attrs) in block_attrs {
            if attrs == BlockAttrs::default() {
                self.block_attrs.remove(&block_id);
            } else {
                self.block_attrs.insert(block_id, attrs);
            }
        }
        for (collection_id, records) in collection_records {
            self.collection_records.insert(collection_id, records);
        }
        for (thread_id, thread) in comment_threads {
            if let Some(thread) = thread {
                self.comment_threads.insert(thread_id, thread);
            } else {
                self.comment_threads.remove(&thread_id);
            }
        }
        for (asset_id, asset) in assets {
            if let Some(asset) = asset {
                self.assets.insert(asset_id, asset);
            } else {
                self.assets.remove(&asset_id);
            }
        }
        for (block_id, asset_ids) in block_asset_ids {
            if asset_ids.is_empty() {
                self.block_asset_ids.remove(&block_id);
            } else {
                self.block_asset_ids.insert(block_id, asset_ids);
            }
        }
        self.remap_focused_table_cell_after_table_operations(&transaction.ops);

        // focus/selection 指向已删除块时清理（不变量：selection 指向存在的块）。
        if let Some(editing) = &self.editing
            && self.index.index_of(editing.block_id).is_none()
        {
            self.editing = None;
            self.focused_text_selection = None;
        }
        if let Some(cell) = self.focused_table_cell
            && self.index.index_of(cell.block_id).is_none()
        {
            self.focused_table_cell = None;
        }
        if let Some(selection) = self.document_selection
            && (self.index.index_of(selection.anchor.block_id).is_none()
                || self.index.index_of(selection.focus.block_id).is_none())
        {
            self.document_selection = None;
        }

        let affected_blocks: Vec<BlockId> = {
            let mut seen = dirty_blocks;
            seen.extend(deleted.iter().copied());
            transaction
                .ops
                .iter()
                .flat_map(EditOperation::affected_blocks)
                .for_each(|block_id| {
                    seen.insert(block_id);
                });
            seen.into_iter().collect()
        };
        let dirty_range = {
            let positions: Vec<usize> = affected_blocks
                .iter()
                .filter_map(|block_id| self.index.index_of(*block_id))
                .collect();
            positions
                .iter()
                .min()
                .zip(positions.iter().max())
                .map(|(min, max)| *min..max.saturating_add(1))
        };
        for block_id in &affected_blocks {
            self.pending_measured_heights.remove(block_id);
            if self
                .payload_window
                .get(*block_id)
                .is_some_and(|record| matches!(record.kind, RichBlockKind::Table))
            {
                let committed_layout_version = self
                    .index
                    .index_of(*block_id)
                    .map(|position| self.index.layout_meta[position].layout_version);
                // The payload/table runtime are already committed and valid.
                // Height refresh is derived layout state inside this same
                // commit epoch. `apply_measured_height` may advance the layout
                // version for an independent async measurement; normalize it
                // back here because staging already advanced this transaction's
                // layout identity exactly once.
                let _ = self.refresh_table_block_height(*block_id);
                if let (Some(committed), Some(position)) =
                    (committed_layout_version, self.index.index_of(*block_id))
                {
                    self.index.layout_meta[position].layout_version = committed;
                }
            }
        }
        let content_versions = affected_blocks
            .iter()
            .filter_map(|block_id| {
                self.payload_window
                    .get(*block_id)
                    .map(|payload| (*block_id, payload.content_version))
            })
            .collect();
        let layout_versions = affected_blocks
            .iter()
            .filter_map(|block_id| {
                let position = self.index.index_of(*block_id)?;
                Some((*block_id, self.index.layout_meta[position].layout_version))
            })
            .collect();

        let recorded_undo = origin.records_local_undo() && !transaction.inverse_ops.is_empty();
        if recorded_undo {
            // Preconditions authorize the initial submission. Undo/redo use
            // the operations' carried before values against the then-current
            // state; replaying the original version precondition would make
            // every redo stale after the inverse advances content versions.
            let mut undoable = transaction.clone();
            undoable.preconditions.clear();
            self.clear_runtime_redo_history();
            self.external_undo_stack.record_transaction(undoable);
            self.external_undo_stack.truncate_undo_to(100);
            self.undo_events.push(RuntimeUndoEvent::ExternalTransaction);
            self.trim_runtime_undo_history();
        }
        if !affected_blocks.is_empty() || structure_changed {
            self.layout_dirty = true;
            self.note_content_changed();
        }
        if !transaction.ops.is_empty()
            && !matches!(origin, ChangeOrigin::Remote | ChangeOrigin::Migration)
        {
            let mut persistent = transaction.clone();
            persistent.origin = origin;
            self.pending_structure_transactions.push(persistent);
        }
        self.last_committed_transaction_id = Some(transaction.id);

        Ok(AppliedTransaction {
            transaction_id: transaction.id,
            document_revision: self.revision(),
            structure_version: self.structure_version(),
            affected_blocks,
            content_versions,
            layout_versions,
            dirty_range,
            structure_changed,
            recorded_undo,
        })
    }

    fn validate_transaction_preconditions(
        &self,
        transaction: &EditTransaction,
    ) -> Result<(), TransactionApplyError> {
        for (index, precondition) in transaction.preconditions.iter().enumerate() {
            let failure = match precondition {
                TransactionPrecondition::DocumentRevision(expected)
                    if self.revision() != *expected =>
                {
                    Some(format!(
                        "document revision is {}, expected {expected}",
                        self.revision()
                    ))
                }
                TransactionPrecondition::StructureVersion(expected)
                    if self.structure_version() != *expected =>
                {
                    Some(format!(
                        "structure version is {}, expected {expected}",
                        self.structure_version()
                    ))
                }
                TransactionPrecondition::BlockContentVersion { block_id, version }
                    if self.block_content_version(*block_id) != Some(*version) =>
                {
                    Some(format!(
                        "block {block_id} content version is {:?}, expected {version}",
                        self.block_content_version(*block_id)
                    ))
                }
                TransactionPrecondition::BlockExists(block_id)
                    if self.index.index_of(*block_id).is_none() =>
                {
                    Some(format!("block {block_id} does not exist"))
                }
                TransactionPrecondition::BlockAbsent(block_id)
                    if self.index.index_of(*block_id).is_some() =>
                {
                    Some(format!("block {block_id} already exists"))
                }
                _ => None,
            };
            if let Some(reason) = failure {
                return Err(TransactionApplyError::StalePrecondition { index, reason });
            }
        }
        Ok(())
    }

    fn apply_op_to_staging(
        &self,
        staging: &mut StagingState,
        op: &EditOperation,
    ) -> Result<(), String> {
        match op {
            EditOperation::InsertText {
                block_id,
                offset,
                text,
            } => {
                let record = self.staged_payload(staging, *block_id)?;
                insert_text_into_payload(record, *offset, text)
            }
            EditOperation::DeleteText { block_id, range } => {
                let record = self.staged_payload(staging, *block_id)?;
                delete_text_from_payload(record, range.clone())
            }
            EditOperation::SplitBlock {
                block_id,
                offset,
                new_block_id,
            } => self.stage_split_block(staging, *block_id, *offset, *new_block_id),
            EditOperation::MergeBlocks { previous, current } => {
                self.stage_merge_blocks(staging, *previous, *current)
            }
            EditOperation::InsertBlock { index, block } => {
                stage_insert_blocks(staging, *index, std::slice::from_ref(block), &[])
            }
            EditOperation::InsertBlocks {
                index,
                blocks,
                payloads,
            } => stage_insert_blocks(staging, *index, blocks, payloads),
            EditOperation::DeleteBlock { block_id } => {
                let position = position_of(&staging.records, *block_id)
                    .ok_or_else(|| format!("block {block_id} does not exist"))?;
                if subtree_end(&staging.records, position) != position + 1 {
                    return Err(format!(
                        "block {block_id} has children; use DeleteBlockRange for subtrees"
                    ));
                }
                let record = staging.records.remove(position);
                staging.deleted.insert(record.id);
                staging.structure_changed = true;
                Ok(())
            }
            EditOperation::DeleteBlockRange { range } => {
                if range.start > range.end || range.end > staging.records.len() {
                    return Err(format!(
                        "range {range:?} out of bounds ({} records)",
                        staging.records.len()
                    ));
                }
                ensure_complete_subtrees(&staging.records, range.clone())?;
                for record in staging.records.drain(range.clone()) {
                    staging.deleted.insert(record.id);
                }
                staging.structure_changed = true;
                Ok(())
            }
            EditOperation::MoveBlock {
                block_id,
                target_index,
            } => {
                let position = position_of(&staging.records, *block_id)
                    .ok_or_else(|| format!("block {block_id} does not exist"))?;
                if subtree_end(&staging.records, position) != position + 1 {
                    return Err(format!(
                        "block {block_id} has children; use MoveBlockToParent"
                    ));
                }
                let record = staging.records.remove(position);
                let target = (*target_index).min(staging.records.len());
                staging.records.insert(target, record);
                staging.structure_changed = true;
                Ok(())
            }
            EditOperation::MoveBlockRange {
                range,
                target_index,
            } => {
                if range.start > range.end || range.end > staging.records.len() {
                    return Err(format!("range {range:?} out of bounds"));
                }
                ensure_complete_subtrees(&staging.records, range.clone())?;
                let moved: Vec<_> = staging.records.drain(range.clone()).collect();
                let mut target = *target_index;
                if target > range.start {
                    target = target.saturating_sub(moved.len());
                }
                let target = target.min(staging.records.len());
                staging.records.splice(target..target, moved);
                staging.structure_changed = true;
                Ok(())
            }
            EditOperation::MoveBlockToParent {
                block_id,
                parent_id,
                sibling_index,
            } => stage_move_to_parent(staging, *block_id, *parent_id, *sibling_index),
            EditOperation::SetBlockKind { block_id, kind } => {
                let position = position_of(&staging.records, *block_id)
                    .ok_or_else(|| format!("block {block_id} does not exist"))?;
                staging.records[position].kind_tag = *kind;
                let record = self.staged_payload(staging, *block_id)?;
                record.kind = cditor_core::rich_text::rich_block_kind_from_tag(*kind);
                staging.structure_changed = true;
                Ok(())
            }
            EditOperation::Table(table_op) => {
                let record = self.staged_payload(staging, table_op.block_id())?;
                apply_table_op(record, table_op)
            }
            EditOperation::Text(operation) => apply_text_operation(self, staging, operation),
            EditOperation::Block(operation) => apply_block_operation(self, staging, operation),
            EditOperation::Collection(operation) => {
                apply_collection_operation(self, staging, operation)
            }
            EditOperation::Comment(operation) => apply_comment_operation(self, staging, operation),
            EditOperation::Asset(operation) => apply_asset_operation(self, staging, operation),
        }
    }

    pub(super) fn staged_payload_read<'staging>(
        &self,
        staging: &'staging mut StagingState,
        block_id: BlockId,
    ) -> Result<&'staging BlockPayloadRecord, String> {
        if staging.deleted.contains(&block_id) {
            return Err(format!(
                "block {block_id} was deleted earlier in this transaction"
            ));
        }
        if !staging.payloads.contains_key(&block_id) {
            if position_of(&staging.records, block_id).is_none() {
                return Err(format!("block {block_id} does not exist"));
            }
            let record = self
                .payload_window
                .get(block_id)
                .cloned()
                .map(|payload| self.table_runtime_payload_record(block_id, payload))
                .ok_or_else(|| format!("payload for block {block_id} is not hydrated"))?;
            staging.payloads.insert(block_id, record);
        }
        Ok(staging
            .payloads
            .get(&block_id)
            .expect("payload staged above"))
    }

    /// copy-on-touch 获取 staging payload；未 hydrate/已删除的块拒绝。
    pub(super) fn staged_payload<'staging>(
        &self,
        staging: &'staging mut StagingState,
        block_id: BlockId,
    ) -> Result<&'staging mut BlockPayloadRecord, String> {
        self.staged_payload_read(staging, block_id)?;
        staging.touched.insert(block_id);
        staging.dirty_blocks.insert(block_id);
        Ok(staging
            .payloads
            .get_mut(&block_id)
            .expect("payload staged above"))
    }

    fn stage_split_block(
        &self,
        staging: &mut StagingState,
        block_id: BlockId,
        offset: usize,
        new_block_id: BlockId,
    ) -> Result<(), String> {
        if position_of(&staging.records, new_block_id).is_some() {
            return Err(format!("new block id {new_block_id} already exists"));
        }
        let position = position_of(&staging.records, block_id)
            .ok_or_else(|| format!("block {block_id} does not exist"))?;
        let (leading, trailing, kind) = {
            let record = self.staged_payload(staging, block_id)?;
            let (leading, trailing) = split_payload_text(record, offset)?;
            (leading, trailing, record.kind.clone())
        };
        let record = self.staged_payload(staging, block_id)?;
        record.payload = leading;

        let source = staging.records[position];
        let insert_at = subtree_end(&staging.records, position);
        let new_record = BlockIndexRecord::new(
            new_block_id,
            source.parent_id,
            source.depth,
            source.kind_tag,
            0,
        );
        staging.records.insert(insert_at, new_record);
        staging.payloads.insert(
            new_block_id,
            BlockPayloadRecord {
                block_id: new_block_id,
                content_version: 0,
                kind,
                payload: trailing,
            },
        );
        staging.touched.insert(new_block_id);
        staging.structure_changed = true;
        Ok(())
    }

    fn stage_merge_blocks(
        &self,
        staging: &mut StagingState,
        previous: BlockId,
        current: BlockId,
    ) -> Result<(), String> {
        let current_position = position_of(&staging.records, current)
            .ok_or_else(|| format!("block {current} does not exist"))?;
        if subtree_end(&staging.records, current_position) != current_position + 1 {
            return Err(format!("block {current} has children and cannot be merged"));
        }
        let current_text = self.staged_payload(staging, current)?.plain_text();
        {
            let record = self.staged_payload(staging, previous)?;
            record.payload = append_plain_text_to_payload(record.payload.clone(), current_text);
        }
        staging.records.remove(current_position);
        staging.payloads.remove(&current);
        staging.touched.remove(&current);
        staging.deleted.insert(current);
        staging.structure_changed = true;
        Ok(())
    }
}
