use super::payload::{
    DbInlineSpan, DbTableCell, DbTableCellAlign, DbTableColumn, DbTableRow, DbTableTrackSize,
};
use super::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DbEditTransaction {
    pub id: u64,
    pub kind: DbEditTransactionKind,
    pub timestamp: u64,
    pub ops: Vec<DbEditOperation>,
    pub inverse_ops: Vec<DbEditOperation>,
    pub affected_blocks: Vec<BlockId>,
    pub before_selection: Option<DbDocumentSelection>,
    pub after_selection: Option<DbDocumentSelection>,
    pub before_anchor: Option<DbScrollAnchor>,
    pub after_anchor: Option<DbScrollAnchor>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DbEditTransactionKind {
    Typing,
    CompositionCommit,
    Paste,
    DragDrop,
    Format,
    ExplicitCommand,
    BlockStructureChange,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DbEditOperation {
    InsertText {
        block_id: BlockId,
        offset: usize,
        text: String,
    },
    DeleteText {
        block_id: BlockId,
        start: usize,
        end: usize,
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
        block: DbBlockIndexRecord,
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
        blocks: Vec<DbBlockIndexRecord>,
    },
    DeleteBlockRange {
        start: usize,
        end: usize,
    },
    MoveBlockRange {
        start: usize,
        end: usize,
        target_index: usize,
    },
    Table {
        op: DbTableEditOperation,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DbTableEditOperation {
    SetCellText {
        block_id: BlockId,
        row: usize,
        col: usize,
        old_spans: Vec<DbInlineSpan>,
        new_spans: Vec<DbInlineSpan>,
    },
    InsertRows {
        block_id: BlockId,
        index: usize,
        rows: Vec<DbTableRow>,
    },
    DeleteRows {
        block_id: BlockId,
        index: usize,
        rows: Vec<DbTableRow>,
    },
    InsertColumns {
        block_id: BlockId,
        index: usize,
        columns: Vec<DbTableColumn>,
        cells_by_row: Vec<Vec<DbTableCell>>,
    },
    DeleteColumns {
        block_id: BlockId,
        index: usize,
        columns: Vec<DbTableColumn>,
        cells_by_row: Vec<Vec<DbTableCell>>,
    },
    ResizeRow {
        block_id: BlockId,
        row: usize,
        old_height: DbTableTrackSize,
        new_height: DbTableTrackSize,
    },
    ResizeColumn {
        block_id: BlockId,
        column: usize,
        old_width: DbTableTrackSize,
        new_width: DbTableTrackSize,
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
        range: DbTableRange,
        before: DbBlockPayload,
        after: DbBlockPayload,
    },
    SplitCell {
        block_id: BlockId,
        row: usize,
        col: usize,
        before: DbBlockPayload,
        after: DbBlockPayload,
    },
    SetCellAlign {
        block_id: BlockId,
        range: DbTableRange,
        old_aligns: Vec<Vec<DbTableCellAlign>>,
        new_align: DbTableCellAlign,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DbTableRange {
    pub start_row: usize,
    pub start_col: usize,
    pub end_row: usize,
    pub end_col: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DbBlockIndexRecord {
    pub id: BlockId,
    pub parent_id: Option<BlockId>,
    pub depth: u16,
    pub kind_tag: u16,
    pub flags: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DbDocumentSelection {
    pub anchor: DbTextPosition,
    pub focus: DbTextPosition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DbTextPosition {
    pub block_id: BlockId,
    pub offset: usize,
    pub affinity: DbTextAffinity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DbTextAffinity {
    Upstream,
    Downstream,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DbScrollAnchor {
    pub block_id: BlockId,
    pub offset_in_block: f64,
    pub viewport_y: f64,
}

impl From<&EditTransaction> for DbEditTransaction {
    fn from(tx: &EditTransaction) -> Self {
        Self {
            id: tx.id,
            kind: DbEditTransactionKind::from(tx.kind),
            timestamp: tx.timestamp,
            ops: tx.ops.iter().map(DbEditOperation::from).collect(),
            inverse_ops: tx.inverse_ops.iter().map(DbEditOperation::from).collect(),
            affected_blocks: tx.affected_blocks.clone(),
            before_selection: tx.before_selection.map(DbDocumentSelection::from),
            after_selection: tx.after_selection.map(DbDocumentSelection::from),
            before_anchor: tx.before_anchor.map(DbScrollAnchor::from),
            after_anchor: tx.after_anchor.map(DbScrollAnchor::from),
        }
    }
}

impl From<EditTransactionKind> for DbEditTransactionKind {
    fn from(kind: EditTransactionKind) -> Self {
        match kind {
            EditTransactionKind::Typing => Self::Typing,
            EditTransactionKind::CompositionCommit => Self::CompositionCommit,
            EditTransactionKind::Paste => Self::Paste,
            EditTransactionKind::DragDrop => Self::DragDrop,
            EditTransactionKind::Format => Self::Format,
            EditTransactionKind::ExplicitCommand => Self::ExplicitCommand,
            EditTransactionKind::BlockStructureChange => Self::BlockStructureChange,
        }
    }
}

impl From<&EditOperation> for DbEditOperation {
    fn from(op: &EditOperation) -> Self {
        match op {
            EditOperation::InsertText {
                block_id,
                offset,
                text,
            } => Self::InsertText {
                block_id: *block_id,
                offset: *offset,
                text: text.clone(),
            },
            EditOperation::DeleteText { block_id, range } => Self::DeleteText {
                block_id: *block_id,
                start: range.start,
                end: range.end,
            },
            EditOperation::SplitBlock {
                block_id,
                offset,
                new_block_id,
            } => Self::SplitBlock {
                block_id: *block_id,
                offset: *offset,
                new_block_id: *new_block_id,
            },
            EditOperation::MergeBlocks { previous, current } => Self::MergeBlocks {
                previous: *previous,
                current: *current,
            },
            EditOperation::InsertBlock { index, block } => Self::InsertBlock {
                index: *index,
                block: DbBlockIndexRecord::from(*block),
            },
            EditOperation::DeleteBlock { block_id } => Self::DeleteBlock {
                block_id: *block_id,
            },
            EditOperation::MoveBlock {
                block_id,
                target_index,
            } => Self::MoveBlock {
                block_id: *block_id,
                target_index: *target_index,
            },
            EditOperation::MoveBlockToParent {
                block_id,
                parent_id,
                sibling_index,
            } => Self::MoveBlockToParent {
                block_id: *block_id,
                parent_id: *parent_id,
                sibling_index: *sibling_index,
            },
            EditOperation::SetBlockKind { block_id, kind } => Self::SetBlockKind {
                block_id: *block_id,
                kind: *kind,
            },
            EditOperation::InsertBlocks { index, blocks } => Self::InsertBlocks {
                index: *index,
                blocks: blocks
                    .iter()
                    .copied()
                    .map(DbBlockIndexRecord::from)
                    .collect(),
            },
            EditOperation::DeleteBlockRange { range } => Self::DeleteBlockRange {
                start: range.start,
                end: range.end,
            },
            EditOperation::MoveBlockRange {
                range,
                target_index,
            } => Self::MoveBlockRange {
                start: range.start,
                end: range.end,
                target_index: *target_index,
            },
            EditOperation::Table(op) => Self::Table {
                op: DbTableEditOperation::from(op),
            },
        }
    }
}

impl From<&TableEditOperation> for DbTableEditOperation {
    fn from(op: &TableEditOperation) -> Self {
        match op {
            TableEditOperation::SetCellText {
                block_id,
                row,
                col,
                old_spans,
                new_spans,
            } => Self::SetCellText {
                block_id: *block_id,
                row: *row,
                col: *col,
                old_spans: old_spans.iter().map(DbInlineSpan::from).collect(),
                new_spans: new_spans.iter().map(DbInlineSpan::from).collect(),
            },
            TableEditOperation::InsertRows {
                block_id,
                index,
                rows,
            } => Self::InsertRows {
                block_id: *block_id,
                index: *index,
                rows: rows.iter().map(DbTableRow::from).collect(),
            },
            TableEditOperation::DeleteRows {
                block_id,
                index,
                rows,
            } => Self::DeleteRows {
                block_id: *block_id,
                index: *index,
                rows: rows.iter().map(DbTableRow::from).collect(),
            },
            TableEditOperation::InsertColumns {
                block_id,
                index,
                columns,
                cells_by_row,
            } => Self::InsertColumns {
                block_id: *block_id,
                index: *index,
                columns: columns.iter().map(DbTableColumn::from).collect(),
                cells_by_row: cells_by_row
                    .iter()
                    .map(|cells| cells.iter().map(DbTableCell::from).collect())
                    .collect(),
            },
            TableEditOperation::DeleteColumns {
                block_id,
                index,
                columns,
                cells_by_row,
            } => Self::DeleteColumns {
                block_id: *block_id,
                index: *index,
                columns: columns.iter().map(DbTableColumn::from).collect(),
                cells_by_row: cells_by_row
                    .iter()
                    .map(|cells| cells.iter().map(DbTableCell::from).collect())
                    .collect(),
            },
            TableEditOperation::ResizeRow {
                block_id,
                row,
                old_height,
                new_height,
            } => Self::ResizeRow {
                block_id: *block_id,
                row: *row,
                old_height: DbTableTrackSize::from(*old_height),
                new_height: DbTableTrackSize::from(*new_height),
            },
            TableEditOperation::ResizeColumn {
                block_id,
                column,
                old_width,
                new_width,
            } => Self::ResizeColumn {
                block_id: *block_id,
                column: *column,
                old_width: DbTableTrackSize::from(*old_width),
                new_width: DbTableTrackSize::from(*new_width),
            },
            TableEditOperation::MoveRows {
                block_id,
                from,
                to,
                count,
            } => Self::MoveRows {
                block_id: *block_id,
                from: *from,
                to: *to,
                count: *count,
            },
            TableEditOperation::MoveColumns {
                block_id,
                from,
                to,
                count,
            } => Self::MoveColumns {
                block_id: *block_id,
                from: *from,
                to: *to,
                count: *count,
            },
            TableEditOperation::MergeCells {
                block_id,
                range,
                before,
                after,
            } => Self::MergeCells {
                block_id: *block_id,
                range: DbTableRange::from(*range),
                before: DbBlockPayload::from(&BlockPayload::Table(before.clone())),
                after: DbBlockPayload::from(&BlockPayload::Table(after.clone())),
            },
            TableEditOperation::SplitCell {
                block_id,
                row,
                col,
                before,
                after,
            } => Self::SplitCell {
                block_id: *block_id,
                row: *row,
                col: *col,
                before: DbBlockPayload::from(&BlockPayload::Table(before.clone())),
                after: DbBlockPayload::from(&BlockPayload::Table(after.clone())),
            },
            TableEditOperation::SetCellAlign {
                block_id,
                range,
                old_aligns,
                new_align,
            } => Self::SetCellAlign {
                block_id: *block_id,
                range: DbTableRange::from(*range),
                old_aligns: old_aligns
                    .iter()
                    .map(|row| row.iter().copied().map(DbTableCellAlign::from).collect())
                    .collect(),
                new_align: DbTableCellAlign::from(*new_align),
            },
        }
    }
}

impl From<TableRange> for DbTableRange {
    fn from(range: TableRange) -> Self {
        Self {
            start_row: range.start_row,
            start_col: range.start_col,
            end_row: range.end_row,
            end_col: range.end_col,
        }
    }
}

impl From<BlockIndexRecord> for DbBlockIndexRecord {
    fn from(record: BlockIndexRecord) -> Self {
        Self {
            id: record.id,
            parent_id: record.parent_id,
            depth: record.depth,
            kind_tag: record.kind_tag,
            flags: record.flags,
        }
    }
}

impl From<DocumentSelection> for DbDocumentSelection {
    fn from(selection: DocumentSelection) -> Self {
        Self {
            anchor: DbTextPosition::from(selection.anchor),
            focus: DbTextPosition::from(selection.focus),
        }
    }
}

impl From<TextPosition> for DbTextPosition {
    fn from(position: TextPosition) -> Self {
        Self {
            block_id: position.block_id,
            offset: position.offset,
            affinity: DbTextAffinity::from(position.affinity),
        }
    }
}

impl From<TextAffinity> for DbTextAffinity {
    fn from(affinity: TextAffinity) -> Self {
        match affinity {
            TextAffinity::Upstream => Self::Upstream,
            TextAffinity::Downstream => Self::Downstream,
        }
    }
}

impl From<ScrollAnchor> for DbScrollAnchor {
    fn from(anchor: ScrollAnchor) -> Self {
        Self {
            block_id: anchor.block_id,
            offset_in_block: anchor.offset_in_block,
            viewport_y: anchor.viewport_y,
        }
    }
}

pub fn encode_edit_transaction(tx: &EditTransaction) -> serde_json::Result<serde_json::Value> {
    serde_json::to_value(DbEditTransaction::from(tx))
}

pub fn decode_edit_transaction(value: serde_json::Value) -> serde_json::Result<EditTransaction> {
    serde_json::from_value::<DbEditTransaction>(value).map(EditTransaction::from)
}

impl From<DbEditTransaction> for EditTransaction {
    fn from(tx: DbEditTransaction) -> Self {
        Self {
            id: tx.id,
            ops: tx.ops.into_iter().map(EditOperation::from).collect(),
            inverse_ops: tx
                .inverse_ops
                .into_iter()
                .map(EditOperation::from)
                .collect(),
            affected_blocks: tx.affected_blocks,
            before_selection: tx.before_selection.map(DocumentSelection::from),
            after_selection: tx.after_selection.map(DocumentSelection::from),
            before_anchor: tx.before_anchor.map(ScrollAnchor::from),
            after_anchor: tx.after_anchor.map(ScrollAnchor::from),
            timestamp: tx.timestamp,
            kind: EditTransactionKind::from(tx.kind),
        }
    }
}

impl From<DbEditTransactionKind> for EditTransactionKind {
    fn from(kind: DbEditTransactionKind) -> Self {
        match kind {
            DbEditTransactionKind::Typing => Self::Typing,
            DbEditTransactionKind::CompositionCommit => Self::CompositionCommit,
            DbEditTransactionKind::Paste => Self::Paste,
            DbEditTransactionKind::DragDrop => Self::DragDrop,
            DbEditTransactionKind::Format => Self::Format,
            DbEditTransactionKind::ExplicitCommand => Self::ExplicitCommand,
            DbEditTransactionKind::BlockStructureChange => Self::BlockStructureChange,
        }
    }
}

impl From<DbEditOperation> for EditOperation {
    fn from(op: DbEditOperation) -> Self {
        match op {
            DbEditOperation::InsertText {
                block_id,
                offset,
                text,
            } => Self::InsertText {
                block_id,
                offset,
                text,
            },
            DbEditOperation::DeleteText {
                block_id,
                start,
                end,
            } => Self::DeleteText {
                block_id,
                range: Range { start, end },
            },
            DbEditOperation::SplitBlock {
                block_id,
                offset,
                new_block_id,
            } => Self::SplitBlock {
                block_id,
                offset,
                new_block_id,
            },
            DbEditOperation::MergeBlocks { previous, current } => {
                Self::MergeBlocks { previous, current }
            }
            DbEditOperation::InsertBlock { index, block } => Self::InsertBlock {
                index,
                block: BlockIndexRecord::from(block),
            },
            DbEditOperation::DeleteBlock { block_id } => Self::DeleteBlock { block_id },
            DbEditOperation::MoveBlock {
                block_id,
                target_index,
            } => Self::MoveBlock {
                block_id,
                target_index,
            },
            DbEditOperation::MoveBlockToParent {
                block_id,
                parent_id,
                sibling_index,
            } => Self::MoveBlockToParent {
                block_id,
                parent_id,
                sibling_index,
            },
            DbEditOperation::SetBlockKind { block_id, kind } => {
                Self::SetBlockKind { block_id, kind }
            }
            DbEditOperation::InsertBlocks { index, blocks } => Self::InsertBlocks {
                index,
                blocks: blocks.into_iter().map(BlockIndexRecord::from).collect(),
            },
            DbEditOperation::DeleteBlockRange { start, end } => Self::DeleteBlockRange {
                range: Range { start, end },
            },
            DbEditOperation::MoveBlockRange {
                start,
                end,
                target_index,
            } => Self::MoveBlockRange {
                range: Range { start, end },
                target_index,
            },
            DbEditOperation::Table { op } => Self::Table(TableEditOperation::from(op)),
        }
    }
}

impl From<DbTableEditOperation> for TableEditOperation {
    fn from(op: DbTableEditOperation) -> Self {
        match op {
            DbTableEditOperation::SetCellText {
                block_id,
                row,
                col,
                old_spans,
                new_spans,
            } => Self::SetCellText {
                block_id,
                row,
                col,
                old_spans: old_spans.into_iter().map(InlineSpan::from).collect(),
                new_spans: new_spans.into_iter().map(InlineSpan::from).collect(),
            },
            DbTableEditOperation::InsertRows {
                block_id,
                index,
                rows,
            } => Self::InsertRows {
                block_id,
                index,
                rows: rows.into_iter().map(TableRowPayload::from).collect(),
            },
            DbTableEditOperation::DeleteRows {
                block_id,
                index,
                rows,
            } => Self::DeleteRows {
                block_id,
                index,
                rows: rows.into_iter().map(TableRowPayload::from).collect(),
            },
            DbTableEditOperation::InsertColumns {
                block_id,
                index,
                columns,
                cells_by_row,
            } => Self::InsertColumns {
                block_id,
                index,
                columns: columns.into_iter().map(TableColumnPayload::from).collect(),
                cells_by_row: cells_by_row
                    .into_iter()
                    .map(|cells| cells.into_iter().map(TableCellPayload::from).collect())
                    .collect(),
            },
            DbTableEditOperation::DeleteColumns {
                block_id,
                index,
                columns,
                cells_by_row,
            } => Self::DeleteColumns {
                block_id,
                index,
                columns: columns.into_iter().map(TableColumnPayload::from).collect(),
                cells_by_row: cells_by_row
                    .into_iter()
                    .map(|cells| cells.into_iter().map(TableCellPayload::from).collect())
                    .collect(),
            },
            DbTableEditOperation::ResizeRow {
                block_id,
                row,
                old_height,
                new_height,
            } => Self::ResizeRow {
                block_id,
                row,
                old_height: TableTrackSize::from(old_height),
                new_height: TableTrackSize::from(new_height),
            },
            DbTableEditOperation::ResizeColumn {
                block_id,
                column,
                old_width,
                new_width,
            } => Self::ResizeColumn {
                block_id,
                column,
                old_width: TableTrackSize::from(old_width),
                new_width: TableTrackSize::from(new_width),
            },
            DbTableEditOperation::MoveRows {
                block_id,
                from,
                to,
                count,
            } => Self::MoveRows {
                block_id,
                from,
                to,
                count,
            },
            DbTableEditOperation::MoveColumns {
                block_id,
                from,
                to,
                count,
            } => Self::MoveColumns {
                block_id,
                from,
                to,
                count,
            },
            DbTableEditOperation::MergeCells {
                block_id,
                range,
                before,
                after,
            } => Self::MergeCells {
                block_id,
                range: TableRange::from(range),
                before: table_payload_from_db_payload(before),
                after: table_payload_from_db_payload(after),
            },
            DbTableEditOperation::SplitCell {
                block_id,
                row,
                col,
                before,
                after,
            } => Self::SplitCell {
                block_id,
                row,
                col,
                before: table_payload_from_db_payload(before),
                after: table_payload_from_db_payload(after),
            },
            DbTableEditOperation::SetCellAlign {
                block_id,
                range,
                old_aligns,
                new_align,
            } => Self::SetCellAlign {
                block_id,
                range: TableRange::from(range),
                old_aligns: old_aligns
                    .into_iter()
                    .map(|row| row.into_iter().map(TableCellAlign::from).collect())
                    .collect(),
                new_align: TableCellAlign::from(new_align),
            },
        }
    }
}

impl From<DbTableRange> for TableRange {
    fn from(range: DbTableRange) -> Self {
        Self {
            start_row: range.start_row,
            start_col: range.start_col,
            end_row: range.end_row,
            end_col: range.end_col,
        }
    }
}

fn table_payload_from_db_payload(payload: DbBlockPayload) -> TablePayload {
    match BlockPayload::from(payload) {
        BlockPayload::Table(table) => table,
        _ => TablePayload::default(),
    }
}

impl From<DbBlockIndexRecord> for BlockIndexRecord {
    fn from(record: DbBlockIndexRecord) -> Self {
        BlockIndexRecord::new(
            record.id,
            record.parent_id,
            record.depth,
            record.kind_tag,
            record.flags,
        )
    }
}

impl From<DbDocumentSelection> for DocumentSelection {
    fn from(selection: DbDocumentSelection) -> Self {
        Self {
            anchor: TextPosition::from(selection.anchor),
            focus: TextPosition::from(selection.focus),
        }
    }
}

impl From<DbTextPosition> for TextPosition {
    fn from(position: DbTextPosition) -> Self {
        Self {
            block_id: position.block_id,
            offset: position.offset,
            affinity: TextAffinity::from(position.affinity),
        }
    }
}

impl From<DbTextAffinity> for TextAffinity {
    fn from(affinity: DbTextAffinity) -> Self {
        match affinity {
            DbTextAffinity::Upstream => Self::Upstream,
            DbTextAffinity::Downstream => Self::Downstream,
        }
    }
}

impl From<DbScrollAnchor> for ScrollAnchor {
    fn from(anchor: DbScrollAnchor) -> Self {
        Self {
            block_id: anchor.block_id,
            offset_in_block: anchor.offset_in_block,
            viewport_y: anchor.viewport_y,
        }
    }
}
