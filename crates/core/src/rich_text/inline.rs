use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct InlineSpan {
    pub text: String,
    pub marks: Vec<InlineMark>,
}

impl InlineSpan {
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            marks: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InlineMark {
    Bold,
    Italic,
    Underline,
    Strike,
    Code,
    Link { href: String },
    Color(String),
    Background(String),
}

pub fn plain_text_from_spans(spans: &[InlineSpan]) -> String {
    spans.iter().map(|span| span.text.as_str()).collect()
}
