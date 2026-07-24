use cditor_core::ids::BlockId;
use cditor_core::rich_text::{
    BlockPayload, BlockPayloadView, InlineSpan, RichBlockKind, TextAlign,
};
use cditor_runtime::{TextSurfaceSnapshot, ViewBlockSnapshot};
use cditor_text::{TextLayoutInput, TextLayoutSurfaceId};

#[derive(Debug, Clone, PartialEq)]
pub struct RichTextLayoutInput {
    pub block_id: BlockId,
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

impl RichTextLayoutInput {
    pub fn from_text_surface_snapshot(
        snapshot: TextSurfaceSnapshot,
        layout_version: u64,
        text_align: TextAlign,
        width_px: f64,
        theme_version: u64,
        font_version: u64,
    ) -> Self {
        Self {
            block_id: snapshot.owner_block_id,
            surface_id: snapshot.identity.surface_id,
            content_version: snapshot.identity.content_version,
            layout_version,
            kind: snapshot.kind,
            text_align,
            spans: snapshot.spans,
            width_px,
            theme_version,
            font_version,
        }
    }

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
            surface_id: TextLayoutSurfaceId::Block(snapshot.block_id),
            content_version: payload.content_version,
            layout_version: snapshot.layout.layout_version,
            kind: snapshot.kind.clone(),
            text_align: snapshot.attrs.text_align,
            spans,
            width_px,
            theme_version,
            font_version,
        })
    }

    pub(crate) fn to_text_layout_input(&self) -> TextLayoutInput {
        TextLayoutInput {
            surface_id: self.surface_id,
            content_version: self.content_version,
            layout_version: self.layout_version,
            kind: self.kind.clone(),
            text_align: self.text_align,
            spans: self.spans.clone(),
            width_px: self.width_px,
            theme_version: self.theme_version,
            font_version: self.font_version,
        }
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

    #[test]
    fn auxiliary_surface_layout_identity_keeps_content_and_layout_versions_distinct() {
        let runtime = DocumentRuntime::from_payloads(
            1,
            vec![cditor_core::rich_text::BlockPayloadRecord {
                block_id: 7,
                content_version: 9,
                kind: RichBlockKind::Image,
                payload: BlockPayload::Image(cditor_core::rich_text::ImagePayload {
                    caption: "caption".into(),
                    ..Default::default()
                }),
            }],
            720.0,
        );
        let snapshot = runtime
            .text_surface_snapshot(cditor_core::ids::SurfaceId::ImageCaption { block_id: 7 })
            .unwrap();

        let input = RichTextLayoutInput::from_text_surface_snapshot(
            snapshot,
            41,
            TextAlign::Center,
            560.0,
            3,
            5,
        );

        assert_eq!(input.content_version, 9);
        assert_eq!(input.layout_version, 41);
        assert_eq!(
            input.surface_id,
            cditor_core::ids::SurfaceId::ImageCaption { block_id: 7 }
        );
    }

    #[test]
    fn paragraph_heading_list_and_code_share_the_same_layout_input_pipeline() {
        let fixtures = [
            (
                1,
                RichBlockKind::Paragraph,
                BlockPayload::RichText {
                    spans: vec![InlineSpan::plain("paragraph")],
                },
            ),
            (
                2,
                RichBlockKind::Heading { level: 2 },
                BlockPayload::RichText {
                    spans: vec![InlineSpan::plain("heading")],
                },
            ),
            (
                3,
                RichBlockKind::BulletedList,
                BlockPayload::RichText {
                    spans: vec![InlineSpan::plain("list")],
                },
            ),
            (
                4,
                RichBlockKind::Code {
                    language: Some("rust".to_owned()),
                },
                BlockPayload::Code {
                    language: Some("rust".to_owned()),
                    text: "fn main() {}".to_owned(),
                },
            ),
        ];
        let runtime = DocumentRuntime::from_payloads(
            1,
            fixtures
                .iter()
                .map(
                    |(block_id, kind, payload)| cditor_core::rich_text::BlockPayloadRecord {
                        block_id: *block_id,
                        content_version: 1,
                        kind: kind.clone(),
                        payload: payload.clone(),
                    },
                )
                .collect(),
            720.0,
        );
        let projection = runtime.projection_for_window();

        for (block_id, kind, _) in fixtures {
            let snapshot = projection
                .blocks
                .iter()
                .find(|block| block.block_id == block_id)
                .expect("fixture block is visible");
            let input = RichTextLayoutInput::from_snapshot(snapshot, 860.0, 1, 1)
                .expect("primary text kind must use the shared layout input");

            assert_eq!(input.kind, kind);
            assert_eq!(input.surface_id, TextLayoutSurfaceId::Block(block_id));
            assert!(!input.to_text_layout_input().plain_text().is_empty());
        }
    }

    #[test]
    fn app_input_converts_to_framework_independent_text_input() {
        let input = RichTextLayoutInput {
            block_id: 9,
            surface_id: TextLayoutSurfaceId::Block(9),
            content_version: 2,
            layout_version: 3,
            kind: RichBlockKind::Paragraph,
            text_align: TextAlign::Center,
            spans: vec![InlineSpan::plain("centered")],
            width_px: 640.0,
            theme_version: 4,
            font_version: 5,
        };

        let text_input = input.to_text_layout_input();

        assert_eq!(text_input.surface_id.block_id(), Some(input.block_id));
        assert_eq!(text_input.kind, input.kind);
        assert_eq!(text_input.text_align, TextAlign::Center);
        assert_eq!(text_input.plain_text(), "centered");
        assert_eq!(text_input.theme_version, 4);
        assert_eq!(text_input.font_version, 5);
    }
}
