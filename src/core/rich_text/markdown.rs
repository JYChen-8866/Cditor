use super::{
    BlockPayload, CalloutVariant, InlineMark, InlineSpan, RichBlockKind, RichBlockRecord,
    RichTextDocument, TableCellPayload, TablePayload, TableRowPayload,
};
use crate::core::ids::{BlockId, DocumentId};
use crate::core::rich_text::MARKDOWN_PARSE_STATS;

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
        "###### ", "##### ", "#### ", "### ", "## ", "# ", "[ ] ", "[x] ", "[X] ", "1. ", "- ",
        "* ", "> ",
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

fn block_kind_for_marker(marker: &str) -> Option<RichBlockKind> {
    match marker {
        "#" => Some(RichBlockKind::Heading { level: 1 }),
        "##" => Some(RichBlockKind::Heading { level: 2 }),
        "###" => Some(RichBlockKind::Heading { level: 3 }),
        "####" => Some(RichBlockKind::Heading { level: 4 }),
        "#####" => Some(RichBlockKind::Heading { level: 5 }),
        "######" => Some(RichBlockKind::Heading { level: 6 }),
        "-" | "*" => Some(RichBlockKind::BulletedList),
        "1." => Some(RichBlockKind::NumberedList),
        "[ ]" => Some(RichBlockKind::Todo { checked: false }),
        "[x]" | "[X]" => Some(RichBlockKind::Todo { checked: true }),
        ">" => Some(RichBlockKind::Quote),
        _ => None,
    }
}

fn parse_callout_marker(line: &str) -> Option<CalloutVariant> {
    match line.trim() {
        "[!NOTE]" => Some(CalloutVariant::Note),
        "[!TIP]" => Some(CalloutVariant::Tip),
        "[!IMPORTANT]" => Some(CalloutVariant::Important),
        "[!WARNING]" => Some(CalloutVariant::Warning),
        "[!CAUTION]" => Some(CalloutVariant::Caution),
        _ => None,
    }
}

fn looks_like_single_block_markdown(line: &str) -> bool {
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
        || parse_numbered_item(line).is_some()
}

fn parse_heading(line: &str) -> Option<(u8, &str)> {
    let level = line.bytes().take_while(|byte| *byte == b'#').count();
    if !(1..=6).contains(&level) {
        return None;
    }
    let text = line[level..].strip_prefix(' ')?;
    Some((level as u8, text))
}

fn parse_numbered_item(line: &str) -> Option<&str> {
    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    line[digits..].strip_prefix(". ")
}

fn parse_fence_start(line: &str) -> Option<(Option<String>, &str)> {
    let trimmed = line.trim_start();
    let rest = trimmed.strip_prefix("```")?;
    let language = rest.trim();
    Some(((!language.is_empty()).then(|| language.to_string()), ""))
}

fn is_table_candidate_line(line: &str) -> bool {
    line.trim_start().starts_with('|')
}

fn collect_table_candidate_region(lines: &[&str], start: usize) -> usize {
    let mut index = start + 1;
    while index < lines.len() && is_table_candidate_line(lines[index]) {
        index += 1;
    }
    index
}

fn parse_table_region(lines: &[&str]) -> Option<TablePayload> {
    if lines.len() < 2 {
        return None;
    }
    let header = split_table_cells(lines[0])?;
    let alignment = split_table_cells(lines[1])?;
    if header.is_empty() || alignment.len() != header.len() {
        return None;
    }
    if !alignment.iter().all(|cell| is_alignment_cell(cell)) {
        return None;
    }

    let mut rows = Vec::with_capacity(lines.len() - 1);
    rows.push(table_row_from_cells(header));
    for line in &lines[2..] {
        let cells = split_table_cells(line)?;
        if cells.len() != rows[0].cells.len() {
            return None;
        }
        rows.push(table_row_from_cells(cells));
    }
    Some(TablePayload {
        rows,
        header_rows: 1,
        header_cols: 0,
    })
}

fn split_table_cells(line: &str) -> Option<Vec<String>> {
    let trimmed = line.trim();
    let without_left = trimmed.strip_prefix('|').unwrap_or(trimmed);
    let without_edges = without_left.strip_suffix('|').unwrap_or(without_left);
    let mut cells = Vec::new();
    let mut cell = String::new();
    let mut chars = without_edges.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' && chars.peek() == Some(&'|') {
            cell.push('|');
            let _ = chars.next();
        } else if ch == '|' {
            cells.push(cell.trim().to_owned());
            cell.clear();
        } else {
            cell.push(ch);
        }
    }
    cells.push(cell.trim().to_owned());
    (!cells.is_empty()).then_some(cells)
}

