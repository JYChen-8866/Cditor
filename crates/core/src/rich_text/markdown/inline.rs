use super::*;

pub(super) fn parse_inline_markdown(markdown: &str) -> Vec<InlineSpan> {
    let parsed = parse_inline_markdown_extended(markdown);
    let mut spans = parsed.spans;
    merge_inline_spans(&mut spans);
    if spans.is_empty() {
        spans.push(InlineSpan::plain(String::new()));
    }
    spans
}

pub(super) struct InlineParseResult {
    pub(super) spans: Vec<InlineSpan>,
    pub(super) changed: bool,
}

pub(super) fn parse_inline_markdown_extended(text: &str) -> InlineParseResult {
    let mut spans = Vec::new();
    let mut plain = String::new();
    let mut changed = false;
    let mut cursor = 0usize;

    while cursor < text.len() {
        let rest = &text[cursor..];
        if let Some(consumed) = parse_markdown_image(rest) {
            plain.push_str(&rest[..consumed]);
            cursor += consumed;
            continue;
        }
        if let Some((label, href, consumed)) = parse_markdown_link(rest) {
            flush_plain_span(&mut spans, &mut plain);
            spans.push(InlineSpan {
                text: label.to_string(),
                marks: vec![InlineMark::Link {
                    href: href.to_string(),
                }],
            });
            cursor += consumed;
            changed = true;
            continue;
        }
        if let Some((content, consumed)) = parse_delimited_mark(rest, "**") {
            flush_plain_span(&mut spans, &mut plain);
            spans.push(InlineSpan {
                text: content.to_string(),
                marks: vec![InlineMark::Bold],
            });
            cursor += consumed;
            changed = true;
            continue;
        }
        if let Some((content, consumed)) = parse_delimited_mark(rest, "~~") {
            flush_plain_span(&mut spans, &mut plain);
            spans.push(InlineSpan {
                text: content.to_string(),
                marks: vec![InlineMark::Strike],
            });
            cursor += consumed;
            changed = true;
            continue;
        }
        if let Some((content, consumed)) = parse_delimited_mark(rest, "++") {
            flush_plain_span(&mut spans, &mut plain);
            spans.push(InlineSpan {
                text: content.to_string(),
                marks: vec![InlineMark::Underline],
            });
            cursor += consumed;
            changed = true;
            continue;
        }
        if let Some((content, consumed)) = parse_delimited_mark(rest, "*") {
            flush_plain_span(&mut spans, &mut plain);
            spans.push(InlineSpan {
                text: content.to_string(),
                marks: vec![InlineMark::Italic],
            });
            cursor += consumed;
            changed = true;
            continue;
        }
        if let Some((content, consumed)) = parse_delimited_mark(rest, "_") {
            flush_plain_span(&mut spans, &mut plain);
            spans.push(InlineSpan {
                text: content.to_string(),
                marks: vec![InlineMark::Italic],
            });
            cursor += consumed;
            changed = true;
            continue;
        }
        if let Some((url, consumed)) = parse_auto_link(rest) {
            flush_plain_span(&mut spans, &mut plain);
            spans.push(InlineSpan {
                text: url.to_string(),
                marks: vec![InlineMark::Link {
                    href: url.to_string(),
                }],
            });
            cursor += consumed;
            changed = true;
            continue;
        }
        if let Some((content, consumed)) = parse_delimited_mark(rest, "``") {
            flush_plain_span(&mut spans, &mut plain);
            spans.push(InlineSpan {
                text: content.to_string(),
                marks: vec![InlineMark::Code],
            });
            cursor += consumed;
            changed = true;
            continue;
        }
        if let Some((content, consumed)) = parse_delimited_mark(rest, "`") {
            flush_plain_span(&mut spans, &mut plain);
            spans.push(InlineSpan {
                text: content.to_string(),
                marks: vec![InlineMark::Code],
            });
            cursor += consumed;
            changed = true;
            continue;
        }

        let Some(ch) = rest.chars().next() else {
            break;
        };
        plain.push(ch);
        cursor += ch.len_utf8();
    }

    flush_plain_span(&mut spans, &mut plain);
    merge_inline_spans(&mut spans);
    InlineParseResult { spans, changed }
}

fn flush_plain_span(spans: &mut Vec<InlineSpan>, plain: &mut String) {
    if !plain.is_empty() {
        spans.push(InlineSpan::plain(std::mem::take(plain)));
    }
}

fn merge_inline_spans(spans: &mut Vec<InlineSpan>) {
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
    *spans = merged;
}

fn parse_delimited_mark<'a>(text: &'a str, delimiter: &str) -> Option<(&'a str, usize)> {
    let inner = text.strip_prefix(delimiter)?;
    let end = inner.find(delimiter)?;
    if end == 0 {
        return None;
    }
    Some((&inner[..end], delimiter.len() + end + delimiter.len()))
}

fn parse_auto_link(text: &str) -> Option<(&str, usize)> {
    let prefix_len = if text.starts_with("https://") {
        "https://".len()
    } else if text.starts_with("http://") {
        "http://".len()
    } else {
        return None;
    };
    let mut end = prefix_len;
    for (index, ch) in text[prefix_len..].char_indices() {
        if ch.is_whitespace() || matches!(ch, '<' | '>' | '[' | ']' | '(' | ')' | '"') {
            break;
        }
        end = prefix_len + index + ch.len_utf8();
    }
    while end > prefix_len
        && text[..end]
            .chars()
            .next_back()
            .is_some_and(|ch| matches!(ch, '.' | ',' | ';' | ':' | '!' | '?'))
    {
        end -= text[..end].chars().next_back().map_or(0, char::len_utf8);
    }
    (end > prefix_len).then_some((&text[..end], end))
}

fn parse_markdown_image(text: &str) -> Option<usize> {
    let inner = text.strip_prefix("![")?;
    let label_end = inner.find("](")?;
    let after_label = &inner[label_end + 2..];
    let href_end = after_label.find(')')?;
    Some(2 + label_end + 2 + href_end + 1)
}

fn parse_markdown_link(text: &str) -> Option<(&str, &str, usize)> {
    let inner = text.strip_prefix('[')?;
    let label_end = inner.find("](")?;
    if label_end == 0 {
        return None;
    }
    let after_label = &inner[label_end + 2..];
    let href_end = after_label.find(')')?;
    if href_end == 0 {
        return None;
    }
    let label = &inner[..label_end];
    let href = &after_label[..href_end];
    Some((label, href, 1 + label_end + 2 + href_end + 1))
}
