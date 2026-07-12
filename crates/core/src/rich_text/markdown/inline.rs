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
    // Phase 1: Find atomic spans (links, images, code) that block delimiter parsing inside them.
    let atomics = find_atomic_spans(text);

    // Phase 2: Find all balanced delimiter pairs (longest-first, properly nested).
    let pairs = find_delimiter_pairs(text, &atomics);

    // Phase 3: Check if there are unclosed delimiters — if so, don't trigger shortcut.
    let has_unclosed = has_unclosed_delimiters(text, &pairs, &atomics);

    if pairs.is_empty() && atomics.is_empty() {
        return InlineParseResult {
            spans: vec![InlineSpan::plain(text.to_string())],
            changed: false,
        };
    }

    // changed=true when we have meaningful marks AND no unclosed delimiters.
    // Atomics (links, code) always count as "changed" since they transform syntax.
    let has_marks = !pairs.is_empty() || atomics.iter().any(|a| a.kind != AtomicKind::Image);
    let changed = has_marks && !has_unclosed;

    // Phase 4: Build spans from resolved pairs.
    let spans = build_spans(text, &pairs, &atomics);

    InlineParseResult { spans, changed }
}

// --- Data structures ---

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AtomicKind {
    Code,
    Link,
    AutoLink,
    Image,
}

#[derive(Debug, Clone)]
struct AtomicSpan {
    start: usize,
    end: usize,
    kind: AtomicKind,
    // For links: label and href
    label: String,
    href: String,
}

#[derive(Debug, Clone)]
struct DelimiterPair {
    open_start: usize,
    open_end: usize,
    close_start: usize,
    close_end: usize,
    marks: Vec<InlineMark>,
}

// --- Phase 1: Find atomic (non-nestable) spans ---

fn find_atomic_spans(text: &str) -> Vec<AtomicSpan> {
    let mut atomics = Vec::new();
    let mut cursor = 0;

    while cursor < text.len() {
        let rest = &text[cursor..];

        // Double backtick code: ``text``
        if rest.starts_with("``") {
            if let Some(end) = rest[2..].find("``") {
                atomics.push(AtomicSpan {
                    start: cursor,
                    end: cursor + 2 + end + 2,
                    kind: AtomicKind::Code,
                    label: rest[2..2 + end].to_string(),
                    href: String::new(),
                });
                cursor += 2 + end + 2;
                continue;
            }
        }

        // Single backtick code: `text`
        if rest.starts_with('`') {
            if let Some(end) = rest[1..].find('`') {
                if end > 0 {
                    atomics.push(AtomicSpan {
                        start: cursor,
                        end: cursor + 1 + end + 1,
                        kind: AtomicKind::Code,
                        label: rest[1..1 + end].to_string(),
                        href: String::new(),
                    });
                    cursor += 1 + end + 1;
                    continue;
                }
            }
        }

        // Image: ![alt](src) — consumed as plain text, not a mark
        if rest.starts_with("![") {
            if let Some(consumed) = parse_markdown_image(rest) {
                atomics.push(AtomicSpan {
                    start: cursor,
                    end: cursor + consumed,
                    kind: AtomicKind::Image,
                    label: String::new(),
                    href: String::new(),
                });
                cursor += consumed;
                continue;
            }
        }

        // Link: [label](href)
        if rest.starts_with('[') {
            if let Some((label, href, consumed)) = parse_markdown_link(rest) {
                atomics.push(AtomicSpan {
                    start: cursor,
                    end: cursor + consumed,
                    kind: AtomicKind::Link,
                    label: label.to_string(),
                    href: href.to_string(),
                });
                cursor += consumed;
                continue;
            }
        }

        // Auto-link: https://... or http://...
        if rest.starts_with("https://") || rest.starts_with("http://") {
            if let Some((url, consumed)) = parse_auto_link(rest) {
                atomics.push(AtomicSpan {
                    start: cursor,
                    end: cursor + consumed,
                    kind: AtomicKind::AutoLink,
                    label: url.to_string(),
                    href: url.to_string(),
                });
                cursor += consumed;
                continue;
            }
        }

        cursor += rest.chars().next().map_or(1, char::len_utf8);
    }

    atomics
}

