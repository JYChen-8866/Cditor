use cditor_core::ids::SurfaceId;
use cditor_core::rich_text::{InlineSpan, RichBlockKind, TextAlign};

pub type TextLayoutSurfaceId = SurfaceId;

#[derive(Debug, Clone, PartialEq)]
pub struct TextLayoutInput {
    pub surface_id: TextLayoutSurfaceId,
    pub content_version: u64,
    pub layout_version: u64,
    pub kind: RichBlockKind,
    pub text_align: TextAlign,
    pub spans: Vec<InlineSpan>,
    pub width_px: f64,
    pub theme_version: u64,
    pub font_version: u64,
}

impl TextLayoutInput {
    pub fn plain_text(&self) -> String {
        self.spans.iter().map(|span| span.text.as_str()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_preserves_span_order() {
        let input = TextLayoutInput {
            surface_id: TextLayoutSurfaceId::Block(7),
            content_version: 3,
            layout_version: 4,
            kind: RichBlockKind::Paragraph,
            text_align: TextAlign::Start,
            spans: vec![InlineSpan::plain("ab"), InlineSpan::plain("中")],
            width_px: 320.0,
            theme_version: 5,
            font_version: 6,
        };

        assert_eq!(input.plain_text(), "ab中");
    }
}
