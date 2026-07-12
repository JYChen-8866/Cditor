use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::ids::{BlockId, DocumentId};

use super::{
    BlockPayload, InlineMark, InlineSpan, RichBlockKind, TablePayload, plain_text_from_spans,
};

pub const CDITOR_CLIPBOARD_SCHEMA: &str = "application/x-cditor-clipboard";
pub const CDITOR_CLIPBOARD_VERSION: u16 = 2;
pub const MAX_CLIPBOARD_METADATA_BYTES: usize = 8 * 1024 * 1024;
const MAX_CLIPBOARD_BLOCKS: usize = 100_000;
const MAX_CLIPBOARD_SPANS: usize = 1_000_000;

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
                fragments.len() >= 2
                    && fragments.len() <= MAX_CLIPBOARD_BLOCKS
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
                    && blocks.iter().all(valid_block)
            }
            Self::Table { table } => {
                table.row_count() <= MAX_CLIPBOARD_BLOCKS
                    && table.column_count() <= MAX_CLIPBOARD_BLOCKS
                    && table
                        .rows
                        .iter()
                        .all(|row| row.cells.iter().all(|cell| valid_spans(&cell.spans)))
            }
        }
    }
}

fn valid_fragment_structure(fragments: &[ClipboardBlockFragment]) -> bool {
    let mut seen = HashSet::with_capacity(fragments.len());
    for fragment in fragments {
        if !seen.insert(fragment.source_id)
            || fragment
                .parent_source_id
                .is_some_and(|parent| !seen.contains(&parent))
        {
            return false;
        }
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

fn valid_block(block: &ClipboardBlock) -> bool {
    if !kind_matches_payload(&block.kind, &block.payload) {
        return false;
    }
    match &block.payload {
        BlockPayload::RichText { spans } => valid_spans(spans),
        BlockPayload::Table(table) => table
            .rows
            .iter()
            .all(|row| row.cells.iter().all(|cell| valid_spans(&cell.spans))),
        BlockPayload::Image(image) => safe_resource(&image.source),
        BlockPayload::File(file) => safe_resource(&file.source),
        BlockPayload::Embed(embed) => safe_resource(&embed.url),
        _ => true,
    }
}

fn kind_matches_payload(kind: &RichBlockKind, payload: &BlockPayload) -> bool {
    match kind {
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
            matches!(payload, BlockPayload::Table(_) | BlockPayload::Empty)
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
    use crate::rich_text::{TableCellPayload, TableRowPayload, WhiteboardPayload};

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
}