// --- Phase 2: Find delimiter pairs ---

/// Delimiter types ordered by priority (longest first).
const DELIMITER_TABLE: &[(&str, &[InlineMark])] = &[
    ("***", &[InlineMark::Bold, InlineMark::Italic]),
    ("___", &[InlineMark::Bold, InlineMark::Italic]),
    ("**", &[InlineMark::Bold]),
    ("~~", &[InlineMark::Strike]),
    ("++", &[InlineMark::Underline]),
    ("*", &[InlineMark::Italic]),
    ("_", &[InlineMark::Italic]),
];

fn find_delimiter_pairs(text: &str, atomics: &[AtomicSpan]) -> Vec<DelimiterPair> {
    let mut pairs = Vec::new();
    // Track which byte positions are already claimed by a delimiter (open or close).
    let mut claimed = vec![false; text.len()];

    // Mark atomic spans as claimed so delimiters inside them are ignored.
    for atomic in atomics {
        for i in atomic.start..atomic.end {
            claimed[i] = true;
        }
    }

    // Process delimiters longest-first to give priority to `**` over `*`.
    for &(delimiter, marks) in DELIMITER_TABLE {
        let dlen = delimiter.len();
        let mut cursor = 0;

        while cursor + dlen <= text.len() {
            // Ensure we're at a char boundary.
            if !text.is_char_boundary(cursor) {
                cursor += 1;
                continue;
            }

            // Skip if any byte in this position is claimed.
            if claimed[cursor..cursor + dlen].iter().any(|&c| c) {
                cursor += 1;
                continue;
            }

            // Check if this position matches the delimiter.
            if !text[cursor..].starts_with(delimiter) {
                cursor += 1;
                continue;
            }

            // For single-char delimiters, don't match if preceded or followed by the same char
            // (would indicate an unclosed longer delimiter like `**`).
            if dlen == 1 {
                let d = delimiter.as_bytes()[0];
                if cursor > 0 && text.as_bytes()[cursor - 1] == d {
                    cursor += 1;
                    continue;
                }
                if cursor + 1 < text.len() && text.as_bytes()[cursor + 1] == d {
                    cursor += 1;
                    continue;
                }
            }

            // Found a potential opener. Now find the closer.
            let open_start = cursor;
            let open_end = cursor + dlen;
            let mut search = open_end;

            let mut found_closer = false;
            while search + dlen <= text.len() {
                if !text.is_char_boundary(search) {
                    search += 1;
                    continue;
                }
                if claimed[search..search + dlen].iter().any(|&c| c) {
                    search += 1;
                    continue;
                }
                if !text[search..].starts_with(delimiter) {
                    search += 1;
                    continue;
                }
                // For single-char delimiters, don't match closer if followed by same char.
                if dlen == 1
                    && search + dlen < text.len()
                    && text.as_bytes()[search + dlen] == delimiter.as_bytes()[0]
                {
                    search += 1;
                    continue;
                }

                // Ensure content between opener and closer is non-empty.
                if search == open_end {
                    search += 1;
                    continue;
                }

                // Found a valid closer.
                let close_start = search;
                let close_end = search + dlen;

                // Claim the opener and closer bytes.
                for i in open_start..open_end {
                    claimed[i] = true;
                }
                for i in close_start..close_end {
                    claimed[i] = true;
                }

                pairs.push(DelimiterPair {
                    open_start,
                    open_end,
                    close_start,
                    close_end,
                    marks: marks.to_vec(),
                });
                found_closer = true;
                cursor = close_end;
                break;
            }

            if !found_closer {
                // No closer found, skip this opener.
                cursor = open_end;
            }
        }
    }

    // Sort pairs by open_start for span building.
    pairs.sort_by_key(|p| p.open_start);
    pairs
}

// --- Phase 3: Check for unclosed delimiters ---

