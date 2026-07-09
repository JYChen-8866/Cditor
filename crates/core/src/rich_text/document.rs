use crate::document::BlockIndexRecord;
use crate::ids::{BlockId, DocumentId};
use crate::layout::BlockLayoutMeta;
use crate::version::StructureVersion;

use super::{
    BlockAttrs, BlockPayload, BlockPayloadRecord, CalloutVariant, FilePayload, ImagePayload,
    InlineSpan, RichBlockKind, TablePayload, WhiteboardPayload, kind_tag_for_rich_block_kind,
};

pub const CURRENT_RICH_TEXT_FORMAT_VERSION: RichTextFormatVersion = 1;

pub type RichTextFormatVersion = u32;
pub type SortKey = String;

#[derive(Debug, Clone, PartialEq)]
pub struct RichTextDocument {
    pub id: DocumentId,
    pub version: RichTextFormatVersion,
    pub metadata: DocumentMetadata,
    pub root_blocks: Vec<BlockId>,
    pub blocks: Vec<RichBlockRecord>,
    pub structure_version: StructureVersion,
}

impl RichTextDocument {
    pub fn empty(id: DocumentId) -> Self {
        Self {
            id,
            version: CURRENT_RICH_TEXT_FORMAT_VERSION,
            metadata: DocumentMetadata::default(),
            root_blocks: Vec::new(),
            blocks: Vec::new(),
            structure_version: 1,
        }
    }

    pub fn push_root_block(&mut self, mut block: RichBlockRecord) -> BlockId {
        block.document_id = self.id;
        block.parent_id = None;
        block.depth = 0;
        block.structure_version = self.structure_version;
        let id = block.id;
        if let Some(previous) = self.root_blocks.last().copied() {
            block.prev_id = Some(previous);
            if let Some(previous_block) =
                self.blocks.iter_mut().find(|record| record.id == previous)
            {
                previous_block.next_id = Some(id);
            }
        }
        self.root_blocks.push(id);
        self.blocks.push(block);
        id
    }

    pub fn index_records(&self) -> Vec<BlockIndexRecord> {
        self.blocks
            .iter()
            .map(RichBlockRecord::to_index_record)
            .collect()
    }

