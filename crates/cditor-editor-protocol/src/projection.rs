use std::ops::Range;

use cditor_core::{
    block::BlockChromeSnapshot,
    edit::{SelectionRange, TextAffinity},
    ids::{BlockId, DocumentId},
    layout::BlockLayoutMeta,
    rich_text::{
        BlockAttrs, BlockPayloadView, InlineSpan, RichBlockKind, TableCellAlign, TablePayload,
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
    pub scroll: Scroll,
    pub render_window: Window,
    pub payload_prefetch_block_range: Range<usize>,
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
    pub table: TablePayload,
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
    pub spans: Vec<InlineSpan>,
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
}