fn is_alignment_cell(cell: &String) -> bool {
    let trimmed = cell.trim();
    let inner = trimmed.trim_matches(':');
    !inner.is_empty() && inner.chars().all(|ch| ch == '-')
}

fn table_row_from_cells(cells: Vec<String>) -> TableRowPayload {
    TableRowPayload {
        cells: cells
            .into_iter()
            .map(|cell| TableCellPayload {
                spans: parse_inline_markdown(&cell),
            })
            .collect(),
    }
}

fn block_to_plain_markdown(block: &RichBlockRecord) -> String {
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

fn table_to_plain_markdown(payload: &BlockPayload) -> Option<String> {
    let BlockPayload::Table(table) = payload else {
        return None;
    };
    let first = table.rows.first()?;
    let columns = first.cells.len();
    if columns == 0 {
        return None;
    }
    let mut lines = Vec::new();
    for (row_index, row) in table.rows.iter().enumerate() {
        let cells = row
            .cells
            .iter()
            .map(|cell| {
                escape_table_cell(&crate::core::rich_text::plain_text_from_spans(&cell.spans))
            })
            .collect::<Vec<_>>();
        lines.push(format!("| {} |", cells.join(" | ")));
        if row_index == 0 {
            lines.push(format!("| {} |", vec!["---"; columns].join(" | ")));
        }
    }
    Some(lines.join("\n"))
}

fn escape_table_cell(cell: &str) -> String {
    cell.replace('|', "\\|")
}

fn parse_inline_markdown(markdown: &str) -> Vec<InlineSpan> {
    let parsed = parse_inline_markdown_extended(markdown);
    let mut spans = parsed.spans;
    merge_inline_spans(&mut spans);
    if spans.is_empty() {
        spans.push(InlineSpan::plain(String::new()));
    }
    spans
}

struct InlineParseResult {
    spans: Vec<InlineSpan>,
    changed: bool,
}

fn parse_inline_markdown_extended(text: &str) -> InlineParseResult {
    let mut spans = Vec::new();
    let mut plain = String::new();
    let mut changed = false;
    let mut cursor = 0usize;

    while cursor < text.len() {
        let rest = &text[cursor..];
        if let Some(consumed) = parse_markdown_image(rest) {
            plain.push_str(&rest[..consumed]);
            cursor += consumed;
            continue;
        }
        if let Some((label, href, consumed)) = parse_markdown_link(rest) {
            flush_plain_span(&mut spans, &mut plain);
            spans.push(InlineSpan {
                text: label.to_string(),
                marks: vec![InlineMark::Link {
                    href: href.to_string(),
                }],
            });
            cursor += consumed;
            changed = true;
            continue;
        }
        if let Some((content, consumed)) = parse_delimited_mark(rest, "**") {
            flush_plain_span(&mut spans, &mut plain);
            spans.push(InlineSpan {
                text: content.to_string(),
                marks: vec![InlineMark::Bold],
            });
            cursor += consumed;
            changed = true;
            continue;
        }
        if let Some((content, consumed)) = parse_delimited_mark(rest, "~~") {
            flush_plain_span(&mut spans, &mut plain);
            spans.push(InlineSpan {
                text: content.to_string(),
                marks: vec![InlineMark::Strike],
            });
            cursor += consumed;
            changed = true;
            continue;
        }
        if let Some((content, consumed)) = parse_delimited_mark(rest, "++") {
            flush_plain_span(&mut spans, &mut plain);
            spans.push(InlineSpan {
                text: content.to_string(),
                marks: vec![InlineMark::Underline],
            });
            cursor += consumed;
            changed = true;
            continue;
        }
        if let Some((content, consumed)) = parse_delimited_mark(rest, "*") {
            flush_plain_span(&mut spans, &mut plain);
            spans.push(InlineSpan {
                text: content.to_string(),
                marks: vec![InlineMark::Italic],
            });
            cursor += consumed;
            changed = true;
            continue;
        }
        if let Some((content, consumed)) = parse_delimited_mark(rest, "_") {
            flush_plain_span(&mut spans, &mut plain);
            spans.push(InlineSpan {
                text: content.to_string(),
                marks: vec![InlineMark::Italic],
            });
            cursor += consumed;
            changed = true;
            continue;
        }
        if let Some((url, consumed)) = parse_auto_link(rest) {
            flush_plain_span(&mut spans, &mut plain);
            spans.push(InlineSpan {
                text: url.to_string(),
                marks: vec![InlineMark::Link {
                    href: url.to_string(),
                }],
            });
            cursor += consumed;
            changed = true;
            continue;
        }
        if let Some((content, consumed)) = parse_delimited_mark(rest, "``") {
            flush_plain_span(&mut spans, &mut plain);
            spans.push(InlineSpan {
                text: content.to_string(),
                marks: vec![InlineMark::Code],
            });
            cursor += consumed;
            changed = true;
            continue;
        }
        if let Some((content, consumed)) = parse_delimited_mark(rest, "`") {
            flush_plain_span(&mut spans, &mut plain);
            spans.push(InlineSpan {
                text: content.to_string(),
                marks: vec![InlineMark::Code],
            });
            cursor += consumed;
            changed = true;
            continue;
        }

        let Some(ch) = rest.chars().next() else {
            break;
        };
        plain.push(ch);
        cursor += ch.len_utf8();
    }

    flush_plain_span(&mut spans, &mut plain);
    merge_inline_spans(&mut spans);
    InlineParseResult { spans, changed }
}

fn flush_plain_span(spans: &mut Vec<InlineSpan>, plain: &mut String) {
    if !plain.is_empty() {
        spans.push(InlineSpan::plain(std::mem::take(plain)));
    }
}

fn merge_inline_spans(spans: &mut Vec<InlineSpan>) {
    let mut merged: Vec<InlineSpan> = Vec::new();
    for span in spans.drain(..) {
        if span.text.is_empty() {
            continue;
        }
        if let Some(last) = merged.last_mut()
            && last.marks == span.marks
        {
            last.text.push_str(&span.text);
            continue;
        }
        merged.push(span);
    }
    *spans = merged;
}

fn parse_delimited_mark<'a>(text: &'a str, delimiter: &str) -> Option<(&'a str, usize)> {
    let inner = text.strip_prefix(delimiter)?;
    let end = inner.find(delimiter)?;
    if end == 0 {
        return None;
    }
    Some((&inner[..end], delimiter.len() + end + delimiter.len()))
}

