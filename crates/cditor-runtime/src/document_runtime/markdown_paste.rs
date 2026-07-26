use super::*;

pub(super) fn markdown_trace_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("CDITOR_TRACE_MARKDOWN")
            .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
            .unwrap_or(false)
    })
}

pub(super) fn trace_markdown(event: &str, details: impl std::fmt::Display) {
    if markdown_trace_enabled() {
        crate::diagnostics::write_stderr(format_args!("[cditor][markdown][{event}] {details}"));
    }
}

pub(super) fn markdown_trace_preview(text: &str) -> String {
    text.chars()
        .take(160)
        .collect::<String>()
        .replace('\r', "\\r")
        .replace('\n', "\\n")
}

impl DocumentRuntime {
    pub(super) fn try_apply_space_block_markdown_shortcut(
        &mut self,
        block_id: BlockId,
        caret: usize,
    ) -> Result<bool, String> {
        let text = self
            .document
            .text_models
            .get(&block_id)
            .ok_or_else(|| format!("missing text model for block {block_id}"))?
            .text()
            .to_owned();
        let caret = previous_char_boundary(&text, caret.min(text.len()));
        if caret != text.len() {
            return Ok(false);
        }
        let current_kind = self
            .document
            .payload_window
            .get(block_id)
            .map(|payload| payload.kind.clone())
            .unwrap_or(RichBlockKind::Paragraph);
        if let Some((kind, marker_len)) = block_kind_shortcut_with_marker_len(&(text.clone() + " "))
            && marker_len == text.len() + 1
            && should_apply_space_block_markdown_shortcut(&current_kind, &kind)
        {
            self.cancel_composition();
            self.push_undo_snapshot(block_id)?;
            let payload = if matches!(kind, RichBlockKind::Divider | RichBlockKind::Separator) {
                BlockPayload::Empty
            } else {
                BlockPayload::RichText { spans: Vec::new() }
            };
            self.replace_block_kind_and_payload(block_id, kind, payload)?;
            return Ok(true);
        }
        // Inside a Quote block, detect callout markers like [!TIP] and upgrade.
        if matches!(current_kind, RichBlockKind::Quote)
            && let Some(variant) = parse_callout_marker(&text)
        {
            self.cancel_composition();
            self.push_undo_snapshot(block_id)?;
            self.replace_block_kind_and_payload(
                block_id,
                RichBlockKind::Callout { variant },
                BlockPayload::RichText { spans: Vec::new() },
            )?;
            return Ok(true);
        }
        Ok(false)
    }

    pub(super) fn apply_inline_markdown_shortcut(
        &mut self,
        block_id: BlockId,
    ) -> Result<bool, String> {
        let Some(kind) = self
            .document
            .payload_window
            .get(block_id)
            .map(|payload| payload.kind.clone())
        else {
            return Ok(false);
        };
        if !matches!(
            kind,
            RichBlockKind::Paragraph
                | RichBlockKind::Heading { .. }
                | RichBlockKind::Quote
                | RichBlockKind::Callout { .. }
                | RichBlockKind::BulletedList
                | RichBlockKind::NumberedList
                | RichBlockKind::Todo { .. }
        ) {
            return Ok(false);
        }

        // Get current text and caret position
        let text = self
            .document
            .text_models
            .get(&block_id)
            .ok_or_else(|| format!("missing text model for block {block_id}"))?
            .text()
            .to_owned();

        let caret = self
            .editing
            .session
            .as_ref()
            .map(EditingSession::focus_offset)
            .unwrap_or(text.len());

        // Try incremental detection first
        if let Some(detection) = cditor_core::rich_text::detect_delimiter_at_caret(&text, caret) {
            let changed = self.apply_incremental_inline_markdown(block_id, &text, detection)?;
            if changed {
                let offset = self.caret_offset_for_block(block_id).unwrap_or_default();
                self.set_typing_mark_override(SurfaceId::Block(block_id), offset, Vec::new());
            }
            return Ok(changed);
        }

        // Fallback to full parse for complex cases (links, code blocks, etc.)
        let Some(spans) = markdown_inline_shortcut_spans(&text) else {
            return Ok(false);
        };
        self.replace_preapplied_block_kind_and_spans(block_id, kind, spans)?;
        let offset = self.caret_offset_for_block(block_id).unwrap_or_default();
        self.set_typing_mark_override(SurfaceId::Block(block_id), offset, Vec::new());
        Ok(true)
    }

