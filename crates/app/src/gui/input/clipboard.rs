use cditor_runtime::{RichTextSelectionSnapshot, TableClipboardSnapshot};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RichClipboardItem {
    pub plain_text: String,
    pub rich_text: Option<RichTextSelectionSnapshot>,
    pub table: Option<TableClipboardSnapshot>,
}

impl RichClipboardItem {
    pub fn plain_text(text: String) -> Self {
        Self {
            plain_text: text,
            rich_text: None,
            table: None,
        }
    }

    pub fn from_rich(rich_text: RichTextSelectionSnapshot) -> Self {
        Self {
            plain_text: rich_text.text.clone(),
            rich_text: Some(rich_text),
            table: None,
        }
    }

    pub fn from_table(table: TableClipboardSnapshot) -> Self {
        Self {
            plain_text: table.markdown.clone(),
            rich_text: None,
            table: Some(table),
        }
    }

    pub fn matches_system_text(&self, text: &str) -> bool {
        self.plain_text == text
            || self
                .table
                .as_ref()
                .is_some_and(|table| table.plain_text == text || table.markdown == text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cditor_core::rich_text::{InlineMark, InlineSpan};

    #[test]
    fn rich_clipboard_item_keeps_plain_text_for_system_clipboard_matching() {
        let item = RichClipboardItem::from_rich(RichTextSelectionSnapshot {
            text: "bold".to_owned(),
            spans: vec![InlineSpan {
                text: "bold".to_owned(),
                marks: vec![InlineMark::Bold],
            }],
        });

        assert!(item.matches_system_text("bold"));
        assert!(!item.matches_system_text("plain"));
        assert!(item.table.is_none());
        assert!(
            item.rich_text
                .as_ref()
                .unwrap()
                .spans
                .iter()
                .any(|span| span.marks.contains(&InlineMark::Bold))
        );
    }
}
