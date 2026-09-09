use super::{InlineMark, InlineSpan};
use crate::markdown::{MarkdownSyntax, SyntaxKind};
use pulldown_cmark::TagEnd;

pub fn parse_inline_markdown(markdown: &str) -> Vec<InlineSpan> {
    parse_inline_markdown_extended(markdown).spans
}

pub struct InlineMarkdownParseResult {
    pub spans: Vec<InlineSpan>,
    pub changed: bool,
}

/// CommonMark parsing and editor shortcut policy are separate: incomplete
/// input stays literal; code/link boundaries come from the standards parser.
pub fn parse_inline_markdown_extended(text: &str) -> InlineMarkdownParseResult {
    let unchanged = || InlineMarkdownParseResult {
        spans: vec![InlineSpan::plain(text)], changed: false,
    };
    // A neutral prefix prevents paragraph text such as "# title" from becoming
    // a block shortcut here. Block shortcuts have their own entry point.
    let source = format!("x {text}");
    let syntax = MarkdownSyntax::parse(&source);
    // Keep CDitor's ++underline++ extension, but only in literal text tokens.
    // Masking is byte-length preserving; code, escaped text and URLs are opaque.
    let mut masked = source.clone();
    for node in syntax.nodes() {
        if let SyntaxKind::Text(value) = &node.kind
            && source.get(node.source.clone()) == Some(value.as_str())
        {
            let replacement = value.replace("++", "~~");
            masked.replace_range(node.source.clone(), &replacement);
        }
    }
    let syntax = MarkdownSyntax::parse(&masked);
    let mut stack = vec![(0, Vec::<InlineMark>::new())];
    let mut spans = Vec::new();
    let mut unresolved = false;
    let mut paragraphs = 0;
    let mut removed_prefix = false;
    while let Some((id, mut marks)) = stack.pop() {
        let node = &syntax.nodes()[id];
        let original = &source[node.source.clone()];
        let value = match &node.kind {
            SyntaxKind::Document => None,
            SyntaxKind::Block(TagEnd::Paragraph) => { paragraphs += 1; None },
            SyntaxKind::Strong => { marks.push(InlineMark::Bold); None },
            SyntaxKind::Emphasis => { marks.push(InlineMark::Italic); None },
            SyntaxKind::Strike => {
                let underline = original.starts_with("++");
                if underline != original.ends_with("++") { return unchanged(); }
                marks.push(if underline { InlineMark::Underline } else { InlineMark::Strike });
                None
            },
            SyntaxKind::Link { destination, .. } => {
                marks.push(InlineMark::Link { href: destination.clone() }); None
            },
            SyntaxKind::Code(value) => { marks.push(InlineMark::Code); Some(value.clone()) },
            SyntaxKind::Text(value) => {
                let literal = if original.contains("++") { original } else { value };
                // Escaped punctuation is not an unfinished shortcut. Only raw,
                // unchanged text tokens participate in this conservative gate.
                if original == literal && !escaped_at(&source, node.source.start) {
                    unresolved |= literal.contains(['*', '_', '~']) || literal.contains("++");
                }
                Some(literal.to_owned())
            },
            SyntaxKind::SoftBreak => Some("\n".to_owned()),
            // Unsupported objects/HTML must not be silently consumed by typing.
            _ => return unchanged(),
        };
        if let Some(mut value) = value {
            if !removed_prefix {
                let Some(rest) = value.strip_prefix("x ") else { return unchanged(); };
                value = rest.to_owned();
                removed_prefix = true;
            }
            if !value.is_empty() {
                append_with_auto_links(&mut spans, &value, &marks);
            }
        }
        for &child in node.children.iter().rev() { stack.push((child, marks.clone())); }
    }
    if unresolved || paragraphs != 1 { return unchanged(); }
    // pulldown-cmark omits trailing paragraph whitespace; shortcuts must not.
    let trailing = text.len() - text.trim_end_matches([' ', '\t']).len();
    if trailing > 0 {
        push_span(&mut spans, &text[text.len() - trailing..], &[]);
    }
    let changed = spans.iter().any(|span| !span.marks.is_empty());
    if !changed { return unchanged(); }
    InlineMarkdownParseResult { spans, changed }
}

