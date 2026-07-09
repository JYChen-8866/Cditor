use super::{
    BlockPayload, CalloutVariant, InlineMark, InlineSpan, RichBlockKind, RichBlockRecord,
    RichTextDocument, TableCellPayload, TablePayload, TableRowPayload,
};
use crate::ids::{BlockId, DocumentId};
use crate::rich_text::MARKDOWN_PARSE_STATS;

mod block;
mod export;
mod inline;
mod table;

use block::{
    block_kind_for_marker, looks_like_single_block_markdown, parse_callout_marker,
    parse_fence_start, parse_heading, parse_numbered_item,
};
use export::block_to_plain_markdown;
use inline::{parse_inline_markdown, parse_inline_markdown_extended};
use table::{collect_table_candidate_region, is_table_candidate_line, parse_table_region};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ParsedMarkdownDocument {
    pub root_blocks: Vec<BlockId>,
    pub blocks: Vec<RichBlockRecord>,
}

impl ParsedMarkdownDocument {
    pub fn push_root_block(&mut self, block: RichBlockRecord) -> BlockId {
        let id = block.id;
        self.root_blocks.push(id);
        self.blocks.push(block);
        id
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarkdownImportOptions {
    pub document_id: DocumentId,
    pub first_block_id: BlockId,
}

impl Default for MarkdownImportOptions {
    fn default() -> Self {
        Self {
            document_id: 1,
            first_block_id: 1,
        }
    }
}

#[must_use]
pub fn parse_markdown_document(
    markdown: &str,
    options: MarkdownImportOptions,
) -> ParsedMarkdownDocument {
    MARKDOWN_PARSE_STATS.record_full_parse(markdown.len());
    let mut parser = MarkdownParser::new(options);
    parser.parse_document(markdown)
}

#[must_use]
pub fn import_markdown_block_incremental(
    markdown: &str,
    options: MarkdownImportOptions,
) -> Option<RichBlockRecord> {
    let trimmed = markdown.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut parser = MarkdownParser::new(options);
    if markdown.contains('\n') || markdown.contains('\r') {
        let block = parser.parse_incremental_multiline_block(markdown);
        if block.is_some() {
            MARKDOWN_PARSE_STATS.record_incremental_parse(markdown.len());
        }
        return block;
    }

    looks_like_single_block_markdown(trimmed).then(|| {
        MARKDOWN_PARSE_STATS.record_incremental_parse(markdown.len());
        parser.parse_markdown_line(markdown)
    })
}

#[must_use]
pub fn import_markdown_inline_incremental(markdown: &str) -> Option<Vec<InlineSpan>> {
    if markdown.contains('\n') || markdown.contains('\r') {
        return None;
    }
    Some(parse_inline_markdown(markdown))
}

#[must_use]
pub fn block_kind_shortcut(marker: &str) -> Option<RichBlockKind> {
    block_kind_for_marker(marker)
}

#[must_use]
pub fn block_kind_shortcut_with_marker_len(text: &str) -> Option<(RichBlockKind, usize)> {
    const MARKERS: &[&str] = &[
        "###### ", "##### ", "#### ", "### ", "## ", "# ", "- [ ] ", "- [x] ", "- [X] ", "[ ] ",
        "[x] ", "[X] ", "1. ", "- ", "* ", "> ",
    ];
    MARKERS.iter().find_map(|marker_with_space| {
        text.strip_prefix(marker_with_space).map(|_| {
            let marker = marker_with_space.trim_end();
            (
                block_kind_for_marker(marker).expect("known markdown marker"),
                marker_with_space.len(),
            )
        })
    })
}

#[must_use]
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

#[must_use]
pub fn looks_like_markdown_paste(text: &str) -> bool {
    text.lines().any(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with("# ")
            || trimmed.starts_with("## ")
            || trimmed.starts_with("### ")
            || trimmed.starts_with("> ")
            || trimmed.starts_with("- ")
            || trimmed.starts_with("* ")
            || trimmed.starts_with("- [ ] ")
            || trimmed.starts_with("- [x] ")
            || trimmed.starts_with("- [X] ")
            || trimmed.starts_with("```")
            || trimmed.starts_with("···")
            || trimmed.starts_with('|')
            || trimmed == "---"
            || trimmed == "***"
            || trimmed == "___"
            || parse_numbered_item(trimmed).is_some()
    })
}

#[must_use]
pub fn markdown_inline_shortcut_spans(text: &str) -> Option<Vec<InlineSpan>> {
    let spans = parse_inline_markdown_extended(text);
    spans.changed.then_some(spans.spans)
}

#[must_use]
pub fn export_plain_markdown(document: &RichTextDocument) -> String {
    document
        .blocks
        .iter()
        .map(block_to_plain_markdown)
        .collect::<Vec<_>>()
        .join("\n")
}

struct MarkdownParser {
    document_id: DocumentId,
    next_block_id: BlockId,
}

impl MarkdownParser {
    fn new(options: MarkdownImportOptions) -> Self {
        Self {
            document_id: options.document_id,
            next_block_id: options.first_block_id,
        }
    }

