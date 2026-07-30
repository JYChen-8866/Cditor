pub mod block_editor_model;
pub mod block_layout;
pub mod block_metrics;
pub mod block_provider;
pub mod block_width;
pub mod columns;
pub mod height_index;
#[cfg(test)]
mod height_index_property_tests;
pub mod page_layout;

pub use block_editor_model::{
    BlockEditorModel, BlockFragment, BlockFragmentKind, BlockHitTestResult, BlockInnerAnchor,
    BlockInnerChange, BlockInnerOperation, BlockInnerSelection, BlockInternalScrollState,
    BlockViewport, CodeBlockEditorModel, ComplexBlockInteraction, Point, TableEditorModel,
    WheelHandling, WheelTransfer,
};
pub use block_layout::BlockLayoutMeta;
pub use block_metrics::{
    BlockHeightRule, COMPLEX_BLOCK_SHELL_CHROME_HEIGHT_PX, DEFAULT_LAYOUT_WIDTH_PX,
    IMAGE_BLOCK_ESTIMATED_HEIGHT_PX, NOTION_TABLE_CELL_LINE_HEIGHT_PX,
    NOTION_TABLE_CELL_PADDING_Y_PX, NOTION_TABLE_DEFAULT_ROW_HEIGHT_PX,
    STRUCTURED_BLOCK_CONTENT_VIEWPORT_MAX_HEIGHT_PX, TABLE_HORIZONTAL_SCROLLBAR_CHROME_HEIGHT_PX,
    TextLikeMetrics, V1_CODE_CONTENT_PADDING_BOTTOM_PX, V1_CODE_CONTENT_PADDING_TOP_PX,
    V1_CODE_FRAME_BORDER_WIDTH_PX, V1_CODE_INNER_MIN_HEIGHT_PX, V1_CODE_OUTER_MAX_HEIGHT_PX,
    V1_CODE_SCROLL_END_SPACER_PX, V1_CODE_SURFACE_GAP_PX, V1_CODE_TEXT_LINE_HEIGHT_PX,
    V1_CODE_TOOLBAR_SURFACE_HEIGHT_PX, estimate_block_height, estimate_kind_fallback_height,
    estimate_rich_spans_height, estimate_text_payload_height, estimate_wrapped_line_count,
    normalize_text_inner_measured_height, text_line_height_for_kind,
};
pub use block_provider::{
    BlockLayoutProvider, CodeBlockLayoutProvider, ImageLayoutProvider, ParagraphLayoutProvider,
    Size, StableBox, StableBoxLayoutProvider, StableBoxProvider, TableLayoutProvider,
};
pub use block_width::{
    BODY_BLOCK_CONTENT_WIDTH_PX, BlockWidthClass, FULL_BLOCK_CONTENT_WIDTH_PX,
    WIDE_BLOCK_CONTENT_WIDTH_PX, block_width_class_for_kind, layout_width_for_kind,
};
pub use columns::{
    COLUMN_WEIGHT_TOTAL, ColumnLayoutTrack, ColumnSpec, ColumnsChildHeightIndex,
    ColumnsHeightChange, ColumnsHeightIndexError, ColumnsLayout, ColumnsLayoutError,
    ColumnsLayoutModel, DEFAULT_COLUMN_GAP_PX, MAX_COLUMNS_PER_GROUP, MIN_COLUMN_WIDTH_PX,
};
pub use height_index::{
    BlockHeightIndex, BlockHeightIndexError, BlockHeightIndexView, BlockOffsetHit, HeightChange,
    HeightConfidence, HeightEstimate,
};
pub use page_layout::{
    CachedPageMismatchPolicy, CachedPageRestore, PAGE_POLICY_VERSION, PageBlockEstimate,
    PageHeightChange, PageLayout, PageLayoutIdentity, PageLayoutIndex, PageLayoutIndexError,
    PageLocalBlockOffsetHit, PageLocalHeightIndex, PageOffsetHit, PagePolicy, PageSummary,
};
