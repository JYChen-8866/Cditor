use super::*;

pub(super) fn payload_for_converted_kind(kind: &RichBlockKind, text: String) -> BlockPayload {
    match kind {
        RichBlockKind::Code { language } => BlockPayload::Code {
            language: language.clone(),
            text,
        },
        RichBlockKind::Html => BlockPayload::Html {
            html: text,
            sanitized: false,
        },
        RichBlockKind::Table => default_table_payload(text),
        RichBlockKind::Mermaid => super::mermaid::mermaid_payload_from_text(&text),
        RichBlockKind::Whiteboard => default_whiteboard_payload(),
        RichBlockKind::Video => {
            BlockPayload::Video(cditor_core::rich_text::VideoPayload::default())
        }
        RichBlockKind::Divider | RichBlockKind::Separator => BlockPayload::Empty,
        RichBlockKind::Image
        | RichBlockKind::File
        | RichBlockKind::Attachment
        | RichBlockKind::MindMap
        | RichBlockKind::Embed => BlockPayload::Empty,
        RichBlockKind::Database => {
            BlockPayload::Collection(cditor_core::rich_text::CollectionPayload {
                collection_id: 0,
                title: cditor_core::rich_text::RichTextContent::plain(text),
                ..Default::default()
            })
        }
        _ => BlockPayload::RichText {
            spans: vec![InlineSpan::plain(text)],
        },
    }
}
