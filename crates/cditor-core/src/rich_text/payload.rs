use crate::ids::BlockId;
use crate::layout::{
    ColumnSpec, ColumnsLayoutError, ColumnsLayoutModel, DEFAULT_COLUMN_GAP_PX, StableBox,
};
use crate::schema::{SchemaDomain, VersionedEnvelope};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::{InlineSpan, RichBlockKind, TablePayload, plain_text_from_spans};

/// Persisted rich text for sub-block surfaces such as image captions and
/// collection titles. Older documents encoded these values as a JSON string;
/// deserialization accepts that representation while preserving rich spans for
/// newer documents.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RichTextContent {
    pub spans: Vec<InlineSpan>,
}

impl RichTextContent {
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            spans: vec![InlineSpan::plain(text)],
        }
    }

    pub fn plain_text(&self) -> String {
        plain_text_from_spans(&self.spans)
    }

    pub fn is_empty(&self) -> bool {
        self.spans.iter().all(|span| span.text.is_empty())
    }
}

impl From<String> for RichTextContent {
    fn from(value: String) -> Self {
        Self::plain(value)
    }
}

impl From<&str> for RichTextContent {
    fn from(value: &str) -> Self {
        Self::plain(value)
    }
}

impl Serialize for RichTextContent {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if self.spans.iter().all(|span| span.marks.is_empty()) {
            return self.plain_text().serialize(serializer);
        }
        RichTextContentRepr::Rich {
            spans: self.spans.clone(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for RichTextContent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match RichTextContentRepr::deserialize(deserializer)? {
            RichTextContentRepr::Plain(text) => Ok(Self::plain(text)),
            RichTextContentRepr::Rich { spans } => Ok(Self { spans }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
enum RichTextContentRepr {
    Plain(String),
    Rich { spans: Vec<InlineSpan> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockPayloadRecord {
    pub block_id: BlockId,
    pub content_version: u64,
    pub kind: RichBlockKind,
    pub payload: BlockPayload,
}

impl BlockPayloadRecord {
    pub fn rich_text(block_id: BlockId, kind: RichBlockKind, text: impl Into<String>) -> Self {
        Self {
            block_id,
            content_version: 1,
            kind,
            payload: BlockPayload::RichText {
                spans: vec![InlineSpan::plain(text)],
            },
        }
    }

    pub fn plain_text(&self) -> String {
        self.payload.plain_text()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BlockPayload {
    RichText {
        spans: Vec<InlineSpan>,
    },
    Code {
        language: Option<String>,
        text: String,
    },
    Table(TablePayload),
    Columns(ColumnsGroupPayload),
    Image(ImagePayload),
    Collection(CollectionPayload),
    File(FilePayload),
    Whiteboard(WhiteboardPayload),
    Embed(EmbedPayload),
    Html {
        html: String,
        sanitized: bool,
    },
    /// 当前 build 不理解的 block/plugin payload。
    ///
    /// `envelope.body` 使用 `RawValue`，存储、clipboard、move 与 native export
    /// 必须原样透传；Runtime 只能显示安全占位，不能解释或重写内容。
    Opaque {
        envelope: VersionedEnvelope,
        plain_text_fallback: String,
    },
    Empty,
}

impl BlockPayload {
    pub const INVALID_OPAQUE_DOMAIN: &'static str =
        "opaque block payload envelope must use BlockPayload domain";

    pub fn plain_text(&self) -> String {
        match self {
            Self::RichText { spans } => plain_text_from_spans(spans),
            Self::Code { text, .. } => text.clone(),
            Self::Table(table) => table.plain_text(),
            Self::Columns(_) => String::new(),
            Self::Image(image) => {
                [image.alt.as_str(), image.caption.plain_text().as_str()].join(" ")
            }
            Self::Collection(collection) => collection.title.plain_text(),
            Self::File(file) => file.name.clone(),
            Self::Whiteboard(_) => "whiteboard".to_owned(),
            Self::Embed(embed) => [embed.title.as_str(), embed.url.as_str()].join(" "),
            Self::Html { html, .. } => html.clone(),
            Self::Opaque {
                plain_text_fallback,
                ..
            } => plain_text_fallback.clone(),
            Self::Empty => String::new(),
        }
    }

    pub fn opaque(
        envelope: VersionedEnvelope,
        plain_text_fallback: impl Into<String>,
    ) -> Result<Self, &'static str> {
        let payload = Self::Opaque {
            envelope,
            plain_text_fallback: plain_text_fallback.into(),
        };
        payload.validate_opaque_envelope_domain()?;
        Ok(payload)
    }

    /// Validates the invariant that serde cannot enforce while preserving an
    /// unknown envelope's raw JSON bytes.
    pub fn validate_opaque_envelope_domain(&self) -> Result<(), &'static str> {
        if matches!(
            self,
            Self::Opaque { envelope, .. } if envelope.domain != SchemaDomain::BlockPayload
        ) {
            return Err(Self::INVALID_OPAQUE_DOMAIN);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColumnsGroupPayload {
    pub columns: Vec<ColumnSpec>,
    #[serde(default = "default_columns_gap_milli")]
    pub gap_milli: u16,
}

impl ColumnsGroupPayload {
    pub fn equal(group_id: BlockId, column_ids: Vec<BlockId>) -> Result<Self, ColumnsLayoutError> {
        let model = ColumnsLayoutModel::equal(group_id, column_ids)?;
        Ok(Self {
            columns: model.columns().to_vec(),
            gap_milli: (DEFAULT_COLUMN_GAP_PX * 1000.0) as u16,
        })
    }

    pub fn layout_model(
        &self,
        group_id: BlockId,
    ) -> Result<ColumnsLayoutModel, ColumnsLayoutError> {
        ColumnsLayoutModel::from_weights(group_id, self.columns.clone())
    }

    pub fn gap_px(&self) -> f64 {
        f64::from(self.gap_milli) / 1000.0
    }
}

fn default_columns_gap_milli() -> u16 {
    24_000
}

#[derive(Debug, Clone, PartialEq)]
pub enum BlockPayloadView {
    Loaded(BlockPayloadRecord),
    Placeholder { estimated_height: f64 },
    Loading { stable_box: StableBox },
    Error { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ImagePayload {
    pub source: String,
    pub alt: String,
    #[serde(default)]
    pub caption: RichTextContent,
    /// User-selected display width as a fraction of the editor image max width.
    /// 1000 means full image column width; None uses the Notion-like default.
    pub display_width_ratio_milli: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionPayload {
    /// A stable collection identity. It is intentionally separate from the
    /// hosting block id so a collection can later be embedded or moved.
    pub collection_id: u64,
    #[serde(default)]
    pub title: RichTextContent,
    /// Schema changes are versioned independently from text edits.
    #[serde(default = "default_collection_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub properties: Vec<CollectionPropertyPayload>,
    #[serde(default)]
    pub views: Vec<CollectionViewPayload>,
    /// The selected view is persisted by stable id rather than by its current
    /// vector position so view reorder does not change editor state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_view_id: Option<u64>,
}

impl Default for CollectionPayload {
    fn default() -> Self {
        Self {
            collection_id: 0,
            title: RichTextContent::default(),
            schema_version: default_collection_schema_version(),
            properties: Vec::new(),
            views: Vec::new(),
            active_view_id: None,
        }
    }
}

impl CollectionPayload {
    pub fn for_block(block_id: BlockId, title: impl Into<String>) -> Self {
        Self {
            collection_id: stable_collection_child_id(block_id, 0),
            title: RichTextContent::plain(title),
            schema_version: default_collection_schema_version(),
            properties: vec![CollectionPropertyPayload {
                property_id: stable_collection_child_id(block_id, 1),
                name: "名称".to_owned(),
                kind: CollectionPropertyKind::Title,
                required: true,
            }],
            views: vec![CollectionViewPayload {
                view_id: stable_collection_child_id(block_id, 2),
                name: "表格".to_owned(),
                layout: CollectionViewLayout::Table,
            }],
            active_view_id: Some(stable_collection_child_id(block_id, 2)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionPropertyPayload {
    pub property_id: u64,
    pub name: String,
    pub kind: CollectionPropertyKind,
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CollectionPropertyKind {
    Title,
    Text,
    Number,
    Checkbox,
    Select,
    MultiSelect,
    Date,
    Url,
    Email,
    Phone,
    CreatedTime,
    UpdatedTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionViewPayload {
    pub view_id: u64,
    pub name: String,
    pub layout: CollectionViewLayout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CollectionViewLayout {
    Table,
    List,
    Board,
    Gallery,
    Calendar,
    Timeline,
}

fn default_collection_schema_version() -> u32 {
    1
}

fn stable_collection_child_id(block_id: BlockId, discriminator: u64) -> u64 {
    block_id.rotate_left(17) ^ 0x9e37_79b9_7f4a_7c15u64.wrapping_add(discriminator)
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct FilePayload {
    pub name: String,
    pub source: String,
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct WhiteboardPayload {
    pub scene_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct EmbedPayload {
    pub url: String,
    pub title: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rich_text::InlineMark;

    #[test]
    fn rich_text_content_reads_legacy_string_and_keeps_plain_json_compatible() {
        let decoded: RichTextContent = serde_json::from_str("\"legacy caption\"").unwrap();
        assert_eq!(decoded.plain_text(), "legacy caption");
        assert_eq!(
            serde_json::to_string(&decoded).unwrap(),
            "\"legacy caption\""
        );
    }

    #[test]
    fn rich_text_content_round_trips_marked_spans_without_flattening() {
        let content = RichTextContent {
            spans: vec![InlineSpan {
                text: "caption".to_owned(),
                marks: vec![InlineMark::Bold, InlineMark::Italic],
            }],
        };

        let encoded = serde_json::to_string(&content).unwrap();
        let decoded: RichTextContent = serde_json::from_str(&encoded).unwrap();

        assert!(encoded.contains("spans"));
        assert_eq!(decoded, content);
    }

    #[test]
    fn collection_constructor_assigns_stable_schema_and_view_ids() {
        let first = CollectionPayload::for_block(42, "Projects");
        let second = CollectionPayload::for_block(42, "Renamed");

        assert_eq!(first.collection_id, second.collection_id);
        assert_eq!(
            first.properties[0].property_id,
            second.properties[0].property_id
        );
        assert_eq!(first.views[0].view_id, second.views[0].view_id);
        assert_ne!(first.collection_id, first.properties[0].property_id);
        assert_eq!(first.properties[0].kind, CollectionPropertyKind::Title);
        assert_eq!(first.views[0].layout, CollectionViewLayout::Table);
    }

    #[test]
    fn columns_payload_round_trips_stable_ids_weights_and_gap() {
        let payload = BlockPayload::Columns(ColumnsGroupPayload::equal(10, vec![11, 12]).unwrap());
        let encoded = serde_json::to_string(&payload).unwrap();
        let decoded: BlockPayload = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded, payload);
        let BlockPayload::Columns(columns) = decoded else {
            panic!("expected columns payload")
        };
        assert_eq!(columns.gap_px(), DEFAULT_COLUMN_GAP_PX);
        assert_eq!(columns.layout_model(10).unwrap().columns().len(), 2);
    }

    #[test]
    fn opaque_payload_rejects_a_non_block_domain() {
        let envelope = VersionedEnvelope::encode(SchemaDomain::Clipboard, &"opaque").unwrap();
        assert_eq!(
            BlockPayload::opaque(envelope, "fallback").unwrap_err(),
            BlockPayload::INVALID_OPAQUE_DOMAIN
        );
    }

    #[test]
    fn opaque_payload_validation_rejects_a_wrong_domain_after_deserialization() {
        let envelope = VersionedEnvelope::encode(SchemaDomain::Clipboard, &"opaque").unwrap();
        let invalid = BlockPayload::Opaque {
            envelope,
            plain_text_fallback: "fallback".to_owned(),
        };
        let encoded = serde_json::to_string(&invalid).unwrap();
        let decoded: BlockPayload = serde_json::from_str(&encoded).unwrap();

        assert_eq!(
            decoded.validate_opaque_envelope_domain().unwrap_err(),
            BlockPayload::INVALID_OPAQUE_DOMAIN
        );
    }
}
