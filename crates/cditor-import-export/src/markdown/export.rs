use super::table::table_to_plain_markdown;
use cditor_core::rich_text::{
    BlockPayload, CalloutVariant, InlineMark, InlineSpan, RichBlockKind, RichBlockRecord,
};

pub(crate) fn block_to_plain_markdown(block: &RichBlockRecord) -> String {
    let text = match &block.payload {
        BlockPayload::RichText { spans } => spans_to_markdown(spans),
        _ => block.payload.plain_text(),
    };
    match &block.kind {
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
        RichBlockKind::Separator | RichBlockKind::Divider => "---".to_owned(),
        RichBlockKind::Table => table_to_plain_markdown(&block.payload).unwrap_or(text),
        RichBlockKind::RawMarkdown => block.raw_fallback.clone().unwrap_or(text),
        _ => text,
    }
}

fn spans_to_markdown(spans: &[InlineSpan]) -> String {
    spans.iter().map(span_to_markdown).collect()
}

fn span_to_markdown(span: &InlineSpan) -> String {
    let has = |expected: &InlineMark| {
        span.marks
            .iter()
            .any(|mark| std::mem::discriminant(mark) == std::mem::discriminant(expected))
    };
    let mut text = if has(&InlineMark::Code) {
        let fence = if span.text.contains('`') { "``" } else { "`" };
        format!("{fence}{}{fence}", span.text)
    } else {
        escape_markdown_text(&span.text)
    };
    if has(&InlineMark::Bold) {
        text = format!("**{text}**");
    }
    if has(&InlineMark::Italic) {
        text = format!("_{text}_");
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

fn escape_markdown_text(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('*', "\\*")
        .replace('_', "\\_")
        .replace('[', "\\[")
        .replace(']', "\\]")
}

fn escape_markdown_url(url: &str) -> String {
    url.replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
}