fn has_unclosed_delimiters(text: &str, pairs: &[DelimiterPair], atomics: &[AtomicSpan]) -> bool {
    // Build a set of all positions covered by pairs or atomics.
    let mut covered = vec![false; text.len()];
    for pair in pairs {
        for i in pair.open_start..pair.close_end {
            covered[i] = true;
        }
    }
    for atomic in atomics {
        for i in atomic.start..atomic.end {
            covered[i] = true;
        }
    }

    // Scan for any delimiter characters in uncovered regions.
    let mut cursor = 0;
    while cursor < text.len() {
        if !text.is_char_boundary(cursor) || covered[cursor] {
            cursor += 1;
            continue;
        }
        let rest = &text[cursor..];
        for &(delimiter, _) in DELIMITER_TABLE {
            if rest.starts_with(delimiter) {
                return true;
            }
        }
        cursor += rest.chars().next().map_or(1, char::len_utf8);
    }
    false
}

// --- Phase 4: Build spans ---

fn build_spans(text: &str, pairs: &[DelimiterPair], atomics: &[AtomicSpan]) -> Vec<InlineSpan> {
    let mut spans = Vec::new();
    let mut cursor = 0;

    // Merge events: we need to walk through text and at each position know
    // which marks are active (from surrounding pairs).
    while cursor < text.len() {
        // Check if cursor is at an atomic span start.
        if let Some(atomic) = atomics.iter().find(|a| a.start == cursor) {
            let marks_at = active_marks_at(cursor, pairs);
            match atomic.kind {
                AtomicKind::Code => {
                    spans.push(InlineSpan {
                        text: atomic.label.clone(),
                        marks: vec![InlineMark::Code],
                    });
                }
                AtomicKind::Link => {
                    let mut marks = marks_at;
                    marks.push(InlineMark::Link {
                        href: atomic.href.clone(),
                    });
                    spans.push(InlineSpan {
                        text: atomic.label.clone(),
                        marks,
                    });
                }
                AtomicKind::AutoLink => {
                    let mut marks = marks_at;
                    marks.push(InlineMark::Link {
                        href: atomic.href.clone(),
                    });
                    spans.push(InlineSpan {
                        text: atomic.label.clone(),
                        marks,
                    });
                }
                AtomicKind::Image => {
                    // Images are kept as plain text in the span model.
                    spans.push(InlineSpan {
                        text: text[atomic.start..atomic.end].to_string(),
                        marks: marks_at,
                    });
                }
            }
            cursor = atomic.end;
            continue;
        }

        // Check if cursor is at a delimiter boundary (opener or closer) — skip it.
        if let Some(pair) = pairs.iter().find(|p| p.open_start == cursor) {
            cursor = pair.open_end;
            continue;
        }
        if let Some(pair) = pairs.iter().find(|p| p.close_start == cursor) {
            cursor = pair.close_end;
            continue;
        }

        // Regular character: collect a run with the same marks.
        let marks = active_marks_at(cursor, pairs);
        let run_start = cursor;
        cursor += text[cursor..].chars().next().map_or(1, char::len_utf8);

        // Extend run while marks remain the same and we don't hit a boundary.
        while cursor < text.len() {
            if atomics.iter().any(|a| a.start == cursor) {
                break;
            }
            if pairs
                .iter()
                .any(|p| p.open_start == cursor || p.close_start == cursor)
            {
                break;
            }
            let next_marks = active_marks_at(cursor, pairs);
            if next_marks != marks {
                break;
            }
            cursor += text[cursor..].chars().next().map_or(1, char::len_utf8);
        }

        spans.push(InlineSpan {
            text: text[run_start..cursor].to_string(),
            marks,
        });
    }

    if spans.is_empty() {
        spans.push(InlineSpan::plain(text.to_string()));
    }

    merge_inline_spans(&mut spans);
    spans
}

/// Compute which marks are active at a given byte position (inside which pairs).
fn active_marks_at(pos: usize, pairs: &[DelimiterPair]) -> Vec<InlineMark> {
    let mut marks = Vec::new();
    for pair in pairs {
        if pos >= pair.open_end && pos < pair.close_start {
            for mark in &pair.marks {
                if !marks.contains(mark) {
                    marks.push(mark.clone());
                }
            }
        }
    }
    marks
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

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> InlineParseResult {
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