fn parse_auto_link(text: &str) -> Option<(&str, usize)> {
    let prefix_len = if text.starts_with("https://") {
        "https://".len()
    } else if text.starts_with("http://") {
        "http://".len()
    } else {
        return None;
    };
    let mut end = prefix_len;
    for (index, ch) in text[prefix_len..].char_indices() {
        if ch.is_whitespace() || matches!(ch, '<' | '>' | '[' | ']' | '(' | ')' | '"') {
            break;
        }
        end = prefix_len + index + ch.len_utf8();
    }
    while end > prefix_len
        && text[..end]
            .chars()
            .next_back()
            .is_some_and(|ch| matches!(ch, '.' | ',' | ';' | ':' | '!' | '?'))
    {
        end -= text[..end].chars().next_back().map_or(0, char::len_utf8);
    }
    (end > prefix_len).then_some((&text[..end], end))
}

fn parse_markdown_image(text: &str) -> Option<usize> {
    let inner = text.strip_prefix("![")?;
    let label_end = inner.find("](")?;
    let after_label = &inner[label_end + 2..];
    let href_end = after_label.find(')')?;
    Some(2 + label_end + 2 + href_end + 1)
}

fn parse_markdown_link(text: &str) -> Option<(&str, &str, usize)> {
    let inner = text.strip_prefix('[')?;
    let label_end = inner.find("](")?;
    if label_end == 0 {
        return None;
    }
    let after_label = &inner[label_end + 2..];
    let href_end = after_label.find(')')?;
    if href_end == 0 {
        return None;
    }
    let label = &inner[..label_end];
    let href = &after_label[..href_end];
    Some((label, href, 1 + label_end + 2 + href_end + 1))
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
            crate::core::rich_text::plain_text_from_spans(&table.rows[1].cells[0].spans),
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
