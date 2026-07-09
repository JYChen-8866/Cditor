use super::*;

pub(super) fn sync_payload_from_model_after_replace(
    payload_window: &mut PayloadWindow,
    block_id: BlockId,
    content_version: u64,
    model: &PieceTableTextModel,
    replaced_range: Range<usize>,
    inserted_text: &str,
) {
    if let Some(payload) = payload_window.payloads.get_mut(&block_id) {
        payload.content_version = content_version;
        payload.payload = text_payload_for_existing_after_replace(
            &payload.payload,
            model.text(),
            replaced_range,
            inserted_text,
        );
    }
}

pub(super) fn merge_inline_spans(spans: &mut Vec<InlineSpan>) {
    let mut merged: Vec<InlineSpan> = Vec::new();
    for span in spans.drain(..) {
        if span.text.is_empty() {
            continue;
        }
        if let Some(last) = merged.last_mut()
            && last.marks == span.marks
        {
            last.text.push_str(&span.text);
            continue;
        }
        merged.push(span);
    }
    if merged.is_empty() {
        merged.push(InlineSpan::plain(String::new()));
    }
    *spans = merged;
}

pub(super) fn prepend_plain_text_to_payload(prefix: String, payload: BlockPayload) -> BlockPayload {
    if prefix.is_empty() {
        return payload;
    }
    match payload {
        BlockPayload::RichText { mut spans } => {
            spans.insert(0, InlineSpan::plain(prefix));
            merge_inline_spans(&mut spans);
            BlockPayload::RichText { spans }
        }
        BlockPayload::Code { language, text } => BlockPayload::Code {
            language,
            text: format!("{prefix}{text}"),
        },
        BlockPayload::Html { html, sanitized } => BlockPayload::Html {
            html: format!("{prefix}{html}"),
            sanitized,
        },
        other => BlockPayload::RichText {
            spans: vec![InlineSpan::plain(format!("{prefix}{}", other.plain_text()))],
        },
    }
}

pub(super) fn append_plain_text_to_payload(payload: BlockPayload, suffix: String) -> BlockPayload {
    if suffix.is_empty() {
        return payload;
    }
    match payload {
        BlockPayload::RichText { mut spans } => {
            spans.push(InlineSpan::plain(suffix));
            merge_inline_spans(&mut spans);
            BlockPayload::RichText { spans }
        }
        BlockPayload::Code { language, text } => BlockPayload::Code {
            language,
            text: format!("{text}{suffix}"),
        },
        BlockPayload::Html { html, sanitized } => BlockPayload::Html {
            html: format!("{html}{suffix}"),
            sanitized,
        },
        other => BlockPayload::RichText {
            spans: vec![InlineSpan::plain(format!("{}{suffix}", other.plain_text()))],
        },
    }
}

pub(super) fn text_payload_for_existing(existing: &BlockPayload, text: &str) -> BlockPayload {
    match existing {
        BlockPayload::Code { language, .. } => BlockPayload::Code {
            language: language.clone(),
            text: text.to_owned(),
        },
        BlockPayload::Html { sanitized, .. } => BlockPayload::Html {
            html: text.to_owned(),
            sanitized: sanitized.clone(),
        },
        _ => BlockPayload::RichText {
            spans: vec![InlineSpan::plain(text)],
        },
    }
}

pub(super) fn text_payload_for_existing_after_replace(
    existing: &BlockPayload,
    updated_text: &str,
    replaced_range: Range<usize>,
    inserted_text: &str,
) -> BlockPayload {
    match existing {
        BlockPayload::Code { language, .. } => BlockPayload::Code {
            language: language.clone(),
            text: updated_text.to_owned(),
        },
        BlockPayload::Html { sanitized, .. } => BlockPayload::Html {
            html: updated_text.to_owned(),
            sanitized: sanitized.clone(),
        },
        BlockPayload::RichText { spans } => BlockPayload::RichText {
            spans: replace_rich_text_spans_preserving_marks(spans, replaced_range, inserted_text),
        },
        _ => text_payload_for_existing(existing, updated_text),
    }
}

