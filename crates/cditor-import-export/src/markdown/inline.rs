//! Inline Markdown parsing for import.
//!
//! Parsing is delegated to the vendored velotype parser (see
//! [`super::velotype`]) so emphasis, links, code spans, inline HTML styling,
//! footnotes and math behave exactly like velotype.

use cditor_core::rich_text::{InlineMarkdownParseResult, InlineSpan};

use super::velotype_bridge::parse_inline_markdown_velotype;

pub(super) fn parse_inline_markdown(markdown: &str) -> Vec<InlineSpan> {
    parse_inline_markdown_velotype(markdown)
}

pub(super) fn parse_inline_markdown_extended(text: &str) -> InlineMarkdownParseResult {
    let spans = parse_inline_markdown_velotype(text);
    // `changed` mirrors the old parser: the text only counts as Markdown when
    // the parse actually consumed syntax. Unclosed delimiters stay literal,
    // so they do not trigger a shortcut/paste conversion.
    let changed = spans != vec![InlineSpan::plain(text.to_owned())];
    InlineMarkdownParseResult { spans, changed }
}
