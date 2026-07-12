pub use cditor_core::rich_text::{CditorClipboardEnvelope, ClipboardSelection};

pub fn envelope_for_selection(
    source_document: Option<cditor_core::ids::DocumentId>,
    selection: ClipboardSelection,
) -> (String, CditorClipboardEnvelope) {
    let system_text = selection.plain_text();
    let envelope = CditorClipboardEnvelope::new(source_document, selection, &system_text);
    (system_text, envelope)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cditor_core::rich_text::{InlineMark, InlineSpan};

    #[test]
    fn envelope_exposes_only_plain_text_to_the_system_clipboard() {
        let selection = ClipboardSelection::Inline {
            spans: vec![InlineSpan {
                text: "hello".to_owned(),
                marks: vec![InlineMark::Bold, InlineMark::Italic],
            }],
        };
        let (system_text, envelope) = envelope_for_selection(Some(9), selection.clone());
        assert_eq!(system_text, "hello");
        let json = serde_json::to_string(&envelope).unwrap();
        assert_eq!(
            CditorClipboardEnvelope::decode_metadata(&json, &system_text)
                .unwrap()
                .selection,
            selection
        );
    }
}
