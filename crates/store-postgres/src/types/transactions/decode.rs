use super::*;

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
            DbEditTransactionKind::AiApply => Self::AiApply,
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
