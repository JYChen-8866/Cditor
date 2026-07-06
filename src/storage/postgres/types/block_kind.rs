use super::*;

pub fn rich_block_kind_to_db(kind: &RichBlockKind) -> String {
    match kind {
        RichBlockKind::Paragraph => "paragraph".to_owned(),
        RichBlockKind::Heading { level } => format!("heading:{level}"),
        RichBlockKind::Quote => "quote".to_owned(),
        RichBlockKind::Callout { variant } => {
            format!("callout:{}", callout_variant_to_db(*variant))
        }
        RichBlockKind::Todo { checked } => format!("todo:{checked}"),
        RichBlockKind::BulletedList => "bulleted_list".to_owned(),
        RichBlockKind::NumberedList => "numbered_list".to_owned(),
        RichBlockKind::Toggle => "toggle".to_owned(),
        RichBlockKind::Code { language } => match language {
            Some(language) => format!("code:{language}"),
            None => "code".to_owned(),
        },
        RichBlockKind::Math => "math".to_owned(),
        RichBlockKind::Mermaid => "mermaid".to_owned(),
        RichBlockKind::Html => "html".to_owned(),
        RichBlockKind::Table => "table".to_owned(),
        RichBlockKind::Image => "image".to_owned(),
        RichBlockKind::File => "file".to_owned(),
        RichBlockKind::Attachment => "attachment".to_owned(),
        RichBlockKind::Whiteboard => "whiteboard".to_owned(),
        RichBlockKind::MindMap => "mind_map".to_owned(),
        RichBlockKind::Embed => "embed".to_owned(),
        RichBlockKind::Divider => "divider".to_owned(),
        RichBlockKind::Separator => "separator".to_owned(),
        RichBlockKind::FootnoteDefinition => "footnote_definition".to_owned(),
        RichBlockKind::Comment => "comment".to_owned(),
        RichBlockKind::RawMarkdown => "raw_markdown".to_owned(),
        RichBlockKind::Database => "database".to_owned(),
        RichBlockKind::Custom(name) => format!("custom:{name}"),
    }
}

pub fn rich_block_kind_from_db(value: &str) -> RichBlockKind {
    if let Some(level) = value.strip_prefix("heading:") {
        return RichBlockKind::Heading {
            level: level.parse::<u8>().unwrap_or(1).clamp(1, 6),
        };
    }
    if let Some(variant) = value.strip_prefix("callout:") {
        return RichBlockKind::Callout {
            variant: callout_variant_from_db(variant),
        };
    }
    if let Some(checked) = value.strip_prefix("todo:") {
        return RichBlockKind::Todo {
            checked: checked == "true",
        };
    }
    if let Some(language) = value.strip_prefix("code:") {
        return RichBlockKind::Code {
            language: Some(language.to_owned()),
        };
    }
    if let Some(name) = value.strip_prefix("custom:") {
        return RichBlockKind::Custom(name.to_owned());
    }

    match value {
        "paragraph" => RichBlockKind::Paragraph,
        "quote" => RichBlockKind::Quote,
        "bulleted_list" => RichBlockKind::BulletedList,
        "numbered_list" => RichBlockKind::NumberedList,
        "toggle" => RichBlockKind::Toggle,
        "code" => RichBlockKind::Code { language: None },
        "math" => RichBlockKind::Math,
        "mermaid" => RichBlockKind::Mermaid,
        "html" => RichBlockKind::Html,
        "table" => RichBlockKind::Table,
        "image" => RichBlockKind::Image,
        "file" => RichBlockKind::File,
        "attachment" => RichBlockKind::Attachment,
        "whiteboard" => RichBlockKind::Whiteboard,
        "mind_map" => RichBlockKind::MindMap,
        "embed" => RichBlockKind::Embed,
        "divider" => RichBlockKind::Divider,
        "separator" => RichBlockKind::Separator,
        "footnote_definition" => RichBlockKind::FootnoteDefinition,
        "comment" => RichBlockKind::Comment,
        "raw_markdown" => RichBlockKind::RawMarkdown,
        "database" => RichBlockKind::Database,
        _ => RichBlockKind::Paragraph,
    }
}

fn callout_variant_to_db(variant: CalloutVariant) -> &'static str {
    match variant {
        CalloutVariant::Note => "note",
        CalloutVariant::Tip => "tip",
        CalloutVariant::Important => "important",
        CalloutVariant::Warning => "warning",
        CalloutVariant::Caution => "caution",
        CalloutVariant::Info => "info",
        CalloutVariant::Success => "success",
        CalloutVariant::Danger => "danger",
    }
}

fn callout_variant_from_db(value: &str) -> CalloutVariant {
    match value {
        "tip" => CalloutVariant::Tip,
        "important" => CalloutVariant::Important,
        "warning" => CalloutVariant::Warning,
        "caution" => CalloutVariant::Caution,
        "info" => CalloutVariant::Info,
        "success" => CalloutVariant::Success,
        "danger" => CalloutVariant::Danger,
        _ => CalloutVariant::Note,
    }
}
