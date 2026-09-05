use super::table::table_to_plain_markdown;
use cditor_core::rich_text::{
    BlockPayload, CalloutVariant, InlineMark, InlineSpan, RichBlockKind, RichBlockRecord,
};

pub(crate) fn block_to_plain_markdown(block: &RichBlockRecord) -> String {
    block_parts_to_plain_markdown(&block.kind, &block.payload, block.raw_fallback.as_deref())
}

pub(crate) fn block_parts_to_plain_markdown(
    kind: &RichBlockKind,
    payload: &BlockPayload,
    raw_fallback: Option<&str>,
) -> String {
    if let BlockPayload::Video(video) = payload {
        return format!(
            "<video src=\"{}\" controls title=\"{}\"></video>",
            escape_html_attribute(&video.source),
            escape_html_attribute(&video.title),
        );
    }
    let text = match payload {
        BlockPayload::RichText { spans } => spans_to_markdown(spans),
        _ => payload.plain_text(),
    };
    match kind {
        // Markdown has no page-header metadata. Preserve the system document
        // title as a top-level heading while keeping body H1 blocks distinct
        // in the editor model.
        RichBlockKind::DocumentTitle => format!("# {text}"),
        RichBlockKind::Heading { level } => format!("{} {}", "#".repeat(usize::from(*level)), text),
        RichBlockKind::BulletedList => format!("- {text}"),
        RichBlockKind::NumberedList => format!("1. {text}"),
        RichBlockKind::Todo { checked } => {
            format!("- [{}] {text}", if *checked { "x" } else { " " })
        }
        RichBlockKind::Quote => format!("> {text}"),
        RichBlockKind::Callout { variant } => format!(
            "> [{}]\n> {text}",
            match variant {
                CalloutVariant::Note => "!NOTE",
                CalloutVariant::Tip => "!TIP",
                CalloutVariant::Important => "!IMPORTANT",
                CalloutVariant::Warning => "!WARNING",
                CalloutVariant::Caution => "!CAUTION",
                CalloutVariant::Info => "!NOTE",
                CalloutVariant::Success => "!TIP",
                CalloutVariant::Danger => "!WARNING",
            }
        ),
        RichBlockKind::Code { language } => format!(
            "```{}\n{}\n```",
            language.as_deref().unwrap_or_default(),
            text
        ),
        RichBlockKind::Mermaid => format!("```mermaid\n{text}\n```"),
        RichBlockKind::Separator | RichBlockKind::Divider => "---".to_owned(),
        RichBlockKind::Table => table_to_plain_markdown(payload).unwrap_or(text),
        RichBlockKind::RawMarkdown => raw_fallback.map(str::to_owned).unwrap_or(text),
        _ => text,
    }
}

fn escape_html_attribute(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub(crate) fn spans_to_markdown(spans: &[InlineSpan]) -> String {
    spans.iter().map(span_to_markdown).collect()
}

fn span_to_markdown(span: &InlineSpan) -> String {
    let has = |expected: &InlineMark| {
        span.marks
            .iter()
            .any(|mark| std::mem::discriminant(mark) == std::mem::discriminant(expected))
    };
    let is_link = span.marks.iter().any(|mark| {
        matches!(
            mark,
            InlineMark::Link { .. } | InlineMark::DocumentLink { .. }
        )
    });
    let mut text = if has(&InlineMark::Code) {
        let fence = if span.text.contains('`') { "``" } else { "`" };
        format!("{fence}{}{fence}", span.text)
    } else if is_link {
        // Mirrors SiYuan/Lute: link text escapes the Protyle inline markers so
        // the link destination stays unambiguous.
        escape_protyle_markers(&span.text)
    } else {
        // Mirrors SiYuan/Lute: plain text is exported verbatim. CommonMark
        // treats intraword `*`, `_`, `[` and `]` literally, so unconditional
        // escaping would only produce noise like `a\_b` for `a_b`.
        span.text.clone()
    };
    if has(&InlineMark::Bold) {
        text = format!("**{text}**");
    }
    if has(&InlineMark::Italic) {
        // SiYuan/Lute emits emphasis with `*` (not `_`), which also avoids
        // intraword underscore edge cases on re-import.
        text = format!("*{text}*");
    }
    if has(&InlineMark::Strike) {
        text = format!("~~{text}~~");
    }
    if let Some(href) = span.marks.iter().find_map(|mark| match mark {
        InlineMark::Link { href } | InlineMark::DocumentLink { href } => Some(href),
        _ => None,
    }) {
        text = format!("[{text}]({})", escape_markdown_url(href));
    }
    text
}

/// Escapes the Protyle inline markers the same way Lute's
/// `EscapeProtyleMarkers` does for link text: `\ * _ ` ` ~ $ = ^ < >`.
fn escape_protyle_markers(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\\' | '*' | '_' | '`' | '~' | '$' | '=' | '^' | '<' | '>' => {
                escaped.push('\\');
                escaped.push(ch);
            }
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn escape_markdown_url(url: &str) -> String {
    url.replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
}
