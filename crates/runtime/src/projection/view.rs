use cditor_core::block::BlockChromeSnapshot;
use cditor_core::edit::SelectionRange;
use cditor_core::ids::{BlockId, DocumentId};
use cditor_core::layout::BlockLayoutMeta;
use cditor_core::rich_text::{
    BlockAttrs, BlockPayloadView, InlineSpan, RichBlockKind, TableCellAlign, TablePayload,
};
use cditor_editor::debug_overlay::DebugOverlaySnapshot;
use cditor_editor::scroll::VirtualScrollState;
use cditor_editor::window::RenderWindow;

#[derive(Debug, Clone, PartialEq)]
pub struct EditorViewProjection {
    pub document_id: DocumentId,
    pub scroll: VirtualScrollState,
    pub render_window: RenderWindow,
    pub blocks: Vec<ViewBlockSnapshot>,
    pub ai_preview: Option<AiPreviewSnapshot>,
    pub before_window_height: f64,
    pub placeholder_window_height: Option<f64>,
    pub placeholder_window_error: Option<String>,
    pub after_window_height: f64,
    pub down_placer_height: f64,
    pub total_visible_blocks: usize,
    pub debug: DebugOverlaySnapshot,
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
pub struct AiPreviewSnapshot {
    pub request_id: u64,
    pub block_id: BlockId,
    pub anchor_offset: usize,
    pub replacement_range: Option<std::ops::Range<usize>>,
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
pub struct TableViewState {
    pub table: TablePayload,
    pub row_count: usize,
    pub col_count: usize,
    pub width_px: f32,
    pub height_px: f32,
    pub column_widths_px: Vec<f32>,
    pub row_heights_px: Vec<f32>,
    pub horizontal_scroll_offset_px: f32,
    pub visible_cells: Vec<TableVisibleCell>,
    pub focused_cell: Option<TableCellPosition>,
    pub focused_cell_offset: Option<usize>,
    pub focused_cell_selection_range: Option<std::ops::Range<usize>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TableVisibleCell {
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
pub struct ViewBlockSnapshot {
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
    pub marked_range: Option<std::ops::Range<usize>>,
    pub table_view: Option<TableViewState>,
    pub focused_table_cell: Option<TableCellPosition>,
    pub focused_table_cell_offset: Option<usize>,
    pub pinned: bool,
    pub placeholder: bool,
}
