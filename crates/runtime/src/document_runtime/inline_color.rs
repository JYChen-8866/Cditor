use std::ops::Range;

use cditor_core::rich_text::{InlineColorTarget, InlineMark, InlineSpan};

use super::text_payload::{merge_inline_spans, push_inline_span, safe_char_range};

pub(super) fn set_color_mark_for_range(
    source: &[InlineSpan],
    range: Range<usize>,
    target: InlineColorTarget,
    color: Option<&str>,
) -> Vec<InlineSpan> {
    let text = cditor_core::rich_text::plain_text_from_spans(source);
    let range = safe_char_range(&text, range);
    let replacement = color.map(|color| match target {
        InlineColorTarget::Text => InlineMark::Color(color.to_owned()),
        InlineColorTarget::Background => InlineMark::Background(color.to_owned()),
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
            if target.matches(mark) {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn span(text: &str, marks: Vec<InlineMark>) -> InlineSpan {
        InlineSpan {
            text: text.to_owned(),
            marks,
        }
    }

    #[test]
    fn setting_a_text_color_replaces_only_the_text_color_family() {
        let source = vec![span(
            "styled",
            vec![
                InlineMark::Bold,
                InlineMark::Color("#d44c47".to_owned()),
                InlineMark::Background("#fbf3db".to_owned()),
            ],
        )];

        let colored =
            set_color_mark_for_range(&source, 0..6, InlineColorTarget::Text, Some("#337ea9"));

        assert_eq!(
            colored,
            vec![span(
                "styled",
                vec![
                    InlineMark::Bold,
                    InlineMark::Color("#337ea9".to_owned()),
                    InlineMark::Background("#fbf3db".to_owned()),
                ],
            )]
        );
    }

    #[test]
    fn clearing_a_partial_background_splits_and_merges_without_touching_text_color() {
        let source = vec![span(
            "abcdef",
            vec![
                InlineMark::Color("#448361".to_owned()),
                InlineMark::Background("#fdebec".to_owned()),
            ],
        )];

        let cleared = set_color_mark_for_range(&source, 1..5, InlineColorTarget::Background, None);

        assert_eq!(
            cleared,
            vec![
                span(
                    "a",
                    vec![
                        InlineMark::Color("#448361".to_owned()),
                        InlineMark::Background("#fdebec".to_owned()),
                    ],
                ),
                span("bcde", vec![InlineMark::Color("#448361".to_owned())]),
                span(
                    "f",
                    vec![
                        InlineMark::Color("#448361".to_owned()),
                        InlineMark::Background("#fdebec".to_owned()),
                    ],
                ),
            ]
        );
    }
}
