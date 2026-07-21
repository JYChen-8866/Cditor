pub mod attrs;
pub mod block_kind;
pub mod columns;
pub mod document;
pub mod inline;
pub mod payload;
pub mod span_splice;
pub mod table;

pub use attrs::{BlockAttrs, TextAlign};
pub use block_kind::{
    CalloutVariant, LayoutBehavior, RichBlockKind, kind_tag_for_rich_block_kind,
    rich_block_kind_from_tag,
};
pub use columns::{ColumnsStructureError, columns_payload_references, validate_columns_structure};
pub use document::{
    AssetRef, CoverPositionY, DocumentMetadata, PageCover, PageIcon, RichBlockRecord,
    RichTextDocument, RichTextFormatVersion, SortKey,
};
pub use inline::{InlineColorTarget, InlineMark, InlineSpan, plain_text_from_spans};
pub use payload::{
    BlockPayload, BlockPayloadRecord, BlockPayloadView, CollectionPayload, CollectionPropertyKind,
    CollectionPropertyPayload, CollectionViewLayout, CollectionViewPayload, ColumnsGroupPayload,
    EmbedPayload, FilePayload, ImagePayload, RichTextContent, WhiteboardPayload,
};
pub use span_splice::{DelimiterPairDetection, detect_delimiter_at_caret, splice_spans_at_range};
pub use table::{
    TableCellAlign, TableCellMerge, TableCellPayload, TableCellStyle, TableColumnPayload,
    TableHeaderStyle, TablePayload, TableRange, TableRowPayload, TableTrackSize,
};
