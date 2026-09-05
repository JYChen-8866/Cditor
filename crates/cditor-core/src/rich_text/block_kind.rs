use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::unknown::{UnknownWire, raw_json};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(remote = "Self")]
pub enum RichBlockKind {
    DocumentTitle,
    Paragraph,
    Heading {
        level: u8,
    },
    Quote,
    Callout {
        variant: CalloutVariant,
    },
    Todo {
        checked: bool,
    },
    BulletedList,
    NumberedList,
    Toggle,
    Code {
        language: Option<String>,
    },
    Math,
    Mermaid,
    Html,
    Table,
    ColumnsGroup,
    Column,
    Image,
    Video,
    File,
    Attachment,
    Whiteboard,
    MindMap,
    Embed,
    Divider,
    Separator,
    FootnoteDefinition,
    Comment,
    RawMarkdown,
    Database,
    Custom(String),
    /// 本 build 不认识的块类型：新版本新增的内建 kind（例如旧版本读到
    /// `Video`），原始 JSON 字节原样保留（见 [`UnknownWire`]）。行为等同于没有
    /// provider 的 [`RichBlockKind::Custom`]：只读占位、不接受文本输入。
    ///
    /// 只由 `Deserialize` 产生，序列化写回原始字节而非 `{"Unknown": …}`。
    #[serde(skip)]
    Unknown(UnknownWire),
}

/// `Unknown` 走原始字节，其余变体交给 derive（`remote = "Self"` 生成的固有
/// 方法）。
impl Serialize for RichBlockKind {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Unknown(unknown) => unknown.serialize(serializer),
            known => Self::serialize(known, serializer),
        }
    }
}

