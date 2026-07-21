use super::*;

pub(super) fn block_kind_for_marker(marker: &str) -> Option<RichBlockKind> {
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

pub(super) fn looks_like_single_block_markdown(line: &str) -> bool {
    line == "---"
        || line == "***"
        || line == "___"
        || parse_heading(line).is_some()
        || line.starts_with("> ")
        || line.starts_with("- [ ] ")
        || line.starts_with("- [x] ")
        || line.starts_with("- [X] ")
        || line.starts_with("- ")
        || line.starts_with("* ")
        || line.starts_with("+ ")
        || parse_numbered_item(line).is_some()
}

pub(super) fn parse_heading(line: &str) -> Option<(u8, &str)> {
    let level = line.bytes().take_while(|byte| *byte == b'#').count();
    if !(1..=6).contains(&level) {
        return None;
    }
    let text = line[level..].strip_prefix(' ')?;
    Some((level as u8, text))
}

pub(super) fn parse_numbered_item(line: &str) -> Option<&str> {
    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    line[digits..].strip_prefix(". ")
}

pub(super) fn parse_fence_start(line: &str) -> Option<(Option<String>, &str)> {
    let trimmed = line.trim_start();
    let rest = trimmed.strip_prefix("```")?;
    let language = rest.trim();
    Some(((!language.is_empty()).then(|| language.to_string()), ""))
}
