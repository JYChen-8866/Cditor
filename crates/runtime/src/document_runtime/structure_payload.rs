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
        RichBlockKind::Whiteboard => default_whiteboard_payload(),
        RichBlockKind::Divider | RichBlockKind::Separator => BlockPayload::Empty,
        RichBlockKind::Image
        | RichBlockKind::File
        | RichBlockKind::Attachment
        | RichBlockKind::MindMap
        | RichBlockKind::Embed
        | RichBlockKind::Database => BlockPayload::Empty,
        _ => BlockPayload::RichText {
            spans: vec![InlineSpan::plain(text)],
        },
    }
}
