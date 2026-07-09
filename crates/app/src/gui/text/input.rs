use cditor_core::ids::BlockId;
use cditor_core::rich_text::{BlockPayload, BlockPayloadView, InlineSpan, RichBlockKind};
use cditor_runtime::ViewBlockSnapshot;

#[derive(Debug, Clone, PartialEq)]
pub struct RichTextLayoutInput {
    pub block_id: BlockId,
    pub content_version: u64,
    pub layout_version: u64,
    pub kind: RichBlockKind,
    pub spans: Vec<InlineSpan>,
    pub width_px: f64,
    pub theme_version: u64,
    pub font_version: u64,
}

impl RichTextLayoutInput {
    pub fn from_snapshot(
        snapshot: &ViewBlockSnapshot,
        width_px: f64,
        theme_version: u64,
        font_version: u64,
    ) -> Option<Self> {
        let BlockPayloadView::Loaded(payload) = &snapshot.payload else {
            return None;
        };
        let spans = editable_spans_from_payload(&payload.payload)?;

        Some(Self {
            block_id: snapshot.block_id,
            content_version: payload.content_version,
            layout_version: snapshot.layout.layout_version,
            kind: snapshot.kind.clone(),
            spans,
            width_px,
            theme_version,
            font_version,
        })
    }
}

fn editable_spans_from_payload(payload: &BlockPayload) -> Option<Vec<InlineSpan>> {
    match payload {
        BlockPayload::RichText { spans } => Some(spans.clone()),
        BlockPayload::Code { text, .. } => Some(vec![InlineSpan::plain(text)]),
        BlockPayload::Html { html, .. } => Some(vec![InlineSpan::plain(html)]),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cditor_runtime::DocumentRuntime;

    #[test]
    fn rich_text_layout_input_from_snapshot() {
        let runtime = DocumentRuntime::demo();
        let projection = runtime.projection_for_window();
        let snapshot = projection
            .blocks
            .iter()
            .find(|block| matches!(block.kind, RichBlockKind::Paragraph))
            .expect("demo contains paragraph block");

        let input = RichTextLayoutInput::from_snapshot(snapshot, 860.0, 1, 1)
            .expect("paragraph payload should produce rich text layout input");

        assert_eq!(input.block_id, snapshot.block_id);
        assert_eq!(input.content_version, 1);
        assert_eq!(input.layout_version, snapshot.layout.layout_version);
        assert!(matches!(input.kind, RichBlockKind::Paragraph));
        assert_eq!(input.width_px, 860.0);
        assert_eq!(input.theme_version, 1);
        assert_eq!(input.font_version, 1);
        assert!(!input.spans.is_empty());
    }

    #[test]
    fn code_payload_produces_editable_text_input() {
        let runtime = DocumentRuntime::from_payloads(
            1,
            vec![cditor_core::rich_text::BlockPayloadRecord {
                block_id: 1,
                content_version: 1,
                kind: RichBlockKind::Code {
                    language: Some("rust".to_owned()),
                },
                payload: BlockPayload::Code {
                    language: Some("rust".to_owned()),
                    text: "fn main() {}".to_owned(),
                },
            }],
            720.0,
        );
        let projection = runtime.projection_for_window();
        let snapshot = projection.blocks.first().expect("code block is visible");

        let input = RichTextLayoutInput::from_snapshot(snapshot, 860.0, 1, 1)
            .expect("code payload should produce editable text input");

        assert!(matches!(input.kind, RichBlockKind::Code { .. }));
        assert!(!input.spans.is_empty());
        assert!(input.spans.iter().any(|span| span.text.contains("fn ")));
    }
}
