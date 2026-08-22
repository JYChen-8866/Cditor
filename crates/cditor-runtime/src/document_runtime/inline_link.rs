use std::ops::Range;

use cditor_core::rich_text::{InlineMark, InlineSpan, splice_spans_at_range};

use super::text_payload::{merge_inline_spans, push_inline_span, safe_char_range};

/// Replaces the link family (`Link` / `DocumentLink`) on the selected range.
/// `None` clears any link; `Some(href)` replaces an existing link with the new
/// destination instead of stacking a second mark, mirroring
/// `set_color_mark_for_range`'s set semantics (a toggle-by-equality mark
/// cannot re-target an existing link).
pub(super) fn set_link_mark_for_range(
    source: &[InlineSpan],
    range: Range<usize>,
    href: Option<&str>,
) -> Vec<InlineSpan> {
    let text = cditor_core::rich_text::plain_text_from_spans(source);
    let range = safe_char_range(&text, range);
    let replacement = href.map(|href| InlineMark::Link {
        href: href.to_owned(),
    });

    let mut output = Vec::new();
    let mut offset = 0usize;
    for span in source {
        let span_start = offset;
        let span_end = span_start + span.text.len();
        offset = span_end;
        let overlap_start = span_start.max(range.start);
        let overlap_end = span_end.min(range.end);
        if overlap_start >= overlap_end {
            push_inline_span(&mut output, &span.text, span.marks.clone());
            continue;
        }

        let local_start = overlap_start - span_start;
        let local_end = overlap_end - span_start;
        push_inline_span(&mut output, &span.text[..local_start], span.marks.clone());
        let mut selected_marks =
            Vec::with_capacity(span.marks.len() + usize::from(replacement.is_some()));
        let mut replaced_family = false;
        for mark in &span.marks {
            if matches!(
                mark,
                InlineMark::Link { .. } | InlineMark::DocumentLink { .. }
            ) {
                if !replaced_family && let Some(replacement) = replacement.as_ref() {
                    selected_marks.push(replacement.clone());
                }
                replaced_family = true;
            } else {
                selected_marks.push(mark.clone());
            }
        }
        if !replaced_family && let Some(replacement) = replacement.as_ref() {
            selected_marks.push(replacement.clone());
        }
        push_inline_span(
            &mut output,
            &span.text[local_start..local_end],
            selected_marks,
        );
        push_inline_span(&mut output, &span.text[local_end..], span.marks.clone());
    }
    merge_inline_spans(&mut output);
    output
}

/// Replaces the selected range's text with `text` carrying the link, keeping
/// the surrounding spans untouched. Used when the link popup edits the label
/// together with the destination.
pub(super) fn replace_range_with_linked_text(
    source: &[InlineSpan],
    range: Range<usize>,
    text: &str,
    href: Option<&str>,
) -> Vec<InlineSpan> {
    let plain = cditor_core::rich_text::plain_text_from_spans(source);
    let range = safe_char_range(&plain, range);
    let marks = href
        .map(|href| {
            vec![InlineMark::Link {
                href: href.to_owned(),
            }]
        })
        .unwrap_or_default();
    let replacement = if text.is_empty() {
        Vec::new()
    } else {
        vec![InlineSpan {
            text: text.to_owned(),
            marks,
        }]
    };
    let mut output = splice_spans_at_range(source, range, replacement);
    merge_inline_spans(&mut output);
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(text: &str, marks: Vec<InlineMark>) -> InlineSpan {
        InlineSpan {
            text: text.to_owned(),
            marks,
        }
    }

    fn link(href: &str) -> InlineMark {
        InlineMark::Link {
            href: href.to_owned(),
        }
    }

    #[test]
    fn setting_a_link_replaces_an_existing_destination_and_keeps_other_marks() {
        let source = vec![span(
            "styled",
            vec![InlineMark::Bold, link("https://old.example")],
        )];

        let updated = set_link_mark_for_range(&source, 0..6, Some("https://new.example"));

        assert_eq!(
            updated,
            vec![span(
                "styled",
                vec![InlineMark::Bold, link("https://new.example")]
            )]
        );
    }

    #[test]
    fn partial_selection_splits_spans_and_none_clears_the_link_family() {
        let source = vec![span("abcdef", vec![link("https://example.com")])];

        let linked = set_link_mark_for_range(&source, 2..4, None);

        assert_eq!(
            linked,
            vec![
                span("ab", vec![link("https://example.com")]),
                span("cd", Vec::new()),
                span("ef", vec![link("https://example.com")]),
            ]
        );
    }

    #[test]
    fn document_links_belong_to_the_same_family() {
        let source = vec![span(
            "doc",
            vec![InlineMark::DocumentLink {
                href: "aurin://node/1".to_owned(),
            }],
        )];

        let updated = set_link_mark_for_range(&source, 0..3, Some("https://example.com"));

        assert_eq!(
            updated,
            vec![span("doc", vec![link("https://example.com")])]
        );
    }

    #[test]
    fn replacing_the_label_splices_new_linked_text_between_neighbours() {
        let source = vec![span("before target after", vec![InlineMark::Italic])];

        let updated =
            replace_range_with_linked_text(&source, 7..13, "renamed", Some("https://example.com"));

        assert_eq!(
            updated,
            vec![
                span("before ", vec![InlineMark::Italic]),
                span("renamed", vec![link("https://example.com")]),
                span(" after", vec![InlineMark::Italic]),
            ]
        );
    }
}
