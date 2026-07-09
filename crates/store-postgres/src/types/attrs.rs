use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DbBlockAttrs {
    pub color: Option<String>,
    pub background_color: Option<String>,
    pub text_align: DbTextAlign,
    pub indent: u16,
    pub folded: bool,
    pub locked: bool,
    pub custom: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DbTextAlign {
    Start,
    Center,
    End,
}

impl From<&BlockAttrs> for DbBlockAttrs {
    fn from(attrs: &BlockAttrs) -> Self {
        Self {
            color: attrs.color.clone(),
            background_color: attrs.background_color.clone(),
            text_align: DbTextAlign::from(attrs.text_align),
            indent: attrs.indent,
            folded: attrs.folded,
            locked: attrs.locked,
            custom: attrs.custom.clone(),
        }
    }
}

impl From<DbBlockAttrs> for BlockAttrs {
    fn from(attrs: DbBlockAttrs) -> Self {
        Self {
            color: attrs.color,
            background_color: attrs.background_color,
            text_align: TextAlign::from(attrs.text_align),
            indent: attrs.indent,
            folded: attrs.folded,
            locked: attrs.locked,
            custom: attrs.custom,
        }
    }
}

impl From<TextAlign> for DbTextAlign {
    fn from(align: TextAlign) -> Self {
        match align {
            TextAlign::Start => Self::Start,
            TextAlign::Center => Self::Center,
            TextAlign::End => Self::End,
        }
    }
}

impl From<DbTextAlign> for TextAlign {
    fn from(align: DbTextAlign) -> Self {
        match align {
            DbTextAlign::Start => Self::Start,
            DbTextAlign::Center => Self::Center,
            DbTextAlign::End => Self::End,
        }
    }
}

pub fn encode_block_attrs(attrs: &BlockAttrs) -> serde_json::Result<serde_json::Value> {
    serde_json::to_value(DbBlockAttrs::from(attrs))
}

pub fn decode_block_attrs(value: serde_json::Value) -> serde_json::Result<BlockAttrs> {
    serde_json::from_value::<DbBlockAttrs>(value).map(BlockAttrs::from)
}