    fn apply_incremental_inline_markdown(
        &mut self,
        block_id: BlockId,
        text: &str,
        detection: cditor_core::rich_text::DelimiterPairDetection,
    ) -> Result<bool, String> {
        let delim_len = detection.delimiter.len();

        // Calculate ranges in the original text
        let opening_delim_start = detection.source_range.start - delim_len;
        let closing_delim_end = detection.source_range.end + delim_len;
        let full_range_with_delims = opening_delim_start..closing_delim_end;

        // Extract the content between delimiters (without delims)
        let content_text = &text[detection.source_range.clone()];

        // Get existing spans to check for marks in the affected range
        let existing_spans = self
            .document
            .payload_window
            .get(block_id)
            .and_then(|p| match &p.payload {
                BlockPayload::RichText { spans } => Some(spans.clone()),
                _ => None,
            })
            .unwrap_or_else(|| vec![InlineSpan::plain(text.to_string())]);

        // Find existing marks in the content range
        let mut existing_marks_in_range = Vec::new();
        let mut cursor = 0;
        for span in &existing_spans {
            let span_start = cursor;
            let span_end = cursor + span.text.len();

            // Check if this span overlaps with the content range (not the full range with delims)
            if span_end > detection.source_range.start && span_start < detection.source_range.end {
                for mark in &span.marks {
                    if !existing_marks_in_range.contains(mark) {
                        existing_marks_in_range.push(mark.clone());
                    }
                }
            }

            cursor = span_end;
        }

        // Merge new mark with existing marks
        let mut combined_marks = existing_marks_in_range;
        if !combined_marks.contains(&detection.mark) {
            combined_marks.push(detection.mark.clone());
        }

        // Create new span with merged marks
        let new_span = InlineSpan {
            text: content_text.to_string(),
            marks: combined_marks,
        };

        // Update text model: replace "**content**" with "content"
        let model = self
            .document
            .text_models
            .get_mut(&block_id)
            .ok_or_else(|| format!("missing text model for block {block_id}"))?;

        let inserted = model
            .replace_range(full_range_with_delims.clone(), content_text)
            .map_err(|e| format!("{:?}", e))?;

        // Use the existing span splice logic
        let updated_spans = replace_rich_text_spans_with_spans(
            &existing_spans,
            full_range_with_delims,
            &[new_span],
        );

        // Update editing session
        if let Some(editing) = self.editing.session.as_mut() {
            editing.set_collapsed_selection(inserted.end);
        }

        // Update payload with new spans
        if let Some(payload) = self.document.payload_window.get_mut(block_id) {
            payload.content_version = self
                .editing
                .session
                .as_ref()
                .map(|e| e.content_version)
                .unwrap_or(payload.content_version + 1);
            payload.payload = BlockPayload::RichText {
                spans: updated_spans,
            };
        }

        Ok(true)
    }
}

fn should_apply_space_block_markdown_shortcut(
    current_kind: &RichBlockKind,
    shortcut_kind: &RichBlockKind,
) -> bool {
    matches!(current_kind, RichBlockKind::Paragraph)
        || (matches!(current_kind, RichBlockKind::BulletedList)
            && matches!(shortcut_kind, RichBlockKind::Todo { .. }))
        || (matches!(current_kind, RichBlockKind::Quote)
            && matches!(shortcut_kind, RichBlockKind::Callout { .. }))
}
