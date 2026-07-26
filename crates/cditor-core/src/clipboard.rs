use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::ids::{BlockId, DocumentId};
use crate::rich_text::{
    BlockPayload, InlineMark, InlineSpan, RichBlockKind, TablePayload, plain_text_from_spans,
};

pub const CDITOR_CLIPBOARD_SCHEMA: &str = "application/x-cditor-clipboard";
pub const CDITOR_CLIPBOARD_VERSION: u16 = 2;
pub const MAX_CLIPBOARD_METADATA_BYTES: usize = 8 * 1024 * 1024;
const MAX_CLIPBOARD_BLOCKS: usize = 100_000;
const MAX_CLIPBOARD_SPANS: usize = 1_000_000;
const MAX_CLIPBOARD_CELLS: usize = 1_000_000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CditorClipboardEnvelope {
    pub schema: String,
    pub version: u16,
    pub source_document: Option<DocumentId>,
    pub selection: ClipboardSelection,
    pub checksum: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ClipboardSelection {
    Inline {
        spans: Vec<InlineSpan>,
    },
    TextFragments {
        fragments: Vec<ClipboardBlockFragment>,
    },
    Blocks {
        blocks: Vec<ClipboardBlock>,
    },
    Table {
        table: TablePayload,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClipboardBlockFragment {
    pub source_id: BlockId,
    pub parent_source_id: Option<BlockId>,
    pub depth: u16,
    pub kind: RichBlockKind,
    pub spans: Vec<InlineSpan>,
    pub boundary: ClipboardFragmentBoundary,
    pub starts_at_block_start: bool,
    pub ends_at_block_end: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClipboardFragmentBoundary {
    StartPartial,
    Full,
    EndPartial,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClipboardBlock {
    pub source_id: BlockId,
    pub parent_source_id: Option<BlockId>,
    pub depth: u16,
    pub kind: RichBlockKind,
    pub payload: BlockPayload,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardDecodeError {
    TooLarge,
    Malformed,
    UnknownSchema,
    UnsupportedVersion,
    ChecksumMismatch,
    InvalidSelection,
}

impl CditorClipboardEnvelope {
    pub fn new(
        source_document: Option<DocumentId>,
        selection: ClipboardSelection,
        system_text: &str,
    ) -> Self {
        let checksum = selection_checksum(&selection, system_text);
        Self {
            schema: CDITOR_CLIPBOARD_SCHEMA.to_owned(),
            version: CDITOR_CLIPBOARD_VERSION,
            source_document,
            selection,
            checksum,
        }
    }

    pub fn decode_metadata(json: &str, system_text: &str) -> Result<Self, ClipboardDecodeError> {
        if json.len() > MAX_CLIPBOARD_METADATA_BYTES {
            return Err(ClipboardDecodeError::TooLarge);
        }
        let envelope: Self =
            serde_json::from_str(json).map_err(|_| ClipboardDecodeError::Malformed)?;
        if envelope.schema != CDITOR_CLIPBOARD_SCHEMA {
            return Err(ClipboardDecodeError::UnknownSchema);
        }
        if envelope.version != CDITOR_CLIPBOARD_VERSION {
            return Err(ClipboardDecodeError::UnsupportedVersion);
        }
        if !envelope.selection.is_valid_for_system_text(system_text) {
            return Err(ClipboardDecodeError::InvalidSelection);
        }
        if envelope.checksum != selection_checksum(&envelope.selection, system_text) {
            return Err(ClipboardDecodeError::ChecksumMismatch);
        }
        Ok(envelope)
    }
}

impl ClipboardSelection {
    pub fn plain_text(&self) -> String {
        match self {
            Self::Inline { spans } => plain_text_from_spans(spans),
            Self::TextFragments { fragments } => fragments
                .iter()
                .map(|fragment| plain_text_from_spans(&fragment.spans))
                .collect::<Vec<_>>()
                .join("\n"),
            Self::Blocks { blocks } => blocks
                .iter()
                .map(|block| block.payload.plain_text())
                .collect::<Vec<_>>()
                .join("\n"),
            Self::Table { table } => table.plain_text(),
        }
    }

    fn is_valid_for_system_text(&self, system_text: &str) -> bool {
        if self.plain_text() != system_text {
            return false;
        }
        match self {
            Self::Inline { spans } => valid_spans(spans),
            Self::TextFragments { fragments } => {
                let total_spans = fragments.iter().try_fold(0usize, |total, fragment| {
                    total.checked_add(fragment.spans.len())
                });
                fragments.len() >= 2
                    && fragments.len() <= MAX_CLIPBOARD_BLOCKS
                    && total_spans.is_some_and(|total| total <= MAX_CLIPBOARD_SPANS)
                    && fragments.first().is_some_and(|fragment| {
                        fragment.boundary == ClipboardFragmentBoundary::StartPartial
                    })
                    && fragments.last().is_some_and(|fragment| {
                        fragment.boundary == ClipboardFragmentBoundary::EndPartial
                    })
                    && fragments[1..fragments.len() - 1].iter().all(|fragment| {
                        fragment.boundary == ClipboardFragmentBoundary::Full
                            && fragment.starts_at_block_start
                            && fragment.ends_at_block_end
                    })
                    && fragments.iter().all(|fragment| {
                        kind_accepts_rich_text_payload(&fragment.kind)
                            && valid_spans(&fragment.spans)
                    })
                    && valid_fragment_structure(fragments)
            }
            Self::Blocks { blocks } => {
                !blocks.is_empty()
                    && blocks.len() <= MAX_CLIPBOARD_BLOCKS
                    && valid_block_structure(blocks)
                    && valid_blocks(blocks)
            }
            Self::Table { table } => valid_table(table, &mut ValidationBudget::default()),
        }
    }
}

fn valid_fragment_structure(fragments: &[ClipboardBlockFragment]) -> bool {
    let mut seen = HashMap::<BlockId, u16>::with_capacity(fragments.len());
    for fragment in fragments {
        if seen.contains_key(&fragment.source_id) {
            return false;
        }
        if let Some(parent) = fragment.parent_source_id {
            let Some(parent_depth) = seen.get(&parent).copied() else {
                return false;
            };
            if parent_depth.checked_add(1) != Some(fragment.depth) {
                return false;
            }
        }
        seen.insert(fragment.source_id, fragment.depth);
    }
    true
}

fn valid_block_structure(blocks: &[ClipboardBlock]) -> bool {
    let mut seen = HashMap::<BlockId, u16>::with_capacity(blocks.len());
    for block in blocks {
        if seen.contains_key(&block.source_id) {
            return false;
        }
        if let Some(parent) = block.parent_source_id {
            let Some(parent_depth) = seen.get(&parent).copied() else {
                return false;
            };
            if parent_depth.checked_add(1) != Some(block.depth) {
                return false;
            }
        }
        seen.insert(block.source_id, block.depth);
    }
    true
}

fn kind_accepts_rich_text_payload(kind: &RichBlockKind) -> bool {
    !matches!(
        kind,
        RichBlockKind::Code { .. }
            | RichBlockKind::Html
            | RichBlockKind::Table
            | RichBlockKind::Image
            | RichBlockKind::File
            | RichBlockKind::Attachment
            | RichBlockKind::Whiteboard
            | RichBlockKind::MindMap
            | RichBlockKind::Embed
            | RichBlockKind::Divider
            | RichBlockKind::Separator
            | RichBlockKind::Database
    )
}

fn valid_spans(spans: &[InlineSpan]) -> bool {
    spans.len() <= MAX_CLIPBOARD_SPANS
        && spans.iter().all(|span| {
            span.marks.iter().all(|mark| match mark {
                InlineMark::Link { href } => safe_resource(href),
                _ => true,
            })
        })
}

#[derive(Debug, Default)]
struct ValidationBudget {
    spans: usize,
    cells: usize,
}

impl ValidationBudget {
    fn add_spans(&mut self, count: usize) -> bool {
        self.spans = self.spans.saturating_add(count);
        self.spans <= MAX_CLIPBOARD_SPANS
    }

    fn add_cells(&mut self, count: usize) -> bool {
        self.cells = self.cells.saturating_add(count);
        self.cells <= MAX_CLIPBOARD_CELLS
    }
}

fn valid_blocks(blocks: &[ClipboardBlock]) -> bool {
    let mut budget = ValidationBudget::default();
    blocks.iter().all(|block| valid_block(block, &mut budget))
}

fn valid_block(block: &ClipboardBlock, budget: &mut ValidationBudget) -> bool {
    if !kind_matches_payload(&block.kind, &block.payload) {
        return false;
    }
    match &block.payload {
        BlockPayload::RichText { spans } => budget.add_spans(spans.len()) && valid_spans(spans),
        BlockPayload::Table(table) => valid_table(table, budget),
        BlockPayload::Collection(collection) => {
            budget.add_spans(collection.title.spans.len())
                && valid_spans(&collection.title.spans)
                && collection.properties.len() <= 1_024
                && collection.views.len() <= 128
        }
        BlockPayload::Image(image) => {
            budget.add_spans(image.caption.spans.len())
                && valid_spans(&image.caption.spans)
                && safe_resource(&image.source)
        }
        BlockPayload::File(file) => safe_resource(&file.source),
        BlockPayload::Embed(embed) => safe_resource(&embed.url),
        BlockPayload::Opaque { envelope, .. } => {
            envelope.domain == crate::schema::SchemaDomain::BlockPayload
                && envelope.body_bytes().len() <= MAX_CLIPBOARD_METADATA_BYTES
        }
        _ => true,
    }
}

fn valid_table(table: &TablePayload, budget: &mut ValidationBudget) -> bool {
    if table.row_count() > MAX_CLIPBOARD_BLOCKS || table.column_count() > MAX_CLIPBOARD_BLOCKS {
        return false;
    }
    table.rows.iter().all(|row| {
        budget.add_cells(row.cells.len())
            && row
                .cells
                .iter()
                .all(|cell| budget.add_spans(cell.spans.len()) && valid_spans(&cell.spans))
    })
}

fn kind_matches_payload(kind: &RichBlockKind, payload: &BlockPayload) -> bool {
    match kind {
        RichBlockKind::Custom(_) => matches!(
            payload,
            BlockPayload::Opaque { .. } | BlockPayload::RichText { .. } | BlockPayload::Code { .. }
        ),
        RichBlockKind::Table => matches!(payload, BlockPayload::Table(_)),
        RichBlockKind::Image => matches!(payload, BlockPayload::Image(_) | BlockPayload::Empty),
        RichBlockKind::File | RichBlockKind::Attachment => {
            matches!(payload, BlockPayload::File(_) | BlockPayload::Empty)
        }
        RichBlockKind::Whiteboard => matches!(payload, BlockPayload::Whiteboard(_)),
        RichBlockKind::MindMap => {
            matches!(payload, BlockPayload::Whiteboard(_) | BlockPayload::Empty)
        }
        RichBlockKind::Embed => matches!(payload, BlockPayload::Embed(_) | BlockPayload::Empty),
        RichBlockKind::Database => {
            matches!(
                payload,
                BlockPayload::Collection(_) | BlockPayload::Table(_) | BlockPayload::Empty
            )
        }
        RichBlockKind::Divider | RichBlockKind::Separator => matches!(payload, BlockPayload::Empty),
        RichBlockKind::Code { .. } => matches!(payload, BlockPayload::Code { .. }),
        RichBlockKind::Html => matches!(payload, BlockPayload::Html { .. }),
        _ => matches!(
            payload,
            BlockPayload::RichText { .. } | BlockPayload::Code { .. }
        ),
    }
}

fn safe_resource(value: &str) -> bool {
    let value = value.trim();
    value.is_empty()
        || (!value.contains('\0')
            && !value.to_ascii_lowercase().starts_with("javascript:")
            && !value.to_ascii_lowercase().starts_with("data:text/html")
            && !value.split(['/', '\\']).any(|part| part == ".."))
}

fn selection_checksum(selection: &ClipboardSelection, system_text: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in serde_json::to_vec(selection)
        .unwrap_or_default()
        .into_iter()
        .chain(system_text.bytes())
    {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::unknown::{
        UNKNOWN_PLUGIN_BODY, UNKNOWN_PLUGIN_FALLBACK, UNKNOWN_PLUGIN_KIND,
        assert_unknown_plugin_bytes, unknown_plugin_payload,
    };
    use crate::rich_text::{TableCellPayload, TableRowPayload, WhiteboardPayload};

    fn decode_selection(
        selection: ClipboardSelection,
    ) -> Result<CditorClipboardEnvelope, ClipboardDecodeError> {
        let system_text = selection.plain_text();
        let envelope = CditorClipboardEnvelope::new(None, selection, &system_text);
        CditorClipboardEnvelope::decode_metadata(
            &serde_json::to_string(&envelope).unwrap(),
            &system_text,
        )
    }

    #[test]
    fn envelope_roundtrips_and_binds_metadata_to_system_text() {
        let selection = ClipboardSelection::Inline {
            spans: vec![InlineSpan {
                text: "bold".to_owned(),
                marks: vec![InlineMark::Bold],
            }],
        };
        let envelope = CditorClipboardEnvelope::new(Some(7), selection.clone(), "bold");
        let json = serde_json::to_string(&envelope).unwrap();
        assert_eq!(
            CditorClipboardEnvelope::decode_metadata(&json, "bold")
                .unwrap()
                .selection,
            selection
        );
        assert_eq!(
            CditorClipboardEnvelope::decode_metadata(&json, "plain"),
            Err(ClipboardDecodeError::InvalidSelection)
        );
    }

    #[test]
    fn envelope_rejects_unsafe_links_and_future_versions() {
        let selection = ClipboardSelection::Inline {
            spans: vec![InlineSpan {
                text: "bad".to_owned(),
                marks: vec![InlineMark::Link {
                    href: "javascript:alert(1)".to_owned(),
                }],
            }],
        };
        let envelope = CditorClipboardEnvelope::new(None, selection, "bad");
        let json = serde_json::to_string(&envelope).unwrap();
        assert_eq!(
            CditorClipboardEnvelope::decode_metadata(&json, "bad"),
            Err(ClipboardDecodeError::InvalidSelection)
        );

        let json = json.replace("\"version\":2", "\"version\":99");
        assert_eq!(
            CditorClipboardEnvelope::decode_metadata(&json, "bad"),
            Err(ClipboardDecodeError::UnsupportedVersion)
        );
    }

    #[test]
    fn envelope_rejects_oversize_malformed_unknown_schema_and_bad_checksum() {
        assert_eq!(
            CditorClipboardEnvelope::decode_metadata(
                &"x".repeat(MAX_CLIPBOARD_METADATA_BYTES + 1),
                "",
            ),
            Err(ClipboardDecodeError::TooLarge)
        );
        assert_eq!(
            CditorClipboardEnvelope::decode_metadata("{", ""),
            Err(ClipboardDecodeError::Malformed)
        );

        let selection = ClipboardSelection::Inline {
            spans: vec![InlineSpan::plain("safe")],
        };
        let mut envelope = CditorClipboardEnvelope::new(None, selection, "safe");
        envelope.schema = "application/x-attacker".to_owned();
        let json = serde_json::to_string(&envelope).unwrap();
        assert_eq!(
            CditorClipboardEnvelope::decode_metadata(&json, "safe"),
            Err(ClipboardDecodeError::UnknownSchema)
        );

        envelope.schema = CDITOR_CLIPBOARD_SCHEMA.to_owned();
        envelope.checksum ^= 1;
        let json = serde_json::to_string(&envelope).unwrap();
        assert_eq!(
            CditorClipboardEnvelope::decode_metadata(&json, "safe"),
            Err(ClipboardDecodeError::ChecksumMismatch)
        );
    }

    #[test]
    fn envelope_rejects_duplicate_forward_parent_and_inconsistent_depth() {
        let paragraph = |source_id, parent_source_id, depth| ClipboardBlock {
            source_id,
            parent_source_id,
            depth,
            kind: RichBlockKind::Paragraph,
            payload: BlockPayload::RichText {
                spans: vec![InlineSpan::plain(source_id.to_string())],
            },
        };
        let invalid_selections = [
            ClipboardSelection::Blocks {
                blocks: vec![paragraph(1, None, 0), paragraph(1, None, 0)],
            },
            ClipboardSelection::Blocks {
                blocks: vec![paragraph(1, Some(2), 1), paragraph(2, None, 0)],
            },
            ClipboardSelection::Blocks {
                blocks: vec![paragraph(1, None, 4), paragraph(2, Some(1), 7)],
            },
        ];

        for selection in invalid_selections {
            assert_eq!(
                decode_selection(selection),
                Err(ClipboardDecodeError::InvalidSelection)
            );
        }
    }

    #[test]
    fn envelope_rejects_kind_payload_mismatch_wrong_domain_and_nested_unsafe_link() {
        let mismatch = ClipboardSelection::Blocks {
            blocks: vec![ClipboardBlock {
                source_id: 1,
                parent_source_id: None,
                depth: 0,
                kind: RichBlockKind::Table,
                payload: BlockPayload::RichText {
                    spans: vec![InlineSpan::plain("not a table")],
                },
            }],
        };
        assert_eq!(
            decode_selection(mismatch),
            Err(ClipboardDecodeError::InvalidSelection)
        );

        let mut opaque = unknown_plugin_payload(2);
        let BlockPayload::Opaque { envelope, .. } = &mut opaque.payload else {
            panic!("fixture must contain opaque payload")
        };
        envelope.domain = crate::schema::SchemaDomain::Clipboard;
        let wrong_domain = ClipboardSelection::Blocks {
            blocks: vec![ClipboardBlock {
                source_id: opaque.block_id,
                parent_source_id: None,
                depth: 0,
                kind: opaque.kind,
                payload: opaque.payload,
            }],
        };
        assert_eq!(
            decode_selection(wrong_domain),
            Err(ClipboardDecodeError::InvalidSelection)
        );

        let unsafe_caption = ClipboardSelection::Blocks {
            blocks: vec![ClipboardBlock {
                source_id: 3,
                parent_source_id: None,
                depth: 0,
                kind: RichBlockKind::Image,
                payload: BlockPayload::Image(crate::rich_text::ImagePayload {
                    source: "https://example.invalid/image.png".to_owned(),
                    alt: String::new(),
                    caption: crate::rich_text::RichTextContent {
                        spans: vec![InlineSpan {
                            text: "caption".to_owned(),
                            marks: vec![InlineMark::Link {
                                href: "javascript:alert(1)".to_owned(),
                            }],
                        }],
                    },
                    display_width_ratio_milli: None,
                }),
            }],
        };
        assert_eq!(
            decode_selection(unsafe_caption),
            Err(ClipboardDecodeError::InvalidSelection)
        );
    }

    #[test]
    fn aggregate_validation_budgets_are_bounded() {
        let mut budget = ValidationBudget::default();
        assert!(budget.add_spans(MAX_CLIPBOARD_SPANS));
        assert!(!budget.add_spans(1));

        let mut budget = ValidationBudget::default();
        assert!(budget.add_cells(MAX_CLIPBOARD_CELLS));
        assert!(!budget.add_cells(1));
    }

    #[test]
    fn envelope_roundtrips_fragments_blocks_and_tables() {
        let fragments = ClipboardSelection::TextFragments {
            fragments: vec![
                ClipboardBlockFragment {
                    source_id: 1,
                    parent_source_id: None,
                    depth: 0,
                    kind: RichBlockKind::Paragraph,
                    spans: vec![InlineSpan::plain("first")],
                    boundary: ClipboardFragmentBoundary::StartPartial,
                    starts_at_block_start: false,
                    ends_at_block_end: true,
                },
                ClipboardBlockFragment {
                    source_id: 2,
                    parent_source_id: None,
                    depth: 0,
                    kind: RichBlockKind::Quote,
                    spans: vec![InlineSpan::plain("last")],
                    boundary: ClipboardFragmentBoundary::EndPartial,
                    starts_at_block_start: true,
                    ends_at_block_end: false,
                },
            ],
        };
        let blocks = ClipboardSelection::Blocks {
            blocks: vec![ClipboardBlock {
                source_id: 4,
                parent_source_id: None,
                depth: 0,
                kind: RichBlockKind::Whiteboard,
                payload: BlockPayload::Whiteboard(WhiteboardPayload {
                    scene_json: r#"{"elements":[]}"#.to_owned(),
                }),
            }],
        };
        let mut table = TablePayload {
            rows: vec![TableRowPayload {
                cells: vec![TableCellPayload::plain("cell")],
                height: Default::default(),
            }],
            ..Default::default()
        };
        table.normalize();
        let table = ClipboardSelection::Table { table };

        for selection in [fragments, blocks, table] {
            let system_text = selection.plain_text();
            let envelope = CditorClipboardEnvelope::new(Some(3), selection.clone(), &system_text);
            let json = serde_json::to_string(&envelope).unwrap();
            assert_eq!(
                CditorClipboardEnvelope::decode_metadata(&json, &system_text)
                    .unwrap()
                    .selection,
                selection
            );
        }
    }

    #[test]
    fn native_clipboard_preserves_unknown_plugin_envelope_bytes() {
        let record = unknown_plugin_payload(41);
        let selection = ClipboardSelection::Blocks {
            blocks: vec![ClipboardBlock {
                source_id: record.block_id,
                parent_source_id: None,
                depth: 0,
                kind: record.kind.clone(),
                payload: record.payload.clone(),
            }],
        };
        let system_text = selection.plain_text();
        assert_eq!(system_text, UNKNOWN_PLUGIN_FALLBACK);
        let envelope = CditorClipboardEnvelope::new(Some(7), selection, &system_text);

        let first_json = serde_json::to_string(&envelope).unwrap();
        let decoded = CditorClipboardEnvelope::decode_metadata(&first_json, &system_text).unwrap();
        let ClipboardSelection::Blocks { blocks } = decoded.selection else {
            panic!("expected block clipboard")
        };
        let block = &blocks[0];
        let record = crate::rich_text::BlockPayloadRecord {
            block_id: block.source_id,
            content_version: 7,
            kind: block.kind.clone(),
            payload: block.payload.clone(),
        };
        assert_unknown_plugin_bytes(&record);
        assert_eq!(
            block.kind,
            RichBlockKind::Custom(UNKNOWN_PLUGIN_KIND.to_owned())
        );
        let BlockPayload::Opaque {
            envelope,
            plain_text_fallback,
        } = &block.payload
        else {
            panic!("expected opaque payload")
        };
        assert_eq!(envelope.body_bytes(), UNKNOWN_PLUGIN_BODY);
        assert_eq!(plain_text_fallback, UNKNOWN_PLUGIN_FALLBACK);
    }
}
