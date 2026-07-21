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

/// The two exclusive inline color families supported by rich text spans.
///
/// A span may contain one text color and one background color at the same
/// time, but never multiple marks from the same family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InlineColorTarget {
    Text,
    Background,
}

impl InlineColorTarget {
    pub fn matches(self, mark: &InlineMark) -> bool {
        matches!(
            (self, mark),
            (Self::Text, InlineMark::Color(_)) | (Self::Background, InlineMark::Background(_))
        )
    }
}

pub fn plain_text_from_spans(spans: &[InlineSpan]) -> String {
    spans.iter().map(|span| span.text.as_str()).collect()
}
