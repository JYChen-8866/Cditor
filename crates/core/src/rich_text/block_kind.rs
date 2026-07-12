use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RichBlockKind {
    Paragraph,
    Heading { level: u8 },
    Quote,
    Callout { variant: CalloutVariant },
    Todo { checked: bool },
    BulletedList,
    NumberedList,
    Toggle,
    Code { language: Option<String> },
    Math,
    Mermaid,
    Html,
    Table,
    Image,
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
    pub fn layout_behavior(&self) -> LayoutBehavior {
        match self {
            Self::Paragraph
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
            | Self::File
            | Self::Attachment
            | Self::Whiteboard
            | Self::MindMap
            | Self::Embed => LayoutBehavior::StableBox,
            Self::Divider | Self::Separator => LayoutBehavior::FixedHeight,
            Self::Math | Self::Mermaid | Self::Html | Self::RawMarkdown | Self::Custom(_) => {
                LayoutBehavior::CustomProvider
            }
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
        )
    }

    pub fn supports_rich_text_title(&self) -> bool {
        !matches!(
            self,
            Self::Divider
                | Self::Separator
                | Self::Image
                | Self::File
                | Self::Attachment
                | Self::Whiteboard
                | Self::MindMap
        )
    }
}

pub fn kind_tag_for_rich_block_kind(kind: &RichBlockKind) -> u16 {
    match kind {
        RichBlockKind::Paragraph => 1,
        RichBlockKind::Heading { .. } => 2,
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
        RichBlockKind::Custom(_) => u16::MAX,
    }
}

pub fn rich_block_kind_from_tag(tag: u16) -> RichBlockKind {
    match tag {
        2 => RichBlockKind::Heading { level: 1 },
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
        _ => RichBlockKind::Paragraph,
    }
}
