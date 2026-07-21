use super::*;
use crate::rich_text::{
    BlockPayloadRecord, InlineSpan, TableCellAlign, TableCellPayload, TableColumnPayload,
    TablePayload, TableRange, TableRowPayload, TableTrackSize,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EditOperation {
    InsertText {
        block_id: BlockId,
        offset: usize,
        text: String,
    },
    DeleteText {
        block_id: BlockId,
        range: Range<usize>,
    },
    SplitBlock {
        block_id: BlockId,
        offset: usize,
        new_block_id: BlockId,
    },
    MergeBlocks {
        previous: BlockId,
        current: BlockId,
    },
    InsertBlock {
        index: usize,
        block: BlockIndexRecord,
    },
    DeleteBlock {
        block_id: BlockId,
    },
    MoveBlock {
        block_id: BlockId,
        target_index: usize,
    },
    MoveBlockToParent {
        block_id: BlockId,
        parent_id: Option<BlockId>,
        sibling_index: usize,
    },
    SetBlockKind {
        block_id: BlockId,
        kind: u16,
    },
    InsertBlocks {
        index: usize,
        blocks: Vec<BlockIndexRecord>,
        #[serde(default)]
        payloads: Vec<BlockPayloadRecord>,
    },
    DeleteBlockRange {
        range: Range<usize>,
    },
    MoveBlockRange {
        range: Range<usize>,
        target_index: usize,
    },
    Text(TextEditOperation),
    Block(BlockEditOperation),
    Table(TableEditOperation),
    Collection(CollectionEditOperation),
    Comment(CommentEditOperation),
    Asset(AssetEditOperation),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TableEditOperation {
    SetCellText {
        block_id: BlockId,
        row: usize,
        col: usize,
        old_spans: Vec<InlineSpan>,
        new_spans: Vec<InlineSpan>,
    },
    InsertRows {
        block_id: BlockId,
        index: usize,
        rows: Vec<TableRowPayload>,
    },
    DeleteRows {
        block_id: BlockId,
        index: usize,
        rows: Vec<TableRowPayload>,
    },
    InsertColumns {
        block_id: BlockId,
        index: usize,
        columns: Vec<TableColumnPayload>,
        cells_by_row: Vec<Vec<TableCellPayload>>,
    },
    DeleteColumns {
        block_id: BlockId,
        index: usize,
        columns: Vec<TableColumnPayload>,
        cells_by_row: Vec<Vec<TableCellPayload>>,
    },
    ResizeRow {
        block_id: BlockId,
        row: usize,
        old_height: TableTrackSize,
        new_height: TableTrackSize,
    },
    ResizeColumn {
        block_id: BlockId,
        column: usize,
        old_width: TableTrackSize,
        new_width: TableTrackSize,
    },
    MoveRows {
        block_id: BlockId,
        from: usize,
        to: usize,
        count: usize,
    },
    MoveColumns {
        block_id: BlockId,
        from: usize,
        to: usize,
        count: usize,
    },
    MergeCells {
        block_id: BlockId,
        range: TableRange,
        before: TablePayload,
        after: TablePayload,
    },
    SplitCell {
        block_id: BlockId,
        row: usize,
        col: usize,
        before: TablePayload,
        after: TablePayload,
    },
    SetCellAlign {
        block_id: BlockId,
        range: TableRange,
        old_aligns: Vec<Vec<TableCellAlign>>,
        new_align: TableCellAlign,
    },
}

impl TableEditOperation {
    pub fn block_id(&self) -> BlockId {
        match self {
            Self::SetCellText { block_id, .. }
            | Self::InsertRows { block_id, .. }
            | Self::DeleteRows { block_id, .. }
            | Self::InsertColumns { block_id, .. }
            | Self::DeleteColumns { block_id, .. }
            | Self::ResizeRow { block_id, .. }
            | Self::ResizeColumn { block_id, .. }
            | Self::MoveRows { block_id, .. }
            | Self::MoveColumns { block_id, .. }
            | Self::MergeCells { block_id, .. }
            | Self::SplitCell { block_id, .. }
            | Self::SetCellAlign { block_id, .. } => *block_id,
        }
    }
}

impl EditOperation {
    pub const fn required_permission(&self) -> TransactionPermission {
        match self {
            Self::Collection(_) => TransactionPermission::ManageCollection,
            Self::Comment(_) => TransactionPermission::Comment,
            Self::Asset(_) => TransactionPermission::ManageAssets,
            _ => TransactionPermission::EditDocument,
        }
    }

    pub fn affected_blocks(&self) -> Vec<BlockId> {
        match self {
            Self::InsertText { block_id, .. }
            | Self::DeleteText { block_id, .. }
            | Self::SplitBlock { block_id, .. }
            | Self::DeleteBlock { block_id }
            | Self::MoveBlock { block_id, .. }
            | Self::MoveBlockToParent { block_id, .. }
            | Self::SetBlockKind { block_id, .. } => vec![*block_id],
            Self::MergeBlocks { previous, current } => vec![*previous, *current],
            Self::InsertBlock { block, .. } => vec![block.id],
            Self::InsertBlocks { blocks, .. } => blocks.iter().map(|block| block.id).collect(),
            Self::DeleteBlockRange { .. } | Self::MoveBlockRange { .. } => Vec::new(),
            Self::Text(operation) => operation.block_id().into_iter().collect(),
            Self::Block(operation) => vec![operation.block_id()],
            Self::Table(operation) => vec![operation.block_id()],
            Self::Collection(operation) => vec![operation.block_id()],
            Self::Comment(operation) => operation.block_id().into_iter().collect(),
            Self::Asset(operation) => vec![operation.block_id()],
        }
    }

    pub fn is_text_input(&self) -> bool {
        matches!(
            self,
            Self::InsertText { .. } | Self::DeleteText { .. } | Self::Text(_)
        )
    }

    pub fn is_structure_operation(&self) -> bool {
        !self.is_text_input()
    }

    pub fn validate_text_range(&self, offsets: &TextOffsetMap) -> Result<(), TextOffsetError> {
        match self {
            Self::InsertText { offset, .. } => offsets
                .validate_grapheme_range(InternalTextOffset(*offset)..InternalTextOffset(*offset)),
            Self::DeleteText { range, .. } => offsets.validate_grapheme_range(
                InternalTextOffset(range.start)..InternalTextOffset(range.end),
            ),
            Self::SplitBlock { offset, .. } => offsets
                .validate_grapheme_range(InternalTextOffset(*offset)..InternalTextOffset(*offset)),
            _ => Ok(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransactionPrecondition {
    DocumentRevision(u64),
    StructureVersion(u64),
    BlockContentVersion { block_id: BlockId, version: u64 },
    BlockExists(BlockId),
    BlockAbsent(BlockId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EditTransactionKind {
    Typing,
    CompositionCommit,
    Paste,
    AiApply,
    DragDrop,
    Format,
    ExplicitCommand,
    BlockStructureChange,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditTransaction {
    pub id: TransactionId,
    #[serde(default)]
    pub origin: ChangeOrigin,
    #[serde(default)]
    pub preconditions: Vec<TransactionPrecondition>,
    pub ops: Arc<Vec<EditOperation>>,
    pub inverse_ops: Arc<Vec<EditOperation>>,
    pub affected_blocks: Vec<BlockId>,
    pub before_selection: Option<DocumentSelection>,
    pub after_selection: Option<DocumentSelection>,
    #[serde(default)]
    pub before_selected_blocks: Vec<BlockId>,
    #[serde(default)]
    pub after_selected_blocks: Vec<BlockId>,
    pub before_anchor: Option<ScrollAnchor>,
    pub after_anchor: Option<ScrollAnchor>,
    pub timestamp: u64,
    pub kind: EditTransactionKind,
}

impl EditTransaction {
    pub fn new(
        id: TransactionId,
        kind: EditTransactionKind,
        timestamp: u64,
        ops: Vec<EditOperation>,
        inverse_ops: Vec<EditOperation>,
    ) -> Self {
        let mut affected_blocks = Vec::new();
        for block_id in ops.iter().flat_map(EditOperation::affected_blocks) {
            if !affected_blocks.contains(&block_id) {
                affected_blocks.push(block_id);
            }
        }
        for block_id in inverse_ops.iter().flat_map(EditOperation::affected_blocks) {
            if !affected_blocks.contains(&block_id) {
                affected_blocks.push(block_id);
            }
        }

        Self {
            id,
            origin: ChangeOrigin::User,
            preconditions: Vec::new(),
            ops: Arc::new(ops),
            inverse_ops: Arc::new(inverse_ops),
            affected_blocks,
            before_selection: None,
            after_selection: None,
            before_selected_blocks: Vec::new(),
            after_selected_blocks: Vec::new(),
            before_anchor: None,
            after_anchor: None,
            timestamp,
            kind,
        }
    }

    pub fn with_selection(
        mut self,
        before_selection: Option<DocumentSelection>,
        after_selection: Option<DocumentSelection>,
    ) -> Self {
        self.before_selection = before_selection;
        self.after_selection = after_selection;
        self
    }

    pub fn with_origin(mut self, origin: ChangeOrigin) -> Self {
        self.origin = origin;
        self
    }

    pub fn with_block_selection(
        mut self,
        before_selected_blocks: Vec<BlockId>,
        after_selected_blocks: Vec<BlockId>,
    ) -> Self {
        self.before_selected_blocks = before_selected_blocks;
        self.after_selected_blocks = after_selected_blocks;
        self
    }

    pub fn with_precondition(mut self, precondition: TransactionPrecondition) -> Self {
        self.preconditions.push(precondition);
        self
    }

    pub fn with_anchor(
        mut self,
        before_anchor: Option<ScrollAnchor>,
        after_anchor: Option<ScrollAnchor>,
    ) -> Self {
        self.before_anchor = before_anchor;
        self.after_anchor = after_anchor;
        self
    }

    pub fn insert_text(
        id: TransactionId,
        timestamp: u64,
        block_id: BlockId,
        offset: usize,
        text: impl Into<String>,
    ) -> Self {
        let text = text.into();
        let end = offset + text.len();
        Self::new(
            id,
            EditTransactionKind::Typing,
            timestamp,
            vec![EditOperation::InsertText {
                block_id,
                offset,
                text: text.clone(),
            }],
            vec![EditOperation::DeleteText {
                block_id,
                range: offset..end,
            }],
        )
    }

    pub fn paste_blocks(
        id: TransactionId,
        timestamp: u64,
        index: usize,
        blocks: Vec<BlockIndexRecord>,
    ) -> Self {
        let end = index + blocks.len();
        Self::new(
            id,
            EditTransactionKind::Paste,
            timestamp,
            vec![EditOperation::InsertBlocks {
                index,
                blocks,
                payloads: Vec::new(),
            }],
            vec![EditOperation::DeleteBlockRange { range: index..end }],
        )
    }

    pub fn inverse_transaction(&self, id: TransactionId, timestamp: u64) -> Self {
        Self {
            id,
            origin: ChangeOrigin::Undo,
            preconditions: Vec::new(),
            ops: self.inverse_ops.clone(),
            inverse_ops: self.ops.clone(),
            affected_blocks: self.affected_blocks.clone(),
            before_selection: self.after_selection,
            after_selection: self.before_selection,
            before_selected_blocks: self.after_selected_blocks.clone(),
            after_selected_blocks: self.before_selected_blocks.clone(),
            before_anchor: self.after_anchor,
            after_anchor: self.before_anchor,
            timestamp,
            kind: self.kind,
        }
    }

    pub fn requires_single_restore(&self) -> bool {
        self.before_selection.is_some()
            || self.after_selection.is_some()
            || !self.before_selected_blocks.is_empty()
            || !self.after_selected_blocks.is_empty()
            || self.before_anchor.is_some()
            || self.after_anchor.is_some()
    }

    pub(super) fn can_merge_typing_with(&self, next: &Self, max_gap_ms: u64) -> bool {
        if self.kind != EditTransactionKind::Typing || next.kind != EditTransactionKind::Typing {
            return false;
        }
        if next.timestamp.saturating_sub(self.timestamp) > max_gap_ms {
            return false;
        }
        if self.after_selection != next.before_selection {
            return false;
        }
        let Some(EditOperation::InsertText {
            block_id: left_block,
            offset: left_offset,
            text: left_text,
        }) = self.ops.last()
        else {
            return false;
        };
        let Some(EditOperation::InsertText {
            block_id: right_block,
            offset: right_offset,
            ..
        }) = next.ops.first()
        else {
            return false;
        };
        left_block == right_block && *right_offset == *left_offset + left_text.len()
    }

    pub(super) fn merge_typing(&mut self, next: EditTransaction) {
        Arc::make_mut(&mut self.ops).extend(next.ops.iter().cloned());
        Arc::make_mut(&mut self.inverse_ops).splice(0..0, next.inverse_ops.iter().cloned());
        self.timestamp = next.timestamp;
        self.after_selection = next.after_selection;
        self.after_anchor = next.after_anchor;
        for block_id in next.affected_blocks {
            if !self.affected_blocks.contains(&block_id) {
                self.affected_blocks.push(block_id);
            }
        }
    }
}
