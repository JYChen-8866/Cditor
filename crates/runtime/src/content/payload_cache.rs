use std::mem::size_of;

use cditor_core::rich_text::{
    BlockPayload, BlockPayloadRecord, InlineMark, InlineSpan, RichBlockKind, TablePayload,
};

/// Default resident payload budget for a PostgreSQL-backed document.
///
/// The structure index, height index and stable layout boxes stay resident;
/// this budget applies only to heavyweight block payload entities.
pub const DEFAULT_POSTGRES_PAYLOAD_CACHE_MAX_ENTRIES: usize = 2_048;
pub const DEFAULT_POSTGRES_PAYLOAD_CACHE_MAX_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PayloadCachePolicy {
    pub max_entries: usize,
    pub max_estimated_bytes: usize,
}

impl PayloadCachePolicy {
    pub const fn persistent_default() -> Self {
        Self {
            max_entries: DEFAULT_POSTGRES_PAYLOAD_CACHE_MAX_ENTRIES,
            max_estimated_bytes: DEFAULT_POSTGRES_PAYLOAD_CACHE_MAX_BYTES,
        }
    }
}

impl Default for PayloadCachePolicy {
    fn default() -> Self {
        Self::persistent_default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PayloadCacheTrimReport {
    pub before_entries: usize,
    pub after_entries: usize,
    pub before_estimated_bytes: usize,
    pub after_estimated_bytes: usize,
    pub evicted_entries: usize,
    pub evicted_block_ids: Vec<cditor_core::ids::BlockId>,
    /// True when dirty or pinned entities alone keep the cache above policy.
    pub over_capacity: bool,
}

pub(crate) fn estimated_payload_record_bytes(record: &BlockPayloadRecord) -> usize {
    size_of::<BlockPayloadRecord>()
        .saturating_add(estimated_kind_heap_bytes(&record.kind))
        .saturating_add(estimated_payload_heap_bytes(&record.payload))
        .saturating_add(estimated_runtime_mirror_bytes(&record.payload))
}

fn estimated_kind_heap_bytes(kind: &RichBlockKind) -> usize {
    match kind {
        RichBlockKind::Code {
            language: Some(language),
        } => language.capacity(),
        RichBlockKind::Custom(name) => name.capacity(),
        _ => 0,
    }
}

fn estimated_payload_heap_bytes(payload: &BlockPayload) -> usize {
    match payload {
        BlockPayload::RichText { spans } => estimated_spans_bytes(spans, spans.capacity()),
        BlockPayload::Code { language, text } => language
            .as_ref()
            .map_or(0, String::capacity)
            .saturating_add(text.capacity()),
        BlockPayload::Table(table) => estimated_table_bytes(table),
        BlockPayload::Image(image) => image
            .source
            .capacity()
            .saturating_add(image.alt.capacity())
            .saturating_add(image.caption.capacity()),
        BlockPayload::File(file) => file.name.capacity().saturating_add(file.source.capacity()),
        BlockPayload::Whiteboard(whiteboard) => whiteboard.scene_json.capacity(),
        BlockPayload::Embed(embed) => embed.url.capacity().saturating_add(embed.title.capacity()),
        BlockPayload::Html { html, .. } => html.capacity(),
        BlockPayload::Empty => 0,
    }
}

/// DocumentRuntime keeps editable text and table state in dedicated entities.
/// Account for those mirrors so the byte budget reflects resident memory rather
/// than only the serialized payload copy.
fn estimated_runtime_mirror_bytes(payload: &BlockPayload) -> usize {
    match payload {
        BlockPayload::RichText { spans } => size_of::<String>()
            .saturating_add(size_of::<Vec<crate::editing::hot_path::InlineRun>>())
            .saturating_add(spans.iter().map(|span| span.text.len()).sum::<usize>()),
        BlockPayload::Code { text, .. } | BlockPayload::Html { html: text, .. } => {
            size_of::<String>()
                .saturating_add(size_of::<Vec<crate::editing::hot_path::InlineRun>>())
                .saturating_add(text.len())
        }
        BlockPayload::Table(table) => {
            size_of::<TablePayload>().saturating_add(estimated_table_bytes(table))
        }
        _ => 0,
    }
}

fn estimated_spans_bytes(spans: &[InlineSpan], capacity: usize) -> usize {
    capacity
        .saturating_mul(size_of::<InlineSpan>())
        .saturating_add(
            spans
                .iter()
                .map(|span| {
                    span.text
                        .capacity()
                        .saturating_add(
                            span.marks
                                .capacity()
                                .saturating_mul(size_of::<InlineMark>()),
                        )
                        .saturating_add(
                            span.marks
                                .iter()
                                .map(estimated_mark_heap_bytes)
                                .sum::<usize>(),
                        )
                })
                .sum::<usize>(),
        )
}

fn estimated_mark_heap_bytes(mark: &InlineMark) -> usize {
    match mark {
        InlineMark::Link { href } => href.capacity(),
        InlineMark::Color(color) | InlineMark::Background(color) => color.capacity(),
        _ => 0,
    }
}

fn estimated_table_bytes(table: &TablePayload) -> usize {
    let rows = table
        .rows
        .capacity()
        .saturating_mul(size_of::<cditor_core::rich_text::TableRowPayload>())
        .saturating_add(
            table
                .rows
                .iter()
                .map(|row| {
                    row.cells
                        .capacity()
                        .saturating_mul(size_of::<cditor_core::rich_text::TableCellPayload>())
                        .saturating_add(
                            row.cells
                                .iter()
                                .map(|cell| {
                                    estimated_spans_bytes(&cell.spans, cell.spans.capacity())
                                        .saturating_add(
                                            cell.style
                                                .background_color
                                                .as_ref()
                                                .map_or(0, String::capacity),
                                        )
                                })
                                .sum::<usize>(),
                        )
                })
                .sum::<usize>(),
        );
    rows.saturating_add(
        table
            .columns
            .capacity()
            .saturating_mul(size_of::<cditor_core::rich_text::TableColumnPayload>()),
    )
    .saturating_add(
        table
            .header_style
            .row_background_color
            .as_ref()
            .map_or(0, String::capacity),
    )
    .saturating_add(
        table
            .header_style
            .column_background_color
            .as_ref()
            .map_or(0, String::capacity),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use cditor_core::rich_text::{InlineSpan, RichBlockKind};

    #[test]
    fn estimator_grows_with_dynamic_text() {
        let small = BlockPayloadRecord::rich_text(1, RichBlockKind::Paragraph, "a");
        let large = BlockPayloadRecord::rich_text(2, RichBlockKind::Paragraph, "x".repeat(16_384));

        assert!(estimated_payload_record_bytes(&large) > estimated_payload_record_bytes(&small));
    }

    #[test]
    fn estimator_counts_inline_mark_storage() {
        let plain = BlockPayloadRecord::rich_text(1, RichBlockKind::Paragraph, "text");
        let marked = BlockPayloadRecord {
            block_id: 2,
            content_version: 1,
            kind: RichBlockKind::Paragraph,
            payload: BlockPayload::RichText {
                spans: vec![InlineSpan {
                    text: "text".to_owned(),
                    marks: vec![InlineMark::Link {
                        href: "https://example.com/a/long/link".to_owned(),
                    }],
                }],
            },
        };

        assert!(estimated_payload_record_bytes(&marked) > estimated_payload_record_bytes(&plain));
    }
}
