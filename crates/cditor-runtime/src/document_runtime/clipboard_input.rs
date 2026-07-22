use cditor_import_export::clipboard::{CditorClipboardEnvelope, ClipboardSelection};

use super::*;

impl DocumentRuntime {
    pub(crate) fn apply_clipboard_data(
        &mut self,
        system_text: &str,
        metadata_json: Option<&str>,
    ) -> Result<bool, String> {
        let metadata_selection = metadata_json.and_then(|json| {
            CditorClipboardEnvelope::decode_metadata(json, system_text)
                .ok()
                .map(|envelope| envelope.selection)
        });
        let text = normalize_clipboard_line_endings(system_text);

        if let Some(selection @ ClipboardSelection::Table { .. }) = metadata_selection.as_ref()
            && self.paste_clipboard_selection(selection)?
        {
            return Ok(true);
        }
        if self.paste_delimited_table_text_at_focused_cell(&text)? {
            return Ok(true);
        }
        if looks_like_markdown_paste(&text) && self.insert_markdown_paste(&text)? {
            return Ok(true);
        }
        if let Some(selection) = metadata_selection.as_ref()
            && self.paste_clipboard_selection(selection)?
        {
            return Ok(true);
        }
        self.replace_text_from_paste(None, &text)
    }
}

fn normalize_clipboard_line_endings(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_line_endings_normalize_crlf_and_lone_cr() {
        assert_eq!(normalize_clipboard_line_endings("a\r\nb\rc"), "a\nb\nc");
    }
}
