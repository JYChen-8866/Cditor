/// Block input capability classification
///
/// Defines what kind of direct user input a block can accept.
/// This determines how focus, keyboard events, and editing commands
/// should be routed when a block is selected.
use crate::rich_text::RichBlockKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockInputCapability {
    /// Block accepts direct text input with inline formatting
    Text(TextInputCapability),

    /// Block manages its own cell-based input (table)
    TableCell,

    /// Block has its own complex interactive editor (whiteboard, embed)
    ComplexBlock,

    /// Block is atomic and does not accept direct text editing
    /// Examples: image, file, divider
    Atomic,

    /// Block cannot accept any input
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextInputCapability {
    /// Full rich text with inline formatting
    Rich,

    /// Plain text only (code blocks)
    Plain,

    /// Markdown source (raw markdown blocks)
    Markdown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnterKeyBehavior {
    SplitText,
    InsertSoftBreak,
    TableCellSoftBreak,
    InsertParagraphAfter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabKeyBehavior {
    InsertSoftTab,
    ReparentSubtree,
    InnerSurface,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockKeyboardPolicy {
    pub enter: EnterKeyBehavior,
    pub tab: TabKeyBehavior,
}

impl BlockKeyboardPolicy {
    pub fn for_kind(kind: &RichBlockKind) -> Self {
        let enter = match kind {
            RichBlockKind::Quote
            | RichBlockKind::Callout { .. }
            | RichBlockKind::Code { .. }
            | RichBlockKind::Html
            | RichBlockKind::RawMarkdown
            | RichBlockKind::Mermaid => EnterKeyBehavior::InsertSoftBreak,
            RichBlockKind::Table => EnterKeyBehavior::TableCellSoftBreak,
            RichBlockKind::Whiteboard
            | RichBlockKind::MindMap
            | RichBlockKind::Image
            | RichBlockKind::File
            | RichBlockKind::Attachment
            | RichBlockKind::Divider
            | RichBlockKind::Separator
            | RichBlockKind::Embed
            | RichBlockKind::Database
            | RichBlockKind::Math
            | RichBlockKind::ColumnsGroup
            | RichBlockKind::Column
            | RichBlockKind::Custom(_) => EnterKeyBehavior::InsertParagraphAfter,
            _ => EnterKeyBehavior::SplitText,
        };
        let tab = match kind {
            RichBlockKind::Code { .. }
            | RichBlockKind::RawMarkdown
            | RichBlockKind::Quote
            | RichBlockKind::Callout { .. } => TabKeyBehavior::InsertSoftTab,
            RichBlockKind::Table => TabKeyBehavior::InnerSurface,
            _ => TabKeyBehavior::ReparentSubtree,
        };
        Self { enter, tab }
    }
}

impl BlockInputCapability {
    /// Returns the input capability for a given block kind
    pub fn for_kind(kind: &RichBlockKind) -> Self {
        match kind {
            RichBlockKind::Paragraph
            | RichBlockKind::Heading { .. }
            | RichBlockKind::Quote
            | RichBlockKind::Callout { .. }
            | RichBlockKind::Todo { .. }
            | RichBlockKind::BulletedList
            | RichBlockKind::NumberedList
            | RichBlockKind::Toggle
            | RichBlockKind::FootnoteDefinition
            | RichBlockKind::Comment => BlockInputCapability::Text(TextInputCapability::Rich),

            RichBlockKind::Code { .. } | RichBlockKind::Html => {
                BlockInputCapability::Text(TextInputCapability::Plain)
            }

            RichBlockKind::RawMarkdown | RichBlockKind::Mermaid => {
                BlockInputCapability::Text(TextInputCapability::Markdown)
            }

            RichBlockKind::Table => BlockInputCapability::TableCell,

            RichBlockKind::Whiteboard | RichBlockKind::MindMap => {
                BlockInputCapability::ComplexBlock
            }

            RichBlockKind::Image
            | RichBlockKind::File
            | RichBlockKind::Attachment
            | RichBlockKind::Divider
            | RichBlockKind::Separator => BlockInputCapability::Atomic,

            RichBlockKind::Embed
            | RichBlockKind::Database
            | RichBlockKind::Math
            | RichBlockKind::Custom(_) => BlockInputCapability::ComplexBlock,

            RichBlockKind::ColumnsGroup | RichBlockKind::Column => BlockInputCapability::None,
        }
    }

    /// Returns true if this block can accept text caret positioning
    pub fn accepts_text_caret(&self) -> bool {
        matches!(self, BlockInputCapability::Text(_))
    }

    /// Returns true if Enter should split this block
    pub fn supports_enter_split(&self) -> bool {
        matches!(self, BlockInputCapability::Text(TextInputCapability::Rich))
    }

    /// Returns true if this block should handle Enter internally
    /// (insert soft line break instead of splitting the block)
    pub fn handles_enter_internally(&self) -> bool {
        matches!(
            self,
            BlockInputCapability::Text(TextInputCapability::Plain | TextInputCapability::Markdown)
                | BlockInputCapability::TableCell
        )
    }

    /// Returns true if this is a Quote or Callout block that inserts soft line breaks
    pub fn is_quote_like(&self) -> bool {
        // Quote and Callout are Rich text but insert soft line breaks on Enter
        false // This will be checked by block kind in handle_enter
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mermaid_is_markdown_source_not_an_opaque_complex_payload() {
        let capability = BlockInputCapability::for_kind(&RichBlockKind::Mermaid);

        assert_eq!(
            capability,
            BlockInputCapability::Text(TextInputCapability::Markdown)
        );
        assert!(capability.accepts_text_caret());
        assert!(capability.handles_enter_internally());
        assert!(!capability.supports_enter_split());
    }

    #[test]
    fn keyboard_policy_covers_text_source_table_atomic_and_complex_families() {
        use EnterKeyBehavior::*;
        use TabKeyBehavior::*;

        let cases = [
            (RichBlockKind::Paragraph, SplitText, ReparentSubtree),
            (RichBlockKind::Quote, InsertSoftBreak, InsertSoftTab),
            (
                RichBlockKind::Code { language: None },
                InsertSoftBreak,
                InsertSoftTab,
            ),
            (RichBlockKind::Html, InsertSoftBreak, ReparentSubtree),
            (RichBlockKind::Mermaid, InsertSoftBreak, ReparentSubtree),
            (RichBlockKind::Table, TableCellSoftBreak, InnerSurface),
            (RichBlockKind::Image, InsertParagraphAfter, ReparentSubtree),
            (
                RichBlockKind::Whiteboard,
                InsertParagraphAfter,
                ReparentSubtree,
            ),
            (
                RichBlockKind::Custom("plugin".to_owned()),
                InsertParagraphAfter,
                ReparentSubtree,
            ),
        ];
        for (kind, enter, tab) in cases {
            assert_eq!(
                BlockKeyboardPolicy::for_kind(&kind),
                BlockKeyboardPolicy { enter, tab }
            );
        }
    }
}
