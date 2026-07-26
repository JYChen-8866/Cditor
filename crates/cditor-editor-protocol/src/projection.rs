use std::{
    ops::{Deref, DerefMut, Range},
    sync::Arc,
};

use cditor_core::{
    block::BlockChromeSnapshot,
    edit::{SelectionRange, TextAffinity},
    ids::{BlockId, DocumentId},
    layout::BlockLayoutMeta,
    rich_text::{
        BlockAttrs, BlockPayload, BlockPayloadRecord, BlockPayloadView, InlineSpan, RichBlockKind,
        TableCellAlign, TablePayload,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectionRequest {
    pub viewport_revision: u64,
    pub include_diagnostics: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditorProjection<Scroll, Window, Diagnostics> {
    pub document_id: DocumentId,
    pub viewport_revision: u64,
    pub window_generation: u64,
    pub scroll: Scroll,
    pub render_window: Window,
    pub payload_visible_block_range: Range<usize>,
    pub payload_prefetch_block_range: Range<usize>,
    pub payload_prefetch_resident: bool,
    pub layout_prefetch_page_range: Range<usize>,
    pub blocks: Vec<BlockProjection>,
    pub ai_preview: Option<AiPreviewProjection>,
    pub before_window_height: f64,
    pub placeholder_window_height: Option<f64>,
    pub placeholder_window_error: Option<String>,
    pub placeholder_window_failure: Option<PayloadWindowFailureProjection>,
    pub after_window_height: f64,
    pub down_placer_height: f64,
    pub total_visible_blocks: usize,
    pub debug: Diagnostics,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayloadWindowFailureProjection {
    pub message: String,
    pub attempts: u8,
    pub max_attempts: u8,
    pub automatic_retry_pending: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiPreviewKind {
    InlineCompletion,
    SelectionRewrite,
    AssistantPanel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiPreviewStatus {
    Streaming,
    Ready,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiPreviewProjection {
    pub request_id: u64,
    pub block_id: BlockId,
    pub anchor_offset: usize,
    pub replacement_range: Option<Range<usize>>,
    pub selection_fingerprint: u64,
    pub text: String,
    pub status: AiPreviewStatus,
    pub kind: AiPreviewKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableCellPosition {
    pub row: usize,
    pub col: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TableProjection {
    pub table: TablePayloadSnapshot,
    pub row_count: usize,
    pub col_count: usize,
    pub width_px: f32,
    pub height_px: f32,
    pub column_widths_px: Vec<f32>,
    pub row_heights_px: Vec<f32>,
    pub horizontal_scroll_offset_px: f32,
    pub visible_cells: Vec<TableCellProjection>,
    pub focused_cell: Option<TableCellPosition>,
    pub focused_cell_offset: Option<usize>,
    pub focused_cell_affinity: Option<TextAffinity>,
    pub focused_cell_selection_range: Option<Range<usize>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TableCellProjection {
    pub position: TableCellPosition,
    pub row_span: usize,
    pub col_span: usize,
    pub x_px: f32,
    pub y_px: f32,
    pub width_px: f32,
    pub height_px: f32,
    pub header: bool,
    pub align: TableCellAlign,
    pub background_color: Option<String>,
    pub spans: TableCellSpansSnapshot,
}

#[derive(Debug, Clone)]
pub struct TablePayloadSnapshot(TablePayloadSnapshotSource);

#[derive(Debug, Clone)]
enum TablePayloadSnapshotSource {
    Record(Arc<BlockPayloadRecord>),
    Table(Arc<TablePayload>),
}

impl TablePayloadSnapshot {
    pub fn from_record(record: Arc<BlockPayloadRecord>) -> Option<Self> {
        matches!(&record.payload, BlockPayload::Table(_))
            .then_some(Self(TablePayloadSnapshotSource::Record(record)))
    }

    pub fn from_shared_table(table: Arc<TablePayload>) -> Self {
        Self(TablePayloadSnapshotSource::Table(table))
    }

    pub fn shares_storage_with(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (
                TablePayloadSnapshotSource::Record(left),
                TablePayloadSnapshotSource::Record(right),
            ) => Arc::ptr_eq(left, right),
            (TablePayloadSnapshotSource::Table(left), TablePayloadSnapshotSource::Table(right)) => {
                Arc::ptr_eq(left, right)
            }
            _ => false,
        }
    }
}

impl Deref for TablePayloadSnapshot {
    type Target = TablePayload;

    fn deref(&self) -> &Self::Target {
        match &self.0 {
            TablePayloadSnapshotSource::Record(record) => {
                let BlockPayload::Table(table) = &record.payload else {
                    unreachable!("table payload snapshot validates its resident record")
                };
                table
            }
            TablePayloadSnapshotSource::Table(table) => table,
        }
    }
}

impl DerefMut for TablePayloadSnapshot {
    fn deref_mut(&mut self) -> &mut Self::Target {
        if let TablePayloadSnapshotSource::Record(record) = &self.0 {
            let BlockPayload::Table(table) = &record.payload else {
                unreachable!("table payload snapshot validates its resident record")
            };
            self.0 = TablePayloadSnapshotSource::Table(Arc::new(table.clone()));
        }
        let TablePayloadSnapshotSource::Table(table) = &mut self.0 else {
            unreachable!("resident table snapshot is promoted before mutation")
        };
        Arc::make_mut(table)
    }
}

impl AsRef<TablePayload> for TablePayloadSnapshot {
    fn as_ref(&self) -> &TablePayload {
        self
    }
}

impl PartialEq for TablePayloadSnapshot {
    fn eq(&self, other: &Self) -> bool {
        self.shares_storage_with(other) || self.as_ref() == other.as_ref()
    }
}

impl Eq for TablePayloadSnapshot {}

impl Default for TablePayloadSnapshot {
    fn default() -> Self {
        Self::from(TablePayload::default())
    }
}

impl From<TablePayload> for TablePayloadSnapshot {
    fn from(table: TablePayload) -> Self {
        Self::from_shared_table(Arc::new(table))
    }
}

impl From<Arc<TablePayload>> for TablePayloadSnapshot {
    fn from(table: Arc<TablePayload>) -> Self {
        Self::from_shared_table(table)
    }
}

#[derive(Debug, Clone)]
pub struct TableCellSpansSnapshot(TableCellSpansSource);

#[derive(Debug, Clone)]
enum TableCellSpansSource {
    TableCell {
        table: TablePayloadSnapshot,
        row: usize,
        col: usize,
    },
    Owned(Arc<[InlineSpan]>),
}

impl TableCellSpansSnapshot {
    pub fn from_table_cell(table: TablePayloadSnapshot, row: usize, col: usize) -> Self {
        Self(TableCellSpansSource::TableCell { table, row, col })
    }

    pub fn shares_storage_with(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (
                TableCellSpansSource::TableCell {
                    table: left,
                    row: left_row,
                    col: left_col,
                },
                TableCellSpansSource::TableCell {
                    table: right,
                    row: right_row,
                    col: right_col,
                },
            ) => left_row == right_row && left_col == right_col && left.shares_storage_with(right),
            (TableCellSpansSource::Owned(left), TableCellSpansSource::Owned(right)) => {
                Arc::ptr_eq(left, right)
            }
            _ => false,
        }
    }
}

impl Deref for TableCellSpansSnapshot {
    type Target = [InlineSpan];

    fn deref(&self) -> &Self::Target {
        match &self.0 {
            TableCellSpansSource::TableCell { table, row, col } => table
                .rows
                .get(*row)
                .and_then(|row| row.cells.get(*col))
                .map(|cell| cell.spans.as_slice())
                .unwrap_or_default(),
            TableCellSpansSource::Owned(spans) => spans,
        }
    }
}

impl AsRef<[InlineSpan]> for TableCellSpansSnapshot {
    fn as_ref(&self) -> &[InlineSpan] {
        self
    }
}

impl PartialEq for TableCellSpansSnapshot {
    fn eq(&self, other: &Self) -> bool {
        self.shares_storage_with(other) || self.as_ref() == other.as_ref()
    }
}

impl Eq for TableCellSpansSnapshot {}

impl Default for TableCellSpansSnapshot {
    fn default() -> Self {
        Self(TableCellSpansSource::Owned(Arc::from([])))
    }
}

impl From<Vec<InlineSpan>> for TableCellSpansSnapshot {
    fn from(spans: Vec<InlineSpan>) -> Self {
        Self(TableCellSpansSource::Owned(Arc::from(spans)))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlockProjection {
    pub block_id: BlockId,
    pub visible_index: usize,
    pub depth: u16,
    pub chrome: BlockChromeSnapshot,
    pub kind: RichBlockKind,
    pub attrs: BlockAttrs,
    pub payload: BlockPayloadView,
    pub layout: BlockLayoutMeta,
    pub selected: bool,
    pub selection_range: Option<SelectionRange>,
    pub selection_overlay: bool,
    pub focused: bool,
    pub caret_offset: Option<usize>,
    pub caret_affinity: Option<TextAffinity>,
    pub marked_range: Option<Range<usize>>,
    pub table_view: Option<TableProjection>,
    pub focused_table_cell: Option<TableCellPosition>,
    pub focused_table_cell_offset: Option<usize>,
    pub pinned: bool,
    pub placeholder: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionProjection {
    pub anchor_block_id: BlockId,
    pub anchor_offset: usize,
    pub head_block_id: BlockId,
    pub head_offset: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SurfaceProjection {
    Document,
    TableCell {
        block_id: BlockId,
        row: usize,
        column: usize,
    },
    Whiteboard {
        block_id: BlockId,
        payload_version: u32,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_request_keeps_diagnostics_explicit() {
        let request = ProjectionRequest {
            viewport_revision: 4,
            include_diagnostics: false,
        };
        assert!(!request.include_diagnostics);
    }

    #[test]
    fn selection_projection_keeps_anchor_and_head_direction() {
        let selection = SelectionProjection {
            anchor_block_id: 1,
            anchor_offset: 8,
            head_block_id: 1,
            head_offset: 2,
        };
        assert!(selection.anchor_offset > selection.head_offset);
    }

    #[test]
    fn resident_table_snapshot_clones_shallowly_and_mutates_with_copy_on_write() {
        let record = Arc::new(BlockPayloadRecord {
            block_id: 7,
            content_version: 1,
            kind: RichBlockKind::Table,
            payload: BlockPayload::Table(TablePayload::default()),
        });
        let mut first = TablePayloadSnapshot::from_record(record.clone()).unwrap();
        let second = first.clone();

        assert!(first.shares_storage_with(&second));
        first.header_rows = 1;

        assert!(!first.shares_storage_with(&second));
        assert_eq!(first.header_rows, 1);
        assert_eq!(second.header_rows, 0);
        let BlockPayload::Table(resident) = &record.payload else {
            panic!("resident record stays a table");
        };
        assert_eq!(resident.header_rows, 0);
    }

    #[test]
    fn table_cell_span_snapshots_share_the_resident_text_allocation() {
        let mut table = TablePayload::default();
        table.rows = vec![cditor_core::rich_text::TableRowPayload {
            cells: vec![cditor_core::rich_text::TableCellPayload::plain(
                "shared cell",
            )],
            height: Default::default(),
        }];
        let table = TablePayloadSnapshot::from_record(Arc::new(BlockPayloadRecord {
            block_id: 7,
            content_version: 1,
            kind: RichBlockKind::Table,
            payload: BlockPayload::Table(table),
        }))
        .unwrap();

        let first = TableCellSpansSnapshot::from_table_cell(table.clone(), 0, 0);
        let second = TableCellSpansSnapshot::from_table_cell(table, 0, 0);

        assert!(first.shares_storage_with(&second));
        assert_eq!(first[0].text.as_ptr(), second[0].text.as_ptr());
    }

    #[test]
    fn table_cell_span_snapshot_equality_keeps_value_semantics_across_storage() {
        let first = TableCellSpansSnapshot::from(vec![InlineSpan::plain("same")]);
        let second = TableCellSpansSnapshot::from(vec![InlineSpan::plain("same")]);

        assert!(!first.shares_storage_with(&second));
        assert_eq!(first, second);
    }
}
