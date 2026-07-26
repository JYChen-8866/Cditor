use std::ops::Range;

use pulldown_cmark::{CodeBlockKind, Event, LinkType, Options, Parser, Tag, TagEnd};

use super::{
    InlineSpan, MarkdownParser, ParsedMarkdownDocument, RichBlockKind, RichBlockRecord,
    parse_inline_markdown,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RegionKind {
    Paragraph,
    BlockQuote,
    Other,
}

#[derive(Debug)]
struct Region {
    range: Range<usize>,
    kind: RegionKind,
    unsupported: bool,
}

struct OpenRegion {
    start: usize,
    end: usize,
    depth: usize,
    kind: RegionKind,
    unsupported: bool,
}

pub(super) fn parse_document(
    parser: &mut MarkdownParser,
    markdown: &str,
) -> ParsedMarkdownDocument {
    let regions = regions(markdown);
    if regions.is_empty() {
        return parser.parse_document(markdown);
    }

    let mut document = ParsedMarkdownDocument::default();
    for region in regions {
        let source = markdown
            .get(region.range.clone())
            .unwrap_or_default()
            .trim_end_matches(['\r', '\n']);
        if source.trim().is_empty() {
            continue;
        }
        if region.unsupported {
            let mut block = RichBlockRecord::raw_markdown(parser.next_id(), source);
            block.document_id = parser.document_id;
            document.push_root_block(block);
            continue;
        }
        match region.kind {
            RegionKind::Paragraph => {
                document.push_root_block(parser.rich_text_block(
                    RichBlockKind::Paragraph,
                    parse_inline_markdown(source.trim()),
                ));
            }
            RegionKind::BlockQuote => {
                if let Some(block) = parser.parse_incremental_quote_or_callout_block(source) {
                    document.push_root_block(block);
                } else {
                    append_document(&mut document, parser.parse_document(source));
                }
            }
            RegionKind::Other => append_document(&mut document, parser.parse_document(source)),
        }
    }
    if document.blocks.is_empty() {
        document.push_root_block(parser.rich_text_block(
            RichBlockKind::Paragraph,
            vec![InlineSpan::plain(String::new())],
        ));
    }
    document
}

fn regions(markdown: &str) -> Vec<Region> {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_HEADING_ATTRIBUTES
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | Options::ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS
        | Options::ENABLE_MATH
        | Options::ENABLE_GFM
        | Options::ENABLE_DEFINITION_LIST;
    let mut regions = Vec::new();
    let mut open: Option<OpenRegion> = None;

    for (event, range) in Parser::new_ext(markdown, options).into_offset_iter() {
        match &event {
            Event::Start(tag) if is_block_tag(tag) => {
                if let Some(open) = &mut open {
                    open.depth += 1;
                    open.end = open.end.max(range.end);
                    open.unsupported |= unsupported_tag(tag, markdown, range.start);
                } else {
                    open = Some(OpenRegion {
                        start: range.start,
                        end: range.end,
                        depth: 1,
                        kind: region_kind(tag),
                        unsupported: unsupported_tag(tag, markdown, range.start),
                    });
                }
            }
            Event::End(end) if is_block_end(*end) => {
                if let Some(current) = &mut open {
                    current.end = current.end.max(range.end);
                    current.depth = current.depth.saturating_sub(1);
                    if current.depth == 0 {
                        let completed = open.take().expect("open region exists");
                        regions.push(Region {
                            range: completed.start..completed.end,
                            kind: completed.kind,
                            unsupported: completed.unsupported,
                        });
                    }
                }
            }
            Event::Rule if open.is_none() => regions.push(Region {
                range,
                kind: RegionKind::Other,
                unsupported: false,
            }),
            _ => {
                if let Some(current) = &mut open {
                    current.end = current.end.max(range.end);
                    current.unsupported |= unsupported_event(&event);
                }
            }
        }
    }
    if let Some(unclosed) = open {
        regions.push(Region {
            range: unclosed.start..unclosed.end,
            kind: unclosed.kind,
            unsupported: true,
        });
    }
    preserve_unparsed_source(regions, markdown)
}

fn preserve_unparsed_source(mut regions: Vec<Region>, markdown: &str) -> Vec<Region> {
    regions.sort_by_key(|region| region.range.start);
    let mut covered: Vec<Region> = Vec::with_capacity(regions.len() + 1);
    let mut cursor = 0usize;
    for region in regions {
        if cursor < region.range.start {
            let gap = cursor..region.range.start;
            if !markdown[gap.clone()].trim().is_empty() {
                if let Some(previous) = covered.last_mut().filter(|item| item.unsupported) {
                    previous.range.end = gap.end;
                } else {
                    covered.push(Region {
                        range: gap,
                        kind: RegionKind::Other,
                        unsupported: true,
                    });
                }
            }
        }
        if let Some(previous) = covered.last_mut()
            && previous.unsupported
            && region.unsupported
            && markdown[previous.range.end..region.range.start]
                .trim()
                .is_empty()
        {
            previous.range.end = region.range.end;
        } else {
            covered.push(region);
        }
        cursor = covered.last().map_or(cursor, |item| item.range.end);
    }
    if cursor < markdown.len() && !markdown[cursor..].trim().is_empty() {
        if let Some(previous) = covered.last_mut().filter(|item| item.unsupported) {
            previous.range.end = markdown.len();
        } else {
            covered.push(Region {
                range: cursor..markdown.len(),
                kind: RegionKind::Other,
                unsupported: true,
            });
        }
    }
    covered
}

fn region_kind(tag: &Tag<'_>) -> RegionKind {
    match tag {
        Tag::Paragraph => RegionKind::Paragraph,
        Tag::BlockQuote(_) => RegionKind::BlockQuote,
        _ => RegionKind::Other,
    }
}

fn unsupported_tag(tag: &Tag<'_>, markdown: &str, start: usize) -> bool {
    match tag {
        Tag::HtmlBlock
        | Tag::FootnoteDefinition(_)
        | Tag::DefinitionList
        | Tag::DefinitionListTitle
        | Tag::DefinitionListDefinition
        | Tag::MetadataBlock(_)
        | Tag::Image { .. } => true,
        Tag::Heading {
            id, classes, attrs, ..
        } => {
            id.is_some()
                || !classes.is_empty()
                || !attrs.is_empty()
                || !markdown
                    .get(start..)
                    .is_some_and(|source| source.trim_start().starts_with('#'))
        }
        Tag::Link { link_type, .. } => *link_type != LinkType::Inline,
        Tag::CodeBlock(CodeBlockKind::Indented) => true,
        Tag::CodeBlock(CodeBlockKind::Fenced(_)) => !markdown
            .get(start..)
            .is_some_and(|source| source.trim_start().starts_with("```")),
        _ => false,
    }
}

fn unsupported_event(event: &Event<'_>) -> bool {
    matches!(
        event,
        Event::Html(_)
            | Event::InlineHtml(_)
            | Event::FootnoteReference(_)
            | Event::InlineMath(_)
            | Event::DisplayMath(_)
    ) || matches!(event, Event::Start(tag) if unsupported_tag(tag, "", 0))
}

fn is_block_tag(tag: &Tag<'_>) -> bool {
    matches!(
        tag,
        Tag::Paragraph
            | Tag::Heading { .. }
            | Tag::BlockQuote(_)
            | Tag::CodeBlock(_)
            | Tag::HtmlBlock
            | Tag::List(_)
            | Tag::Item
            | Tag::FootnoteDefinition(_)
            | Tag::DefinitionList
            | Tag::DefinitionListTitle
            | Tag::DefinitionListDefinition
            | Tag::Table(_)
            | Tag::TableHead
            | Tag::TableRow
            | Tag::TableCell
            | Tag::MetadataBlock(_)
    )
}

fn is_block_end(end: TagEnd) -> bool {
    matches!(
        end,
        TagEnd::Paragraph
            | TagEnd::Heading(_)
            | TagEnd::BlockQuote(_)
            | TagEnd::CodeBlock
            | TagEnd::HtmlBlock
            | TagEnd::List(_)
            | TagEnd::Item
            | TagEnd::FootnoteDefinition
            | TagEnd::DefinitionList
            | TagEnd::DefinitionListTitle
            | TagEnd::DefinitionListDefinition
            | TagEnd::Table
            | TagEnd::TableHead
            | TagEnd::TableRow
            | TagEnd::TableCell
            | TagEnd::MetadataBlock(_)
    )
}

fn append_document(target: &mut ParsedMarkdownDocument, source: ParsedMarkdownDocument) {
    target.root_blocks.extend(source.root_blocks);
    target.blocks.extend(source.blocks);
}
