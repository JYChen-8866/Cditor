use std::ops::Range;

use pulldown_cmark::{Event, LinkType, Options, Parser, Tag, TagEnd};

pub type SyntaxNodeId = usize;

#[derive(Debug, Clone, PartialEq)]
pub enum SyntaxKind {
    Document,
    Block(TagEnd),
    Strong,
    Emphasis,
    Strike,
    Link { destination: String, title: String, reference: String, link_type: LinkType },
    Image { destination: String, title: String },
    Text(String),
    Code(String),
    SoftBreak,
    HardBreak,
    /// Syntax not projected as editable text, including HTML and task markers.
    Literal,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SyntaxNode {
    pub kind: SyntaxKind,
    /// UTF-8 bytes, including delimiters. Valid only in this syntax revision.
    pub source: Range<usize>,
    pub children: Vec<SyntaxNodeId>,
}

/// An arena avoids recursion on deeply nested imported Markdown. Parser ranges
/// plus the intervening source gaps preserve every byte, including definitions
/// for reference links (which pulldown-cmark does not emit as events).
#[derive(Debug, Clone)]
pub struct MarkdownSyntax {
    nodes: Vec<SyntaxNode>,
}

pub fn markdown_options() -> Options {
    Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS
}

impl MarkdownSyntax {
    pub fn parse(source: &str) -> Self {
        Self::parse_with_references(source, &std::collections::HashMap::new())
    }

    /// Resolve document reference links while parsing only one edited block.
    pub fn parse_with_references(source: &str, references: &std::collections::HashMap<String, (String, String)>) -> Self {
        let mut callback = |link: pulldown_cmark::BrokenLink<'_>| {
            references.get(&link.reference.to_lowercase()).map(|(url, title)| (url.clone().into(), title.clone().into()))
        };
        let mut nodes = vec![SyntaxNode { kind: SyntaxKind::Document, source: 0..source.len(), children: vec![] }];
        let mut stack = vec![0];
        for (event, range) in Parser::new_with_broken_link_callback(source, markdown_options(), Some(&mut callback)).into_offset_iter() {
            let is_start = matches!(event, Event::Start(_));
            let kind = match event {
                Event::Start(tag) => match tag {
                    Tag::Strong => SyntaxKind::Strong,
                    Tag::Emphasis => SyntaxKind::Emphasis,
                    Tag::Strikethrough => SyntaxKind::Strike,
                    Tag::Link { dest_url, title, id, link_type } => SyntaxKind::Link {
                        destination: dest_url.to_string(), title: title.to_string(), reference: id.to_string(), link_type,
                    },
                    Tag::Image { dest_url, title, .. } => SyntaxKind::Image {
                        destination: dest_url.to_string(), title: title.to_string(),
                    },
                    _ => SyntaxKind::Block(tag.to_end()),
                },
                Event::End(_) => { stack.pop(); continue; },
                Event::Text(text) => SyntaxKind::Text(text.to_string()),
                Event::Code(text) => SyntaxKind::Code(text.to_string()),
                Event::SoftBreak => SyntaxKind::SoftBreak,
                Event::HardBreak => SyntaxKind::HardBreak,
                _ => SyntaxKind::Literal,
            };
            let id = nodes.len();
            nodes.push(SyntaxNode { kind, source: range, children: vec![] });
            nodes[*stack.last().unwrap()].children.push(id);
            if is_start { stack.push(id); }
        }
        Self { nodes }
    }

    pub fn nodes(&self) -> &[SyntaxNode] { &self.nodes }

    pub fn node(&self, id: SyntaxNodeId) -> Option<&SyntaxNode> { self.nodes.get(id) }

    /// Source gaps cover delimiters, escapes, URLs, titles, block prefixes and
    /// unrendered syntax. No delimiter lengths are guessed from the text.
    pub fn gaps(&self, id: SyntaxNodeId) -> Vec<Range<usize>> {
        let node = &self.nodes[id];
        let mut position = node.source.start;
        let mut gaps = Vec::new();
        for &child in &node.children {
            let range = &self.nodes[child].source;
            if range.start > position { gaps.push(position..range.start); }
            position = position.max(range.end);
        }
        if position < node.source.end { gaps.push(position..node.source.end); }
        gaps
    }
}