fn escaped_at(source: &str, offset: usize) -> bool {
    source.as_bytes()[..offset].iter().rev().take_while(|&&b| b == b'\\').count() % 2 == 1
}

fn push_span(spans: &mut Vec<InlineSpan>, text: &str, marks: &[InlineMark]) {
    if text.is_empty() { return; }
    let marks = canonical_marks(marks);
    if let Some(last) = spans.last_mut().filter(|last| last.marks == marks) {
        last.text.push_str(text);
    } else {
        spans.push(InlineSpan { text: text.to_owned(), marks });
    }
}

fn canonical_marks(marks: &[InlineMark]) -> Vec<InlineMark> {
    let mut output = marks.to_vec();
    output.sort_by_key(|mark| match mark {
        InlineMark::Bold => 0,
        InlineMark::Italic => 1,
        InlineMark::Underline => 2,
        InlineMark::Strike => 3,
        InlineMark::Code => 4,
        InlineMark::Link { .. } | InlineMark::DocumentLink { .. } => 5,
        InlineMark::Color(_) => 6,
        InlineMark::Background(_) => 7,
    });
    output
}

fn append_with_auto_links(spans: &mut Vec<InlineSpan>, text: &str, marks: &[InlineMark]) {
    if marks.iter().any(|mark| matches!(mark, InlineMark::Code | InlineMark::Link { .. })) {
        push_span(spans, text, marks);
        return;
    }
    let mut position = 0;
    let mut plain_start = 0;
    while position < text.len() {
        if (position == 0 || text[..position].chars().next_back().is_some_and(char::is_whitespace))
            && let Some((url, len)) = parse_auto_link(&text[position..])
        {
            push_span(spans, &text[plain_start..position], marks);
            let mut linked = marks.to_vec();
            linked.push(InlineMark::Link { href: url.to_owned() });
            push_span(spans, url, &linked);
            position += len;
            plain_start = position;
        } else {
            position += text[position..].chars().next().unwrap().len_utf8();
        }
    }
    push_span(spans, &text[plain_start..], marks);
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
    for ch in text[prefix_len..].chars() {
        if ch.is_whitespace() || matches!(ch, '<' | '>' | '[' | ']' | '(' | ')' | '"') {
            break;
        }
        end += ch.len_utf8();
    }
    // Strip trailing punctuation.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> InlineMarkdownParseResult {
        parse_inline_markdown_extended(text)
    }

    #[test]
    fn bold_basic() {
        let result = parse("**bold**");
        assert!(result.changed);
        assert_eq!(result.spans.len(), 1);
        assert_eq!(result.spans[0].text, "bold");
        assert_eq!(result.spans[0].marks, vec![InlineMark::Bold]);
    }

    #[test]
    fn italic_basic() {
        let result = parse("*italic*");
        assert!(result.changed);
        assert_eq!(result.spans.len(), 1);
        assert_eq!(result.spans[0].text, "italic");
        assert_eq!(result.spans[0].marks, vec![InlineMark::Italic]);
    }

    #[test]
    fn bold_italic_combined() {
        let result = parse("***both***");
        assert!(result.changed);
        assert_eq!(result.spans.len(), 1);
        assert_eq!(result.spans[0].text, "both");
        assert!(result.spans[0].marks.contains(&InlineMark::Bold));
        assert!(result.spans[0].marks.contains(&InlineMark::Italic));
    }

    #[test]
    fn nested_bold_with_italic() {
        let result = parse("**bold *italic* bold**");
        assert!(result.changed);
        assert_eq!(result.spans.len(), 3);
        assert_eq!(result.spans[0].text, "bold ");
        assert_eq!(result.spans[0].marks, vec![InlineMark::Bold]);
        assert_eq!(result.spans[1].text, "italic");
        assert!(result.spans[1].marks.contains(&InlineMark::Bold));
        assert!(result.spans[1].marks.contains(&InlineMark::Italic));
        assert_eq!(result.spans[2].text, " bold");
        assert_eq!(result.spans[2].marks, vec![InlineMark::Bold]);
    }

    #[test]
    fn unclosed_bold_does_not_trigger() {
        let result = parse("**bold");
        assert!(!result.changed);
    }

    #[test]
    fn partial_bold_does_not_trigger_italic() {
        let result = parse("**bold*");
        assert!(!result.changed);
    }

    #[test]
    fn multiple_marks_in_one_line() {
        let result = parse("hello **bold** and *italic* world");
        assert!(result.changed);
        assert_eq!(result.spans.len(), 5);
        assert_eq!(result.spans[0].text, "hello ");
        assert!(result.spans[0].marks.is_empty());
        assert_eq!(result.spans[1].text, "bold");
        assert_eq!(result.spans[1].marks, vec![InlineMark::Bold]);
        assert_eq!(result.spans[2].text, " and ");
        assert!(result.spans[2].marks.is_empty());
        assert_eq!(result.spans[3].text, "italic");
        assert_eq!(result.spans[3].marks, vec![InlineMark::Italic]);
        assert_eq!(result.spans[4].text, " world");
        assert!(result.spans[4].marks.is_empty());
    }

    #[test]
    fn code_span_blocks_inner_marks() {
        let result = parse("`code **not bold** code`");
        assert!(result.changed);
        assert_eq!(result.spans.len(), 1);
        assert_eq!(result.spans[0].text, "code **not bold** code");
        assert_eq!(result.spans[0].marks, vec![InlineMark::Code]);
    }

    #[test]
    fn link_basic() {
        let result = parse("[click](https://example.com)");
        assert!(result.changed);
        assert_eq!(result.spans.len(), 1);
        assert_eq!(result.spans[0].text, "click");
        assert!(
            matches!(&result.spans[0].marks[..], [InlineMark::Link { href }] if href == "https://example.com")
        );
    }

    #[test]
    fn strikethrough() {
        let result = parse("~~deleted~~");
        assert!(result.changed);
        assert_eq!(result.spans.len(), 1);
        assert_eq!(result.spans[0].text, "deleted");
        assert_eq!(result.spans[0].marks, vec![InlineMark::Strike]);
    }

    #[test]
    fn underline() {
        let result = parse("++underlined++");
        assert!(result.changed);
        assert_eq!(result.spans.len(), 1);
        assert_eq!(result.spans[0].text, "underlined");
        assert_eq!(result.spans[0].marks, vec![InlineMark::Underline]);
    }

    #[test]
    fn plain_text_unchanged() {
        let result = parse("hello world");
        assert!(!result.changed);
        assert_eq!(result.spans.len(), 1);
        assert_eq!(result.spans[0].text, "hello world");
    }

    #[test]
    fn auto_link() {
        let result = parse("visit https://zed.dev today");
        assert!(result.changed);
        assert_eq!(result.spans.len(), 3);
        assert_eq!(result.spans[0].text, "visit ");
        assert_eq!(result.spans[1].text, "https://zed.dev");
        assert!(
            matches!(&result.spans[1].marks[..], [InlineMark::Link { href }] if href == "https://zed.dev")
        );
        assert_eq!(result.spans[2].text, " today");
    }

    #[test]
    fn adjacent_bold_and_strike_no_space() {
        let result = parse("**asd**~~ad~~");
        assert!(result.changed);
        assert_eq!(result.spans.len(), 2);
        assert_eq!(result.spans[0].text, "asd");
        assert_eq!(result.spans[0].marks, vec![InlineMark::Bold]);
        assert_eq!(result.spans[1].text, "ad");
        assert_eq!(result.spans[1].marks, vec![InlineMark::Strike]);
    }

    #[test]
    fn cjk_with_strike_and_trailing_text() {
        let result = parse("埃塞~~asd~~asd");
        assert!(result.changed);
        assert_eq!(result.spans.len(), 3);
        assert_eq!(result.spans[0].text, "埃塞");
        assert!(result.spans[0].marks.is_empty());
        assert_eq!(result.spans[1].text, "asd");
        assert_eq!(result.spans[1].marks, vec![InlineMark::Strike]);
        assert_eq!(result.spans[2].text, "asd");
        assert!(result.spans[2].marks.is_empty());
    }
}
