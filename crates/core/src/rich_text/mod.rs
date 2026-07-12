pub mod attrs;
pub mod block_kind;
pub mod clipboard;
pub mod document;
pub mod inline;
pub mod markdown;
pub mod markdown_stats;
pub mod payload;
pub mod span_splice;
pub mod table;

pub use attrs::{BlockAttrs, TextAlign};
pub use block_kind::{
    CalloutVariant, LayoutBehavior, RichBlockKind, kind_tag_for_rich_block_kind,
    rich_block_kind_from_tag,
};
pub use clipboard::{
    CditorClipboardEnvelope, ClipboardBlock, ClipboardBlockFragment, ClipboardDecodeError,
    ClipboardFragmentBoundary, ClipboardSelection,
};
pub use document::{
    AssetRef, CoverPositionY, DocumentMetadata, PageCover, PageIcon, RichBlockRecord,
    RichTextDocument, RichTextFormatVersion, SortKey,
};
pub use inline::{InlineMark, InlineSpan, plain_text_from_spans};
pub use markdown::{
    MarkdownImportOptions, ParsedMarkdownDocument, block_kind_shortcut,
    block_kind_shortcut_with_marker_len, code_fence_shortcut, export_plain_markdown,
    import_markdown_block_incremental, import_markdown_inline_incremental,
    looks_like_markdown_paste, markdown_inline_shortcut_spans, parse_callout_marker,
    parse_markdown_document,
};
pub use markdown_stats::{MARKDOWN_PARSE_STATS, MarkdownParseStats, MarkdownParseStatsSnapshot};
pub use payload::{
    BlockPayload, BlockPayloadRecord, BlockPayloadView, EmbedPayload, FilePayload, ImagePayload,
    WhiteboardPayload,
};
pub use span_splice::{DelimiterPairDetection, detect_delimiter_at_caret, splice_spans_at_range};
pub use table::{
    TableCellAlign, TableCellMerge, TableCellPayload, TableCellStyle, TableColumnPayload,
    TableHeaderStyle, TablePayload, TableRange, TableRowPayload, TableTrackSize,
};
