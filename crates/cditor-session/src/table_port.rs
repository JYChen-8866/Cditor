use std::ops::Range;

use cditor_core::{
    edit::TextAffinity,
    ids::BlockId,
    rich_text::{BlockPayload, TableRange},
};
use cditor_editor_protocol::{ProtocolError, ProtocolErrorCode};
use cditor_runtime::DocumentRuntime;

use crate::EditorSessionHandle;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusedTableCellSnapshot {
    pub block_id: BlockId,
    pub row: usize,
    pub col: usize,
    pub offset: usize,
    pub affinity: TextAffinity,
    pub selected_range: Range<usize>,
    pub selection_reversed: bool,
    pub marked_range: Option<Range<usize>>,
    pub block_content_version: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TablePayloadSummary {
    pub block_id: BlockId,
    pub rows: usize,
    pub cols: usize,
    pub content_version: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableInteractionSnapshot {
    pub revision: u64,
    pub focused_block_id: Option<BlockId>,
    pub focused_cell: Option<FocusedTableCellSnapshot>,
    pub requested_table: Option<TablePayloadSummary>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableRangeRequest {
    Cell { row: usize, col: usize },
    Range(TableRange),
    Row(usize),
    Column(usize),
}

pub fn project_table_interaction(
    runtime: &DocumentRuntime,
    requested_block_id: Option<BlockId>,
) -> TableInteractionSnapshot {
    let focused_cell_position = runtime.focused_table_cell_text_position();
    eprintln!(
        "[table_port] focused_table_cell_text_position: {:?}",
        focused_cell_position
    );

    let focused_cell = focused_cell_position.map(|(block_id, row, col, offset, affinity)| {
        let (_, _, _, selected_range, selection_reversed, marked_range) = runtime
            .focused_table_cell_selection_state()
            .expect("focused table position and selection state must share ownership");
        FocusedTableCellSnapshot {
            block_id,
            row,
            col,
            offset,
            affinity,
            selected_range,
            selection_reversed,
            marked_range,
            block_content_version: runtime.block_content_version(block_id).unwrap_or(0),
        }
    });
    let requested_table = requested_block_id.and_then(|block_id| {
        let payload = runtime.block_payload_record(block_id)?;
        let BlockPayload::Table(table) = &payload.payload else {
            return None;
        };
        Some(TablePayloadSummary {
            block_id,
            rows: table.rows.len(),
            cols: table.rows.first().map(|row| row.cells.len()).unwrap_or(0),
            content_version: payload.content_version,
        })
    });
    TableInteractionSnapshot {
        revision: runtime.revision(),
        focused_block_id: runtime.focused_block_id(),
        focused_cell,
        requested_table,
    }
}

pub fn project_table_range(
    runtime: &DocumentRuntime,
    block_id: BlockId,
    request: TableRangeRequest,
) -> Option<TableRange> {
    match request {
        TableRangeRequest::Cell { row, col } => {
            runtime.table_cell_selection_range(block_id, row, col)
        }
        TableRangeRequest::Range(range) => runtime.table_range_selection_range(block_id, range),
        TableRangeRequest::Row(row) => runtime.table_row_selection_range(block_id, row),
        TableRangeRequest::Column(col) => runtime.table_column_selection_range(block_id, col),
    }
}

pub fn project_table_horizontal_scroll_offset(runtime: &DocumentRuntime, block_id: BlockId) -> f32 {
    runtime.table_horizontal_scroll_offset_px(block_id)
}

impl EditorSessionHandle {
    pub fn table_interaction(
        &self,
        requested_block_id: Option<BlockId>,
    ) -> Result<TableInteractionSnapshot, ProtocolError> {
        let session = self.inner.try_borrow().map_err(|_| table_busy())?;
        Ok(project_table_interaction(
            &session.runtime,
            requested_block_id,
        ))
    }

    pub fn table_range(
        &self,
        block_id: BlockId,
        request: TableRangeRequest,
    ) -> Result<Option<TableRange>, ProtocolError> {
        let session = self.inner.try_borrow().map_err(|_| table_busy())?;
        Ok(project_table_range(&session.runtime, block_id, request))
    }

    pub fn table_horizontal_scroll_offset(&self, block_id: BlockId) -> Result<f32, ProtocolError> {
        let session = self.inner.try_borrow().map_err(|_| table_busy())?;
        Ok(project_table_horizontal_scroll_offset(
            &session.runtime,
            block_id,
        ))
    }
}

fn table_busy() -> ProtocolError {
    ProtocolError::new(
        ProtocolErrorCode::Busy,
        "editor session is already processing a synchronous request",
    )
    .retryable()
}

#[cfg(test)]
mod tests {
    use cditor_core::rich_text::RichBlockKind;
    use cditor_editor_protocol::command::{CommandEnvelope, CommandSource, EditorCommand};

    use super::*;
    use crate::EditorSession;

    fn table_runtime() -> (DocumentRuntime, BlockId) {
        let runtime = DocumentRuntime::large_mixed_demo();
        let block_id = runtime
            .visible_block_ids()
            .iter()
            .copied()
            .find(|block_id| {
                runtime
                    .block_payload_record(*block_id)
                    .is_some_and(|payload| payload.kind == RichBlockKind::Table)
            })
            .expect("large mixed demo must include a table");
        (runtime, block_id)
    }

    #[test]
    fn interaction_snapshot_owns_focus_selection_and_payload_summary() {
        let (runtime, block_id) = table_runtime();
        let handle = EditorSession::new(runtime, false).into_handle();
        handle
            .dispatch(CommandEnvelope::new(
                EditorCommand::FocusTableCell {
                    block_id,
                    row: 0,
                    col: 0,
                    offset: Some(0),
                    affinity: TextAffinity::Downstream,
                },
                CommandSource::Automation,
            ))
            .unwrap();

        let snapshot = handle.table_interaction(Some(block_id)).unwrap();
        let focused = snapshot.focused_cell.unwrap();
        assert_eq!(
            (focused.block_id, focused.row, focused.col),
            (block_id, 0, 0)
        );
        assert!(focused.selected_range.is_empty());
        assert!(focused.block_content_version > 0);
        let table = snapshot.requested_table.unwrap();
        assert!(table.rows > 0);
        assert!(table.cols > 0);
    }

    #[test]
    fn range_projection_validates_every_ui_selection_shape() {
        let (runtime, block_id) = table_runtime();
        let handle = EditorSession::new(runtime, false).into_handle();

        assert_eq!(
            handle
                .table_range(block_id, TableRangeRequest::Cell { row: 0, col: 0 })
                .unwrap(),
            Some(TableRange::normalized(0, 0, 0, 0))
        );
        assert!(
            handle
                .table_range(block_id, TableRangeRequest::Row(usize::MAX))
                .unwrap()
                .is_none()
        );
        assert!(
            handle
                .table_range(block_id, TableRangeRequest::Column(0))
                .unwrap()
                .is_some()
        );
    }
}
