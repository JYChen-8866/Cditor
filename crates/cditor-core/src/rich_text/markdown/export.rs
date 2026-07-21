use super::*;
use crate::rich_text::markdown::table::table_to_plain_markdown;

pub(super) fn block_to_plain_markdown(block: &RichBlockRecord) -> String {
    let text = block.payload.plain_text();
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
