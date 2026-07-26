use super::{CalloutVariant, InlineSpan, RichBlockKind, parse_inline_markdown_extended};

pub fn parse_callout_marker(line: &str) -> Option<CalloutVariant> {
    match line.trim() {
        "[!NOTE]" => Some(CalloutVariant::Note),
        "[!TIP]" => Some(CalloutVariant::Tip),
        "[!IMPORTANT]" => Some(CalloutVariant::Important),
        "[!WARNING]" => Some(CalloutVariant::Warning),
        "[!CAUTION]" => Some(CalloutVariant::Caution),
        _ => None,
    }
}

fn block_kind_for_marker(marker: &str) -> Option<RichBlockKind> {
    match marker {
        "#" => Some(RichBlockKind::Heading { level: 1 }),
        "##" => Some(RichBlockKind::Heading { level: 2 }),
        "###" => Some(RichBlockKind::Heading { level: 3 }),
        "####" => Some(RichBlockKind::Heading { level: 4 }),
        "#####" => Some(RichBlockKind::Heading { level: 5 }),
        "######" => Some(RichBlockKind::Heading { level: 6 }),
        "-" | "*" | "+" => Some(RichBlockKind::BulletedList),
        "[ ]" | "- [ ]" => Some(RichBlockKind::Todo { checked: false }),
        "[x]" | "[X]" | "- [x]" | "- [X]" => Some(RichBlockKind::Todo { checked: true }),
        ">" => Some(RichBlockKind::Quote),
        "---" | "***" | "___" => Some(RichBlockKind::Separator),
        _ => marker
            .strip_prefix("> ")
            .and_then(parse_callout_marker)
            .map(|variant| RichBlockKind::Callout { variant })
            .or_else(|| {
                let digits = marker.strip_suffix('.')?;
                (!digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()))
                    .then_some(RichBlockKind::NumberedList)
            }),
    }
}

pub fn block_kind_shortcut_with_marker_len(text: &str) -> Option<(RichBlockKind, usize)> {
    const MARKERS: &[&str] = &[
        "> [!IMPORTANT] ",
        "> [!WARNING] ",
        "> [!CAUTION] ",
        "> [!NOTE] ",
        "> [!TIP] ",
        "###### ",
        "##### ",
        "#### ",
        "### ",
        "## ",
        "# ",
        "- [ ] ",
        "- [x] ",
        "- [X] ",
        "[ ] ",
        "[x] ",
        "[X] ",
        "--- ",
        "*** ",
        "___ ",
        "- ",
        "* ",
        "+ ",
        "> ",
    ];
    MARKERS
        .iter()
        .find_map(|value| {
            text.strip_prefix(value).map(|_| {
                let marker = value.trim_end();
                (
                    block_kind_for_marker(marker).expect("known marker"),
                    value.len(),
                )
            })
        })
        .or_else(|| {
            let digits = text.bytes().take_while(u8::is_ascii_digit).count();
            (digits > 0 && text[digits..].starts_with(". "))
                .then_some((RichBlockKind::NumberedList, digits + 2))
        })
}

pub fn code_fence_shortcut(text: &str) -> Option<RichBlockKind> {
    let language = text
        .strip_prefix("```")
        .or_else(|| text.strip_prefix("···"))?
        .trim();
    if language.contains(char::is_whitespace) {
        return None;
    }
    Some(RichBlockKind::Code {
        language: (!language.is_empty()).then(|| language.to_lowercase()),
    })
}

pub fn looks_like_markdown(text: &str) -> bool {
    text.lines().any(|line| {
        let line = line.trim_start();
        [
            "# ", "## ", "### ", "> ", "- ", "* ", "+ ", "- [ ] ", "- [x] ", "```", "···", "|",
        ]
        .iter()
        .any(|prefix| line.starts_with(prefix))
            || matches!(line, "---" | "***" | "___")
            || numbered_item(line)
            || parse_inline_markdown_extended(line).changed
    })
}

fn numbered_item(line: &str) -> bool {
    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    digits > 0 && line[digits..].starts_with(". ")
}

pub fn markdown_inline_shortcut_spans(text: &str) -> Option<Vec<InlineSpan>> {
    let parsed = parse_inline_markdown_extended(text);
    parsed.changed.then_some(parsed.spans)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_block_inline_and_code_shortcuts() {
        assert_eq!(block_kind_shortcut_with_marker_len("## ").unwrap().1, 3);
        assert!(looks_like_markdown("**bold**"));
        assert_eq!(markdown_inline_shortcut_spans("`x`").unwrap()[0].text, "x");
        assert!(matches!(
            code_fence_shortcut("```rust"),
            Some(RichBlockKind::Code { .. })
        ));
    }
}