    fn next_id(&mut self) -> BlockId {
        let id = self.next_block_id;
        self.next_block_id = self.next_block_id.saturating_add(1);
        id
    }

    fn new_block(&mut self, kind: RichBlockKind, payload: BlockPayload) -> RichBlockRecord {
        let mut block = RichBlockRecord::new(self.next_id(), kind, payload);
        block.document_id = self.document_id;
        block
    }

    fn rich_text_block(&mut self, kind: RichBlockKind, spans: Vec<InlineSpan>) -> RichBlockRecord {
        self.new_block(kind, BlockPayload::RichText { spans })
    }

    fn parse_document(&mut self, markdown: &str) -> ParsedMarkdownDocument {
        let mut document = ParsedMarkdownDocument::default();
        let lines = markdown.lines().collect::<Vec<_>>();
        let mut index = 0usize;
        let mut list_stack: Vec<(usize, BlockId)> = Vec::new();

        while index < lines.len() {
            let line = lines[index];
            if line.trim().is_empty() {
                list_stack.clear();
                index += 1;
                continue;
            }

            if is_table_candidate_line(line) {
                let region_end = collect_table_candidate_region(&lines, index);
                if region_end > index + 1
                    && let Some(table) = parse_table_region(&lines[index..region_end])
                {
                    list_stack.clear();
                    document.push_root_block(
                        self.new_block(RichBlockKind::Table, BlockPayload::Table(table)),
                    );
                    index = region_end;
                    continue;
                }
            }

            if let Some((language, _)) = parse_fence_start(line) {
                list_stack.clear();
                let mut content = String::new();
                index += 1;
                while index < lines.len() {
                    let code_line = lines[index];
                    if code_line.trim_start().starts_with("```") {
                        index += 1;
                        break;
                    }
                    content.push_str(code_line);
                    content.push('\n');
                    index += 1;
                }
                if content.ends_with('\n') {
                    content.pop();
                }
                document.push_root_block(self.new_block(
                    RichBlockKind::Code {
                        language: language.clone(),
                    },
                    BlockPayload::Code {
                        language,
                        text: content,
                    },
                ));
                continue;
            }

            if let Some((indent, mut block)) = self.parse_list_line(line) {
                push_markdown_list_block(&mut document, &mut list_stack, indent, &mut block);
            } else {
                list_stack.clear();
                let block = self.parse_markdown_line(line);
                document.push_root_block(block);
            }
            index += 1;
        }

        if document.blocks.is_empty() {
            document.push_root_block(self.rich_text_block(
                RichBlockKind::Paragraph,
                vec![InlineSpan::plain(String::new())],
            ));
        }

        document
    }

    fn parse_incremental_multiline_block(&mut self, markdown: &str) -> Option<RichBlockRecord> {
        self.parse_fenced_code_block(markdown)
            .or_else(|| self.parse_incremental_table_block(markdown))
            .or_else(|| self.parse_incremental_quote_or_callout_block(markdown))
    }

    fn parse_fenced_code_block(&mut self, markdown: &str) -> Option<RichBlockRecord> {
        let lines = markdown.lines().collect::<Vec<_>>();
        if lines.len() < 2 {
            return None;
        }
        let (language, _) = parse_fence_start(lines.first()?)?;
        if !lines.last()?.trim_start().starts_with("```") {
            return None;
        }
        Some(self.new_block(
            RichBlockKind::Code {
                language: language.clone(),
            },
            BlockPayload::Code {
                language,
                text: lines[1..lines.len() - 1].join("\n"),
            },
        ))
    }

    fn parse_incremental_table_block(&mut self, markdown: &str) -> Option<RichBlockRecord> {
        let lines = markdown.lines().collect::<Vec<_>>();
        if lines.len() < 2 || !lines.iter().all(|line| is_table_candidate_line(line)) {
            return None;
        }
        let table = parse_table_region(&lines)?;
        Some(self.new_block(RichBlockKind::Table, BlockPayload::Table(table)))
    }