    pub fn payload_records(&self) -> Vec<BlockPayloadRecord> {
        self.blocks
            .iter()
            .map(RichBlockRecord::to_payload_record)
            .collect()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DocumentMetadata {
    pub title: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub tags: Vec<String>,
    pub cover: Option<PageCover>,
    pub icon: Option<PageIcon>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageCover {
    External {
        url: String,
        position_y: CoverPositionY,
    },
    Asset {
        asset: AssetRef,
        position_y: CoverPositionY,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoverPositionY(u16);

impl CoverPositionY {
    pub const CENTER: Self = Self(500);

    pub fn from_ratio(value: f32) -> Self {
        Self((value.clamp(0.0, 1.0) * 1000.0).round() as u16)
    }

    pub fn ratio(self) -> f32 {
        self.0 as f32 / 1000.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageIcon {
    Emoji { emoji: String },
    Asset { asset: AssetRef },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetRef {
    pub source: String,
    pub media_type: Option<String>,
    pub name: Option<String>,
    pub size_bytes: Option<u64>,
}

impl AssetRef {
    pub fn local(path: impl Into<String>) -> Self {
        Self {
            source: path.into(),
            media_type: None,
            name: None,
            size_bytes: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RichBlockRecord {
    pub id: BlockId,
    pub document_id: DocumentId,
    pub parent_id: Option<BlockId>,
    pub prev_id: Option<BlockId>,
    pub next_id: Option<BlockId>,
    pub sort_key: SortKey,
    pub depth: u16,
    pub kind: RichBlockKind,
    pub attrs: BlockAttrs,
    pub payload: BlockPayload,
    pub children: Vec<BlockId>,
    pub content_version: u64,
    pub structure_version: StructureVersion,
    pub measured_height: Option<f64>,
    pub estimated_height: f64,
    pub raw_fallback: Option<String>,
}

impl RichBlockRecord {
    pub const DEFAULT_TEXT_HEIGHT: f64 = 32.0;
    pub const DEFAULT_IMAGE_HEIGHT: f64 = 220.0;
    pub const DEFAULT_CODE_HEIGHT: f64 = 136.0;
    pub const DEFAULT_TABLE_HEIGHT: f64 = 120.0;

    pub fn new(id: BlockId, kind: RichBlockKind, payload: BlockPayload) -> Self {
        let estimated_height = default_estimated_height(&kind, &payload);
        Self {
            id,
            document_id: 0,
            parent_id: None,
            prev_id: None,
            next_id: None,
            sort_key: id.to_string(),
            depth: 0,
            kind,
            attrs: BlockAttrs::default(),
            payload,
            children: Vec::new(),
            content_version: 1,
            structure_version: 1,
            measured_height: None,
            estimated_height,
            raw_fallback: None,
        }
    }

    pub fn paragraph(id: BlockId, text: impl Into<String>) -> Self {
        Self::rich_text(id, RichBlockKind::Paragraph, text)
    }

    pub fn heading(id: BlockId, level: u8, text: impl Into<String>) -> Self {
        Self::rich_text(
            id,
            RichBlockKind::Heading {
                level: level.clamp(1, 6),
            },
            text,
        )
    }

    pub fn quote(id: BlockId, text: impl Into<String>) -> Self {
        Self::rich_text(id, RichBlockKind::Quote, text)
    }

    pub fn callout(id: BlockId, variant: CalloutVariant, text: impl Into<String>) -> Self {
        Self::rich_text(id, RichBlockKind::Callout { variant }, text)
    }

    pub fn todo(id: BlockId, checked: bool, text: impl Into<String>) -> Self {
        Self::rich_text(id, RichBlockKind::Todo { checked }, text)
    }

    pub fn bulleted_list(id: BlockId, text: impl Into<String>) -> Self {
        Self::rich_text(id, RichBlockKind::BulletedList, text)
    }

    pub fn numbered_list(id: BlockId, text: impl Into<String>) -> Self {
        Self::rich_text(id, RichBlockKind::NumberedList, text)
    }

    pub fn code_block(id: BlockId, language: Option<String>, text: impl Into<String>) -> Self {
        Self::new(
            id,
            RichBlockKind::Code {
                language: language.clone(),
            },
            BlockPayload::Code {
                language,
                text: text.into(),
            },
        )
    }

    pub fn table(id: BlockId, table: TablePayload) -> Self {
        Self::new(id, RichBlockKind::Table, BlockPayload::Table(table))
    }

    pub fn image(
        id: BlockId,
        source: impl Into<String>,
        alt: impl Into<String>,
        caption: impl Into<String>,
    ) -> Self {
        Self::new(
            id,
            RichBlockKind::Image,
            BlockPayload::Image(ImagePayload {
                source: source.into(),
                alt: alt.into(),
                caption: caption.into(),
                display_width_ratio_milli: None,
            }),
        )
    }

    pub fn file(
        id: BlockId,
        source: impl Into<String>,
        name: impl Into<String>,
        size_bytes: Option<u64>,
    ) -> Self {
        Self::asset_file(id, RichBlockKind::File, source, name, size_bytes)
    }

    pub fn attachment(
        id: BlockId,
        source: impl Into<String>,
        name: impl Into<String>,
        size_bytes: Option<u64>,
    ) -> Self {
        Self::asset_file(id, RichBlockKind::Attachment, source, name, size_bytes)
    }

    fn asset_file(
        id: BlockId,
        kind: RichBlockKind,
        source: impl Into<String>,
        name: impl Into<String>,
        size_bytes: Option<u64>,
    ) -> Self {
        Self::new(
            id,
            kind,
            BlockPayload::File(FilePayload {
                source: source.into(),
                name: name.into(),
                size_bytes,
            }),
        )
    }

    pub fn whiteboard(id: BlockId, scene_json: impl Into<String>) -> Self {
        Self::new(
            id,
            RichBlockKind::Whiteboard,
            BlockPayload::Whiteboard(WhiteboardPayload {
                scene_json: scene_json.into(),
            }),
        )
    }

    pub fn divider(id: BlockId) -> Self {
        Self::new(id, RichBlockKind::Divider, BlockPayload::Empty)
    }

    pub fn separator(id: BlockId) -> Self {
        Self::new(id, RichBlockKind::Separator, BlockPayload::Empty)
    }

    pub fn footnote_definition(id: BlockId, text: impl Into<String>) -> Self {
        Self::rich_text(id, RichBlockKind::FootnoteDefinition, text)
    }

    pub fn comment(id: BlockId, text: impl Into<String>) -> Self {
        Self::rich_text(id, RichBlockKind::Comment, text)
    }

    pub fn raw_markdown(id: BlockId, raw: impl Into<String>) -> Self {
        let raw = raw.into();
        let mut block = Self::rich_text(id, RichBlockKind::RawMarkdown, raw.clone());
        block.raw_fallback = Some(raw);
        block
    }

    pub fn rich_text(id: BlockId, kind: RichBlockKind, text: impl Into<String>) -> Self {
        Self::new(
            id,
            kind,
            BlockPayload::RichText {
                spans: vec![InlineSpan::plain(text)],
            },
        )
    }

    pub fn with_parent(mut self, parent_id: BlockId, depth: u16) -> Self {
        self.parent_id = Some(parent_id);
        self.depth = depth;
        self
    }

    pub fn with_attrs(mut self, attrs: BlockAttrs) -> Self {
        self.attrs = attrs;
        self
    }

    pub fn with_measured_height(mut self, height: f64) -> Self {
        self.measured_height = Some(height);
        self
    }

    pub fn to_index_record(&self) -> BlockIndexRecord {
        BlockIndexRecord::new(
            self.id,
            self.parent_id,
            self.depth,
            kind_tag_for_rich_block_kind(&self.kind),
            flags_for_block(self),
        )
        .with_layout_meta(BlockLayoutMeta {
            block_id: self.id,
            estimated_height: self.estimated_height,
            measured_height: self.measured_height,
            width_bucket: 0,
            layout_version: 0,
            dirty: self.measured_height.is_none(),
        })
    }

    pub fn to_payload_record(&self) -> BlockPayloadRecord {
        BlockPayloadRecord {
            block_id: self.id,
            content_version: self.content_version,
            kind: self.kind.clone(),
            payload: self.payload.clone(),
        }
    }
}

fn default_estimated_height(kind: &RichBlockKind, payload: &BlockPayload) -> f64 {
    crate::layout::estimate_block_height(kind, payload, crate::layout::DEFAULT_LAYOUT_WIDTH_PX)
        .height
}

fn flags_for_block(block: &RichBlockRecord) -> u32 {
    let mut flags = 0;
    if block.attrs.folded {
        flags |= 1 << 0;
    }
    if block.attrs.locked {
        flags |= 1 << 1;
    }
    if !block.children.is_empty() {
        flags |= 1 << 2;
    }
    flags
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rich_text::{TableCellPayload, TableRowPayload};

    #[test]
    fn rich_text_document_splits_structure_index_from_payloads() {
        let mut document = RichTextDocument::empty(7);
        document.metadata.title = Some("V2".to_owned());
        document.push_root_block(RichBlockRecord::heading(1, 1, "Title"));
        document.push_root_block(RichBlockRecord::code_block(
            2,
            Some("rust".to_owned()),
            "fn main() {}",
        ));

        let index_records = document.index_records();
        let payloads = document.payload_records();

        assert_eq!(index_records.len(), 2);
        assert_eq!(
            index_records[0].kind_tag,
            kind_tag_for_rich_block_kind(&RichBlockKind::Heading { level: 1 })
        );
        assert_eq!(payloads[1].plain_text(), "fn main() {}");
        assert!(matches!(payloads[1].kind, RichBlockKind::Code { .. }));
    }

    #[test]
    fn typed_block_constructors_cover_first_version_block_shapes() {
        let table = TablePayload {
            rows: vec![TableRowPayload {
                cells: vec![TableCellPayload::plain("cell")],
                height: Default::default(),
            }],
            columns: Vec::new(),
            header_rows: 1,
            header_cols: 0,
            header_style: Default::default(),
        };
        let blocks = vec![
            RichBlockRecord::paragraph(1, "p"),
            RichBlockRecord::heading(2, 2, "h2"),
            RichBlockRecord::quote(3, "q"),
            RichBlockRecord::todo(4, true, "todo"),
            RichBlockRecord::code_block(5, None, "code"),
            RichBlockRecord::table(6, table),
            RichBlockRecord::image(7, "a.png", "alt", "caption"),
            RichBlockRecord::file(8, "a.zip", "a.zip", Some(12)),
            RichBlockRecord::attachment(9, "a.pdf", "a.pdf", Some(34)),
            RichBlockRecord::whiteboard(10, "{}"),
            RichBlockRecord::divider(11),
            RichBlockRecord::separator(12),
            RichBlockRecord::footnote_definition(13, "footnote"),
            RichBlockRecord::comment(14, "comment"),
            RichBlockRecord::raw_markdown(15, "**raw**"),
        ];

        assert_eq!(blocks.len(), 15);
        assert!(matches!(
            blocks[1].kind,
            RichBlockKind::Heading { level: 2 }
        ));
        assert!(matches!(blocks[4].payload, BlockPayload::Code { .. }));
        assert!(matches!(blocks[6].payload, BlockPayload::Image(_)));
    }
}
