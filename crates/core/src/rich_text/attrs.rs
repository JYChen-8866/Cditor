use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockAttrs {
    pub color: Option<String>,
    pub background_color: Option<String>,
    pub text_align: TextAlign,
    pub indent: u16,
    pub folded: bool,
    pub locked: bool,
    pub custom: BTreeMap<String, String>,
}

impl Default for BlockAttrs {
    fn default() -> Self {
        Self {
            color: None,
            background_color: None,
            text_align: TextAlign::Start,
            indent: 0,
            folded: false,
            locked: false,
            custom: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    Start,
    Center,
    End,
}