    fn parse_incremental_quote_or_callout_block(
        &mut self,
        markdown: &str,
    ) -> Option<RichBlockRecord> {
        let mut lines = Vec::new();
        for line in markdown.lines() {
            let trimmed = line.trim_start();
            let quote_line = trimmed.strip_prefix('>')?;
            lines.push(
                quote_line
                    .strip_prefix(' ')
                    .unwrap_or(quote_line)
                    .to_string(),
            );
        }
        if lines.is_empty() {
            return None;
        }

        if let Some(variant) = lines.first().and_then(|line| parse_callout_marker(line)) {
            let text = lines.into_iter().skip(1).collect::<Vec<_>>().join("\n");
            return Some(self.rich_text_block(
                RichBlockKind::Callout { variant },
                parse_inline_markdown(&text),
            ));
        }

        Some(self.rich_text_block(
            RichBlockKind::Quote,
            vec![InlineSpan::plain(lines.join("\n"))],
        ))
    }

    fn parse_list_line(&mut self, line: &str) -> Option<(usize, RichBlockRecord)> {
        let indent = line
            .chars()
            .take_while(|ch| *ch == ' ' || *ch == '\t')
            .map(|ch| if ch == '\t' { 4 } else { 1 })
            .sum::<usize>();
        let trimmed = line.trim_start();

        if let Some(text) = trimmed.strip_prefix("- [ ] ") {
            return Some((
                indent,
                self.rich_text_block(
                    RichBlockKind::Todo { checked: false },
                    parse_inline_markdown(text.trim_start()),
                ),
            ));
        }
        if let Some(text) = trimmed
            .strip_prefix("- [x] ")
            .or_else(|| trimmed.strip_prefix("- [X] "))
        {
            return Some((
                indent,
                self.rich_text_block(
                    RichBlockKind::Todo { checked: true },
                    parse_inline_markdown(text.trim_start()),
                ),
            ));
        }
        if let Some(text) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
        {
            return Some((
                indent,
                self.rich_text_block(
                    RichBlockKind::BulletedList,
                    parse_inline_markdown(text.trim_start()),
                ),
            ));
        }
        if let Some(text) = parse_numbered_item(trimmed) {
            return Some((
                indent,
                self.rich_text_block(
                    RichBlockKind::NumberedList,
                    parse_inline_markdown(text.trim_start()),
                ),
            ));
        }

        None
    }