pub(super) fn replace_rich_text_spans_preserving_marks(
    spans: &[InlineSpan],
    replaced_range: Range<usize>,
    inserted_text: &str,
) -> Vec<InlineSpan> {
    let mut output = Vec::new();
    let mut cursor = 0usize;
    let insertion_marks = marks_for_insertion(spans, replaced_range.start);
    let mut inserted = false;
    for span in spans {
        let span_start = cursor;
        let span_end = span_start + span.text.len();
        if span_end <= replaced_range.start || span_start >= replaced_range.end {
            if !inserted && span_start >= replaced_range.end {
                push_inline_span(&mut output, inserted_text, insertion_marks.clone());
                inserted = true;
            }
            output.push(span.clone());
        } else {
            let keep_prefix_end = replaced_range
                .start
                .saturating_sub(span_start)
                .min(span.text.len());
            let keep_suffix_start = replaced_range
                .end
                .saturating_sub(span_start)
                .min(span.text.len());
            if keep_prefix_end > 0 {
                push_inline_span(
                    &mut output,
                    &span.text[..keep_prefix_end],
                    span.marks.clone(),
                );
            }
            if !inserted {
                push_inline_span(&mut output, inserted_text, insertion_marks.clone());
                inserted = true;
            }
            if keep_suffix_start < span.text.len() {
                push_inline_span(
                    &mut output,
                    &span.text[keep_suffix_start..],
                    span.marks.clone(),
                );
            }
        }
        cursor = span_end;
    }
    if !inserted {
        push_inline_span(&mut output, inserted_text, insertion_marks);
    }
    merge_inline_spans(&mut output);
    if output.is_empty() {
        output.push(InlineSpan::plain(String::new()));
    }
    output
}

pub(super) fn replace_rich_text_spans_with_spans(
    spans: &[InlineSpan],
    replaced_range: Range<usize>,
    inserted_spans: &[InlineSpan],
) -> Vec<InlineSpan> {
    let mut output = Vec::new();
    let mut cursor = 0usize;
    let mut inserted = false;
    for span in spans {
        let span_start = cursor;
        let span_end = span_start + span.text.len();
        if span_end <= replaced_range.start || span_start >= replaced_range.end {
            if !inserted && span_start >= replaced_range.end {
                push_inline_spans(&mut output, inserted_spans);
                inserted = true;
            }
            output.push(span.clone());
        } else {
            let keep_prefix_end = replaced_range
                .start
                .saturating_sub(span_start)
                .min(span.text.len());
            let keep_suffix_start = replaced_range
                .end
                .saturating_sub(span_start)
                .min(span.text.len());
            if keep_prefix_end > 0 {
                push_inline_span(
                    &mut output,
                    &span.text[..keep_prefix_end],
                    span.marks.clone(),
                );
            }
            if !inserted {
                push_inline_spans(&mut output, inserted_spans);
                inserted = true;
            }
            if keep_suffix_start < span.text.len() {
                push_inline_span(
                    &mut output,
                    &span.text[keep_suffix_start..],
                    span.marks.clone(),
                );
            }
        }
        cursor = span_end;
    }
    if !inserted {
        push_inline_spans(&mut output, inserted_spans);
    }
    merge_inline_spans(&mut output);
    output
}

pub(super) fn slice_rich_text_spans(spans: &[InlineSpan], range: Range<usize>) -> Vec<InlineSpan> {
    let mut output = Vec::new();
    let mut cursor = 0usize;
    for span in spans {
        let span_start = cursor;
        let span_end = span_start + span.text.len();
        if span_end > range.start && span_start < range.end {
            let local_start = range.start.saturating_sub(span_start).min(span.text.len());
            let local_end = range.end.saturating_sub(span_start).min(span.text.len());
            if local_start < local_end {
                push_inline_span(
                    &mut output,
                    &span.text[local_start..local_end],
                    span.marks.clone(),
                );
            }
        }
        cursor = span_end;
    }
    merge_inline_spans(&mut output);
    output
}

