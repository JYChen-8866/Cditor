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
}