/// 语法合法但本 build 不认识的 kind 退化为 [`RichBlockKind::Unknown`]，而不是
/// 让整篇文档加载失败。
impl<'de> Deserialize<'de> for RichBlockKind {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = raw_json(deserializer)?;
        // 固有方法优先于 trait 方法：调用 derive 生成的解码器，不会递归。
        let mut known = serde_json::Deserializer::from_str(raw.get());
        match Self::deserialize(&mut known) {
            Ok(kind) => Ok(kind),
            Err(error) => Ok(Self::Unknown(UnknownWire::new(raw, error.to_string()))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CalloutVariant {
    Note,
    Tip,
    Important,
    Warning,
    Caution,
    Info,
    Success,
    Danger,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutBehavior {
    TextLike,
    FixedHeight,
    StableBox,
    InternalVirtualized,
    CustomProvider,
}

impl RichBlockKind {
    pub const fn is_document_title(&self) -> bool {
        matches!(self, Self::DocumentTitle)
    }

    pub fn layout_behavior(&self) -> LayoutBehavior {
        match self {
            Self::DocumentTitle
            | Self::Paragraph
            | Self::Heading { .. }
            | Self::Quote
            | Self::Callout { .. }
            | Self::Todo { .. }
            | Self::BulletedList
            | Self::NumberedList
            | Self::Toggle
            | Self::FootnoteDefinition
            | Self::Comment => LayoutBehavior::TextLike,
            Self::Code { .. } | Self::Table | Self::Database => LayoutBehavior::InternalVirtualized,
            Self::Image
            | Self::Video
            | Self::File
            | Self::Attachment
            | Self::Whiteboard
            | Self::MindMap
            | Self::Embed => LayoutBehavior::StableBox,
            Self::Divider | Self::Separator => LayoutBehavior::FixedHeight,
            Self::Math
            | Self::Mermaid
            | Self::Html
            | Self::RawMarkdown
            | Self::ColumnsGroup
            | Self::Column
            | Self::Custom(_)
            | Self::Unknown(_) => LayoutBehavior::CustomProvider,
        }
    }

    pub fn supports_children(&self) -> bool {
        matches!(
            self,
            Self::Quote
                | Self::Callout { .. }
                | Self::Todo { .. }
                | Self::BulletedList
                | Self::NumberedList
                | Self::Toggle
                | Self::ColumnsGroup
                | Self::Column
        )
    }

    /// 本 kind 按原样透传的原始 JSON 字节（只有 [`Self::Unknown`] 有）。
    /// 与 [`super::BlockPayload::verbatim_json`] 对应。
    pub fn verbatim_json(&self) -> Option<&str> {
        match self {
            Self::Unknown(unknown) => Some(unknown.json()),
            _ => None,
        }
    }

    pub fn supports_rich_text_title(&self) -> bool {
        !matches!(
            self,
            Self::Divider
                | Self::Separator
                | Self::Image
                | Self::Video
                | Self::File
                | Self::Attachment
                | Self::Whiteboard
                | Self::MindMap
                | Self::ColumnsGroup
                | Self::Column
                | Self::Unknown(_)
        )
    }
}

pub fn kind_tag_for_rich_block_kind(kind: &RichBlockKind) -> u16 {
    match kind {
        RichBlockKind::DocumentTitle => 34,
        RichBlockKind::Paragraph => 1,
        RichBlockKind::Heading { level: 1 } => 2,
        RichBlockKind::Heading { level: 2 } => 26,
        RichBlockKind::Heading { level: 3 } => 27,
        RichBlockKind::Heading { level: 4 } => 28,
        RichBlockKind::Heading { level: 5 } => 29,
        RichBlockKind::Heading { .. } => 30,
        RichBlockKind::Quote => 3,
        RichBlockKind::Callout { .. } => 4,
        RichBlockKind::Todo { .. } => 5,
        RichBlockKind::BulletedList => 6,
        RichBlockKind::NumberedList => 7,
        RichBlockKind::Toggle => 8,
        RichBlockKind::Code { .. } => 9,
        RichBlockKind::Math => 10,
        RichBlockKind::Mermaid => 11,
        RichBlockKind::Html => 12,
        RichBlockKind::Table => 13,
        RichBlockKind::Image => 14,
        RichBlockKind::File => 15,
        RichBlockKind::Whiteboard => 16,
        RichBlockKind::MindMap => 17,
        RichBlockKind::Embed => 18,
        RichBlockKind::Divider => 19,
        RichBlockKind::Database => 20,
        RichBlockKind::Attachment => 21,
        RichBlockKind::Separator => 22,
        RichBlockKind::FootnoteDefinition => 23,
        RichBlockKind::Comment => 24,
        RichBlockKind::RawMarkdown => 25,
        RichBlockKind::ColumnsGroup => 31,
        RichBlockKind::Column => 32,
        RichBlockKind::Video => 33,
        // 读不懂的块类型与自定义块共用 registry 的 unknown descriptor
        // （`lossless_unknown`）。
        RichBlockKind::Custom(_) | RichBlockKind::Unknown(_) => u16::MAX,
    }
}

/// 未登记 tag 的兜底块类型名，与 registry 的 unknown descriptor 一致。
pub const UNKNOWN_BLOCK_KIND_NAME: &str = "unknown";

pub fn rich_block_kind_from_tag(tag: u16) -> RichBlockKind {
    match tag {
        34 => RichBlockKind::DocumentTitle,
        1 => RichBlockKind::Paragraph,
        2 => RichBlockKind::Heading { level: 1 },
        26 => RichBlockKind::Heading { level: 2 },
        27 => RichBlockKind::Heading { level: 3 },
        28 => RichBlockKind::Heading { level: 4 },
        29 => RichBlockKind::Heading { level: 5 },
        30 => RichBlockKind::Heading { level: 6 },
        3 => RichBlockKind::Quote,
        4 => RichBlockKind::Callout {
            variant: CalloutVariant::Note,
        },
        5 => RichBlockKind::Todo { checked: false },
        6 => RichBlockKind::BulletedList,
        7 => RichBlockKind::NumberedList,
        8 => RichBlockKind::Toggle,
        9 => RichBlockKind::Code { language: None },
        10 => RichBlockKind::Math,
        11 => RichBlockKind::Mermaid,
        12 => RichBlockKind::Html,
        13 => RichBlockKind::Table,
        14 => RichBlockKind::Image,
        15 => RichBlockKind::File,
        16 => RichBlockKind::Whiteboard,
        17 => RichBlockKind::MindMap,
        18 => RichBlockKind::Embed,
        19 => RichBlockKind::Divider,
        20 => RichBlockKind::Database,
        21 => RichBlockKind::Attachment,
        22 => RichBlockKind::Separator,
        23 => RichBlockKind::FootnoteDefinition,
        24 => RichBlockKind::Comment,
        25 => RichBlockKind::RawMarkdown,
        31 => RichBlockKind::ColumnsGroup,
        32 => RichBlockKind::Column,
        33 => RichBlockKind::Video,
        // 未登记的 tag：本 build 不认识这个块类型（更新版本新增的内建块，或
        // 自定义/插件块的 u16::MAX）。tag 只是派生出来的布局提示，权威 kind 在
        // 载荷的 kind_json 里；这里当成没有 provider 的自定义块，得到只读的
        // stable box，而不是伪装成一个可编辑段落。
        _ => RichBlockKind::Custom(UNKNOWN_BLOCK_KIND_NAME.to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lightweight_kind_tags_preserve_all_heading_levels() {
        for level in 1..=6 {
            let kind = RichBlockKind::Heading { level };
            assert_eq!(
                rich_block_kind_from_tag(kind_tag_for_rich_block_kind(&kind)),
                kind
            );
        }
    }

    #[test]
    fn columns_kinds_have_stable_distinct_tags() {
        assert_eq!(rich_block_kind_from_tag(31), RichBlockKind::ColumnsGroup);
        assert_eq!(rich_block_kind_from_tag(32), RichBlockKind::Column);
        assert_ne!(
            kind_tag_for_rich_block_kind(&RichBlockKind::ColumnsGroup),
            kind_tag_for_rich_block_kind(&RichBlockKind::Column)
        );
    }

    #[test]
    fn a_kind_from_a_newer_build_round_trips_byte_for_byte() {
        // 旧版本读到新版本新增的内建 kind（这里模拟 `Audio`）。
        let decoded: RichBlockKind = serde_json::from_str("\"Audio\"").unwrap();

        let RichBlockKind::Unknown(unknown) = &decoded else {
            panic!("expected an unknown kind, got {decoded:?}")
        };
        assert_eq!(unknown.tag(), "Audio");
        assert_eq!(serde_json::to_string(&decoded).unwrap(), "\"Audio\"");
    }

    #[test]
    fn an_unknown_struct_shaped_kind_keeps_its_fields() {
        let future = "{\"Audio\":{\"loop_playback\":true}}";

        let decoded: RichBlockKind = serde_json::from_str(future).unwrap();

        assert!(matches!(&decoded, RichBlockKind::Unknown(unknown) if unknown.tag() == "Audio"));
        assert_eq!(serde_json::to_string(&decoded).unwrap(), future);
    }

    #[test]
    fn an_unknown_kind_behaves_like_a_provider_less_custom_block() {
        let kind: RichBlockKind = serde_json::from_str("\"Audio\"").unwrap();

        assert_eq!(kind.layout_behavior(), LayoutBehavior::CustomProvider);
        assert!(!kind.supports_children());
        assert!(!kind.supports_rich_text_title());
        assert_eq!(
            kind_tag_for_rich_block_kind(&kind),
            kind_tag_for_rich_block_kind(&RichBlockKind::Custom("plugin".to_owned()))
        );
    }

    #[test]
    fn known_kinds_still_decode_to_themselves() {
        let cases = [
            ("\"Paragraph\"", RichBlockKind::Paragraph),
            (
                "{\"Heading\":{\"level\":2}}",
                RichBlockKind::Heading { level: 2 },
            ),
            (
                "{\"Custom\":\"future.vendor/card\"}",
                RichBlockKind::Custom("future.vendor/card".to_owned()),
            ),
        ];
        for (json, expected) in cases {
            let decoded: RichBlockKind = serde_json::from_str(json).unwrap();
            assert_eq!(decoded, expected);
            assert_eq!(serde_json::to_string(&decoded).unwrap(), json);
        }
    }

    #[test]
    fn an_unregistered_kind_tag_falls_back_to_a_read_only_custom_block() {
        use crate::block::BlockInputCapability;

        assert_eq!(rich_block_kind_from_tag(1), RichBlockKind::Paragraph);

        // 更新版本新增的 tag、以及自定义/插件块的 u16::MAX：不再伪装成段落。
        for tag in [u16::MAX, 9_999] {
            let kind = rich_block_kind_from_tag(tag);
            assert_eq!(
                kind,
                RichBlockKind::Custom(UNKNOWN_BLOCK_KIND_NAME.to_owned())
            );
            assert_eq!(kind.layout_behavior(), LayoutBehavior::CustomProvider);
            assert!(!BlockInputCapability::for_kind(&kind).accepts_text_caret());
        }
    }

    #[test]
    fn video_has_a_stable_round_trip_tag() {
        assert_eq!(kind_tag_for_rich_block_kind(&RichBlockKind::Video), 33);
        assert_eq!(rich_block_kind_from_tag(33), RichBlockKind::Video);
        assert_eq!(
            RichBlockKind::Video.layout_behavior(),
            LayoutBehavior::StableBox
        );
        assert!(!RichBlockKind::Video.supports_rich_text_title());
    }
}