pub(super) fn marks_for_insertion(spans: &[InlineSpan], offset: usize) -> Vec<InlineMark> {
    let mut cursor = 0usize;
    for span in spans {
        let span_start = cursor;
        let span_end = span_start + span.text.len();
        if span_start <= offset && offset < span_end {
            return span.marks.clone();
        }
        if offset == span_end && !span.marks.is_empty() {
            return span.marks.clone();
        }
        cursor = span_end;
    }
    Vec::new()
}

pub(super) fn push_inline_span(output: &mut Vec<InlineSpan>, text: &str, marks: Vec<InlineMark>) {
    if !text.is_empty() {
        output.push(InlineSpan {
            text: text.to_owned(),
            marks,
        });
    }
}

fn push_inline_spans(output: &mut Vec<InlineSpan>, spans: &[InlineSpan]) {
    for span in spans {
        push_inline_span(output, &span.text, span.marks.clone());
    }
}

pub(super) fn backspace_at_start_resets_kind_to_paragraph(kind: &RichBlockKind) -> bool {
    matches!(
        kind,
        RichBlockKind::Heading { .. }
            | RichBlockKind::Quote
            | RichBlockKind::Callout { .. }
            | RichBlockKind::Todo { .. }
            | RichBlockKind::BulletedList
            | RichBlockKind::NumberedList
            | RichBlockKind::Toggle
            | RichBlockKind::Code { .. }
            | RichBlockKind::Math
            | RichBlockKind::Mermaid
            | RichBlockKind::Html
            | RichBlockKind::FootnoteDefinition
            | RichBlockKind::Comment
            | RichBlockKind::RawMarkdown
            | RichBlockKind::Custom(_)
    )
}

pub(super) fn uses_soft_tab(kind: &RichBlockKind) -> bool {
    matches!(
        kind,
        RichBlockKind::Code { .. }
            | RichBlockKind::RawMarkdown
            | RichBlockKind::Quote
            | RichBlockKind::Callout { .. }
    )
}

pub(super) fn newline_sibling_kind_for_v1(kind: &RichBlockKind) -> RichBlockKind {
    match kind {
        RichBlockKind::Todo { .. } => RichBlockKind::Todo { checked: false },
        RichBlockKind::BulletedList => RichBlockKind::BulletedList,
        RichBlockKind::NumberedList => RichBlockKind::NumberedList,
        RichBlockKind::Quote => RichBlockKind::Quote,
        RichBlockKind::Callout { variant } => RichBlockKind::Callout { variant: *variant },
        _ => RichBlockKind::Paragraph,
    }
}

pub(super) fn split_payload_for_enter(
    payload: &BlockPayload,
    offset: usize,
    new_kind: &RichBlockKind,
) -> (BlockPayload, BlockPayload) {
    match payload {
        BlockPayload::RichText { spans } => {
            let (leading, trailing) = split_inline_spans_at_offset(spans, offset);
            (
                BlockPayload::RichText { spans: leading },
                payload_for_kind_from_plain_or_spans(new_kind, trailing),
            )
        }
        BlockPayload::Code { language, text } => {
            let offset = previous_char_boundary(text, offset.min(text.len()));
            let leading = text[..offset].to_owned();
            let trailing = text[offset..].to_owned();
            (
                BlockPayload::Code {
                    language: language.clone(),
                    text: leading,
                },
                payload_for_kind_from_plain_text(new_kind, trailing),
            )
        }
        BlockPayload::Html { html, sanitized } => {
            let offset = previous_char_boundary(html, offset.min(html.len()));
            let leading = html[..offset].to_owned();
            let trailing = html[offset..].to_owned();
            (
                BlockPayload::Html {
                    html: leading,
                    sanitized: *sanitized,
                },
                payload_for_kind_from_plain_text(new_kind, trailing),
            )
        }
        BlockPayload::Table(table) => (
            BlockPayload::Table(table.clone()),
            payload_for_kind_from_plain_text(new_kind, String::new()),
        ),
        other => {
            let text = other.plain_text();
            let offset = previous_char_boundary(&text, offset.min(text.len()));
            let leading = text[..offset].to_owned();
            let trailing = text[offset..].to_owned();
            (
                payload_for_kind_from_plain_text(new_kind, leading),
                payload_for_kind_from_plain_text(new_kind, trailing),
            )
        }
    }
}

