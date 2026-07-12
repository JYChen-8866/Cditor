/// Span splicing utilities for incremental inline markdown application
///
/// Instead of re-parsing and replacing the entire line, we splice new marks
/// into a specific range while preserving marks outside that range.
use super::inline::{InlineMark, InlineSpan};
use std::ops::Range;

/// Result of detecting a newly closed delimiter pair at caret
#[derive(Debug, Clone, PartialEq)]
pub struct DelimiterPairDetection {
    /// The range in the text model that should be transformed
    pub source_range: Range<usize>,
    /// The delimiter string (e.g., "**", "~~", "*")
    pub delimiter: String,
    /// The mark to apply (e.g., Bold, Strike, Italic)
    pub mark: InlineMark,
}

/// Splice new spans into an existing span list at a specific text range
///
/// This preserves marks outside the source_range and merges marks within it.
pub fn splice_spans_at_range(
    existing_spans: &[InlineSpan],
    source_range: Range<usize>,
    new_spans: Vec<InlineSpan>,
) -> Vec<InlineSpan> {
    let mut result = Vec::new();
    let mut text_offset = 0;

    for span in existing_spans {
        let span_start = text_offset;
        let span_end = text_offset + span.text.len();

        // Case 1: Span is entirely before the source range
        if span_end <= source_range.start {
            result.push(span.clone());
            text_offset = span_end;
            continue;
        }

        // Case 2: Span is entirely after the source range
        if span_start >= source_range.end {
            result.push(span.clone());
            text_offset = span_end;
            continue;
        }

        // Case 3: Span overlaps with source range - need to split it

        // Part before source range
        if span_start < source_range.start {
            let before_len = source_range.start - span_start;
            result.push(InlineSpan {
                text: span.text[..before_len].to_string(),
                marks: span.marks.clone(),
            });
        }

        // Part within source range will be replaced by new_spans
        // (handled after this loop)

        // Part after source range
        if span_end > source_range.end {
            let after_start = source_range.end - span_start;
            result.push(InlineSpan {
                text: span.text[after_start..].to_string(),
                marks: span.marks.clone(),
            });
        }

        text_offset = span_end;
    }

    // Insert new spans at the appropriate position
    // Find insertion point
    let mut insert_at = 0;
    let mut current_offset = 0;
    for (idx, span) in result.iter().enumerate() {
        if current_offset >= source_range.start {
            insert_at = idx;
            break;
        }
        current_offset += span.text.len();
        insert_at = idx + 1;
    }

    // Insert new spans
    for new_span in new_spans.into_iter().rev() {
        result.insert(insert_at, new_span);
    }

    result
}

/// Detect if there's a newly closed delimiter pair at the current caret position
///
/// Returns None if no complete pair is found, or if the pair was already processed.
pub fn detect_delimiter_at_caret(text: &str, caret: usize) -> Option<DelimiterPairDetection> {
    if caret == 0 {
        return None;
    }

    // Check for various delimiter patterns at caret position
    // We look backwards from caret to find matching opening delimiter
    // IMPORTANT: Check longer delimiters first to avoid matching shorter ones incorrectly

    // Try **bold** (2 chars) - must check before single *
    if caret >= 2 && text.get(caret.saturating_sub(2)..caret) == Some("**") {
        if let Some(range) = find_matching_delimiter_pair(text, caret, "**") {
            return Some(DelimiterPairDetection {
                source_range: range,
                delimiter: "**".to_string(),
                mark: InlineMark::Bold,
            });
        }
    }

    // Try ~~strikethrough~~ (2 chars)
    if caret >= 2 && text.get(caret.saturating_sub(2)..caret) == Some("~~") {
        if let Some(range) = find_matching_delimiter_pair(text, caret, "~~") {
            return Some(DelimiterPairDetection {
                source_range: range,
                delimiter: "~~".to_string(),
                mark: InlineMark::Strike,
            });
        }
    }

    // Try *italic* (1 char) - checked after ** to avoid conflicts
    if caret >= 1 && text.as_bytes()[caret - 1] == b'*' {
        // Make sure it's not part of ** that we just checked
        let is_double_star = caret >= 2 && text.as_bytes()[caret - 2] == b'*';
        if !is_double_star {
            if let Some(range) = find_matching_delimiter_pair(text, caret, "*") {
                return Some(DelimiterPairDetection {
                    source_range: range,
                    delimiter: "*".to_string(),
                    mark: InlineMark::Italic,
                });
            }
        }
    }

    None
}

/// Find the matching opening delimiter for a closing delimiter at caret
///
/// Returns the range of text between delimiters (excluding the delimiters themselves)
fn find_matching_delimiter_pair(text: &str, caret: usize, delimiter: &str) -> Option<Range<usize>> {
    let delim_len = delimiter.len();
    if caret < delim_len {
        return None;
    }

    // Verify we actually have the closing delimiter at caret
    let closing_start = caret - delim_len;
    if &text[closing_start..caret] != delimiter {
        return None;
    }

    // Start searching backwards from before the closing delimiter
    let search_start = closing_start;
    let search_text = &text[..search_start];

    // Find the last occurrence of the opening delimiter
    let opening_pos = search_text.rfind(delimiter)?;

    // Check that there's actual content between delimiters
    let content_start = opening_pos + delim_len;
    let content_end = search_start;

    if content_end <= content_start {
        return None; // No content between delimiters
    }

    // For ** and *, we need to ensure we're not matching misaligned delimiters
    if delimiter == "**" {
        // Make sure the opening ** is not preceded by another *
        if opening_pos > 0 && text.as_bytes()[opening_pos - 1] == b'*' {
            return None; // This is part of *** or more
        }
        // Make sure there's not another * right after opening **
        if opening_pos + 2 < text.len() && text.as_bytes()[opening_pos + 2] == b'*' {
            return None; // This is part of *** or more
        }
    }

    if delimiter == "*" {
        // For single *, make sure it's not part of **
        if opening_pos > 0 && text.as_bytes()[opening_pos - 1] == b'*' {
            return None;
        }
        if opening_pos + 1 < text.len() && text.as_bytes()[opening_pos + 1] == b'*' {
            return None;
        }
        // Also check the closing *
        if closing_start > 0 && text.as_bytes()[closing_start - 1] == b'*' {
            return None;
        }
        if caret < text.len() && text.as_bytes()[caret] == b'*' {
            return None;
        }
    }

    Some(content_start..content_end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_delimiter_bold() {
        let text = "**hello**";
        let detection = detect_delimiter_at_caret(text, 9);
        assert!(detection.is_some());
        let det = detection.unwrap();
        assert_eq!(det.source_range, 2..7); // "hello" without delimiters
        assert_eq!(det.mark, InlineMark::Bold);
    }

    #[test]
    fn test_detect_delimiter_strike() {
        let text = "~~world~~";
        let detection = detect_delimiter_at_caret(text, 9);
        assert!(detection.is_some());
        let det = detection.unwrap();
        assert_eq!(det.source_range, 2..7); // "world" without delimiters
        assert_eq!(det.mark, InlineMark::Strike);
    }

    #[test]
    fn test_splice_spans_simple() {
        let existing = vec![
            InlineSpan::plain("Hello ".to_string()),
            InlineSpan::plain("world".to_string()),
        ];

        let new_spans = vec![InlineSpan {
            text: "world".to_string(),
            marks: vec![InlineMark::Bold],
        }];

        let result = splice_spans_at_range(&existing, 6..11, new_spans);

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].text, "Hello ");
        assert_eq!(result[1].text, "world");
        assert_eq!(result[1].marks, vec![InlineMark::Bold]);
    }
}