    fn parse_markdown_line(&mut self, line: &str) -> RichBlockRecord {
        let trimmed = line.trim_start();
        if trimmed == "---" || trimmed == "***" || trimmed == "___" {
            return self.new_block(RichBlockKind::Separator, BlockPayload::Empty);
        }

        if let Some((level, text)) = parse_heading(trimmed) {
            return self.rich_text_block(
                RichBlockKind::Heading { level },
                parse_inline_markdown(text),
            );
        }

        if let Some((_, block)) = self.parse_list_line(trimmed) {
            return block;
        }
        if let Some(text) = trimmed.strip_prefix("> ") {
            return self.rich_text_block(RichBlockKind::Quote, parse_inline_markdown(text));
        }

        self.rich_text_block(RichBlockKind::Paragraph, parse_inline_markdown(trimmed))
    }
}

fn push_markdown_list_block(
    document: &mut ParsedMarkdownDocument,
    list_stack: &mut Vec<(usize, BlockId)>,
    indent: usize,
    block: &mut RichBlockRecord,
) {
    while list_stack
        .last()
        .is_some_and(|(stack_indent, _)| *stack_indent >= indent)
    {
        list_stack.pop();
    }

    if let Some((_, parent_id)) = list_stack.last().copied() {
        block.parent_id = Some(parent_id);
        block.depth = list_stack.len() as u16;
        if let Some(parent) = document
            .blocks
            .iter_mut()
            .find(|block| block.id == parent_id)
        {
            parent.children.push(block.id);
        } else {
            document.root_blocks.push(block.id);
        }
    } else {
        block.depth = 0;
        document.root_blocks.push(block.id);
    }

    let block_id = block.id;
    document.blocks.push(block.clone());
    list_stack.push((indent, block_id));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_markdown_parse_stats_like_v1() {
        MARKDOWN_PARSE_STATS.reset();

        let _ = parse_markdown_document("# Title", MarkdownImportOptions::default());
        let _ = import_markdown_block_incremental("- item", MarkdownImportOptions::default());

        let snapshot = MARKDOWN_PARSE_STATS.snapshot();
        assert!(snapshot.full_parse_count >= 1);
        assert!(snapshot.full_parse_chars >= "# Title".len() as u64);
        assert!(snapshot.incremental_parse_count >= 1);
        assert!(snapshot.incremental_parse_chars >= "- item".len() as u64);
    }

    #[test]
    fn block_shortcuts_match_editor2_markers() {
        assert_eq!(
            block_kind_shortcut("#"),
            Some(RichBlockKind::Heading { level: 1 })
        );
        assert_eq!(
            block_kind_shortcut("[x]"),
            Some(RichBlockKind::Todo { checked: true })
        );
        assert_eq!(
            block_kind_shortcut("- [ ]"),
            Some(RichBlockKind::Todo { checked: false })
        );
        assert_eq!(
            block_kind_shortcut_with_marker_len("- [x] done"),
            Some((RichBlockKind::Todo { checked: true }, 6))
        );
        assert_eq!(
            block_kind_shortcut_with_marker_len("## title"),
            Some((RichBlockKind::Heading { level: 2 }, 3))
        );
        assert_eq!(
            code_fence_shortcut("```Rust"),
            Some(RichBlockKind::Code {
                language: Some("rust".to_owned())
            })
        );
    }

    #[test]
    fn inline_shortcuts_parse_bold_code_and_link() {
        let spans =
            markdown_inline_shortcut_spans("hello **bold** and `code` plus [zed](https://zed.dev)")
                .expect("inline markdown should parse");
        assert!(
            spans
                .iter()
                .any(|span| span.marks.contains(&InlineMark::Bold) && span.text == "bold")
        );
        assert!(
            spans
                .iter()
                .any(|span| span.marks.contains(&InlineMark::Code) && span.text == "code")
        );
        assert!(spans.iter().any(|span| matches!(span.marks.as_slice(), [InlineMark::Link { href }] if href == "https://zed.dev")));
    }

    #[test]
    fn parses_markdown_document_blocks_tables_and_code() {
        let parsed = parse_markdown_document(
            "# Title\n- item\n  - child\n\n| A | B |\n|---|---|\n| 1 | 2 |\n\n```rust\nfn main() {}\n```",
            MarkdownImportOptions::default(),
        );
        assert!(matches!(
            parsed.blocks[0].kind,
            RichBlockKind::Heading { level: 1 }
        ));
        assert!(
            parsed
                .blocks
                .iter()
                .any(|block| matches!(block.kind, RichBlockKind::Table))
        );
        assert!(
            parsed
                .blocks
                .iter()
                .any(|block| matches!(block.kind, RichBlockKind::Code { .. }))
        );
        let child = parsed
            .blocks
            .iter()
            .find(|block| block.depth == 1)
            .expect("nested list child");
        assert!(child.parent_id.is_some());
    }

    #[test]
    fn export_plain_markdown_matches_v1_basic_boundary() {
        let mut document = RichTextDocument::empty(1);
        document.push_root_block(RichBlockRecord::heading(1, 2, "Title"));
        document.push_root_block(RichBlockRecord::bulleted_list(2, "item"));
        document.push_root_block(RichBlockRecord::todo(3, true, "done"));
        document.push_root_block(RichBlockRecord::code_block(
            4,
            Some("rust".to_owned()),
            "fn main() {}",
        ));

        assert_eq!(
            export_plain_markdown(&document),
            "## Title\n- item\n- [x] done\n```rust\nfn main() {}\n```"
        );
    }

    #[test]
    fn table_cells_support_escaped_pipe_like_v1() {
        let parsed = parse_markdown_document(
            "| A | B |\n|---|---|\n| left \\| right | ok |",
            MarkdownImportOptions::default(),
        );
        let table = parsed
            .blocks
            .iter()
            .find_map(|block| match &block.payload {
                BlockPayload::Table(table) => Some(table),
                _ => None,
            })
            .expect("table should parse");
        assert_eq!(table.rows.len(), 2);
        assert_eq!(table.rows[1].cells.len(), 2);
        assert_eq!(
            crate::rich_text::plain_text_from_spans(&table.rows[1].cells[0].spans),
            "left | right"
        );
    }

    #[test]
    fn incremental_multiline_callout_is_supported() {
        let block = import_markdown_block_incremental(
            "> [!WARNING]\n> be careful",
            MarkdownImportOptions::default(),
        )
        .expect("callout markdown should parse");
        assert!(matches!(
            block.kind,
            RichBlockKind::Callout {
                variant: CalloutVariant::Warning
            }
        ));
    }
}