pub(super) fn payload_for_kind_from_plain_or_spans(
    kind: &RichBlockKind,
    spans: Vec<InlineSpan>,
) -> BlockPayload {
    match kind {
        RichBlockKind::Code { language } => BlockPayload::Code {
            language: language.clone(),
            text: cditor_core::rich_text::plain_text_from_spans(&spans),
        },
        RichBlockKind::Html => BlockPayload::Html {
            html: cditor_core::rich_text::plain_text_from_spans(&spans),
            sanitized: true,
        },
        _ => BlockPayload::RichText { spans },
    }
}

pub(super) fn payload_for_kind_from_plain_text(kind: &RichBlockKind, text: String) -> BlockPayload {
    match kind {
        RichBlockKind::Code { language } => BlockPayload::Code {
            language: language.clone(),
            text,
        },
        RichBlockKind::Html => BlockPayload::Html {
            html: text,
            sanitized: true,
        },
        RichBlockKind::Table => default_table_payload(text),
        _ => BlockPayload::RichText {
            spans: vec![InlineSpan::plain(text)],
        },
    }
}

pub(super) fn split_inline_spans_at_offset(
    spans: &[InlineSpan],
    offset: usize,
) -> (Vec<InlineSpan>, Vec<InlineSpan>) {
    let mut leading = Vec::new();
    let mut trailing = Vec::new();
    let mut cursor = 0usize;
    let split_offset = offset.min(cditor_core::rich_text::plain_text_from_spans(spans).len());

    for span in spans {
        let span_start = cursor;
        let span_end = cursor + span.text.len();
        if span_end <= split_offset {
            leading.push(span.clone());
        } else if span_start >= split_offset {
            trailing.push(span.clone());
        } else {
            let local = previous_char_boundary(&span.text, split_offset - span_start);
            let left_text = span.text[..local].to_owned();
            let right_text = span.text[local..].to_owned();
            if !left_text.is_empty() {
                leading.push(InlineSpan {
                    text: left_text,
                    marks: span.marks.clone(),
                });
            }
            if !right_text.is_empty() {
                trailing.push(InlineSpan {
                    text: right_text,
                    marks: span.marks.clone(),
                });
            }
        }
        cursor = span_end;
    }

    if leading.is_empty() {
        leading.push(InlineSpan::plain(String::new()));
    }
    if trailing.is_empty() {
        trailing.push(InlineSpan::plain(String::new()));
    }
    (leading, trailing)
}

pub(super) fn previous_char_boundary(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while offset > 0 && !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

pub(super) fn previous_grapheme_boundary(text: &str, offset: usize) -> usize {
    let offset = previous_char_boundary(text, offset);
    text[..offset]
        .char_indices()
        .next_back()
        .map(|(index, _)| index)
        .unwrap_or(0)
}

pub(super) fn next_grapheme_boundary(text: &str, offset: usize) -> usize {
    let offset = next_char_boundary(text, offset);
    text[offset..]
        .char_indices()
        .nth(1)
        .map(|(index, _)| offset + index)
        .unwrap_or(text.len())
}

pub(super) fn next_char_boundary(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while offset < text.len() && !text.is_char_boundary(offset) {
        offset += 1;
    }
    offset
}

pub(super) fn safe_char_range(text: &str, range: Range<usize>) -> Range<usize> {
    let start = previous_char_boundary(text, range.start.min(text.len()));
    let end = next_char_boundary(text, range.end.min(text.len())).max(start);
    start..end
}

pub(super) fn spans_with_mark_for_range(
    text: &str,
    range: Range<usize>,
    mark: InlineMark,
) -> Vec<InlineSpan> {
    let range = safe_char_range(text, range);
    let mut spans = Vec::new();
    if range.start > 0 {
        spans.push(InlineSpan::plain(&text[..range.start]));
    }
    spans.push(InlineSpan {
        text: text[range.clone()].to_owned(),
        marks: vec![mark],
    });
    if range.end < text.len() {
        spans.push(InlineSpan::plain(&text[range.end..]));
    }
    spans
}
