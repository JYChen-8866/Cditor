use crate::rich_text::{InlineMark, InlineSpan};
use super::{MarkdownError, MarkdownResult};
use pulldown_cmark::{Event, Parser, Tag, TagEnd};

use super::syntax::markdown_options;

#[derive(Debug)]
struct Node {
    mark: Option<InlineMark>,
    text: String,
    children: Vec<usize>,
}

/// Serialize shared ancestors once, never wrap each visual run independently.
/// A reparse is mandatory: Markdown delimiter rules depend on adjacent text.
pub fn render_inline_spans(spans: &[InlineSpan]) -> MarkdownResult<String> {
    let mut nodes = vec![Node { mark: None, text: String::new(), children: vec![] }];
    let mut stack = vec![0];
    let mut active = Vec::new();
    for span in spans.iter().filter(|span| !span.text.is_empty()) {
        for mark in &span.marks {
            if !matches!(mark, InlineMark::Bold | InlineMark::Italic | InlineMark::Strike
                | InlineMark::Code | InlineMark::Link { .. } | InlineMark::DocumentLink { .. }) {
                return Err(unsupported("mark has no supported Markdown representation"));
            }
        }
        let shared = active.iter().zip(&span.marks).take_while(|(a, b)| a == b).count();
        active.truncate(shared);
        stack.truncate(shared + 1);
        for mark in &span.marks[shared..] {
            let id = nodes.len();
            nodes.push(Node { mark: Some(mark.clone()), text: String::new(), children: vec![] });
            nodes[*stack.last().unwrap()].children.push(id);
            stack.push(id);
            active.push(mark.clone());
        }
        let id = nodes.len();
        nodes.push(Node { mark: None, text: span.text.clone(), children: vec![] });
        nodes[*stack.last().unwrap()].children.push(id);
    }
    // Try both delimiter families; neither is universally legal (e.g. intraword '_').
    for italic in ["*", "_"] {
        let output = render_node(0, &nodes, italic, 0)?;
        if let Ok(parsed) = parse_inline_spans(&output)
            && equivalent_spans(spans, &parsed) {
            return Ok(output);
        }
    }
    Err(unsupported("inline formatting cannot be represented without changing Markdown semantics"))
}

fn render_node(id: usize, nodes: &[Node], italic: &str, depth: usize) -> MarkdownResult<String> {
    if depth > 128 {
        return Err(unsupported("inline nesting exceeds 128 levels"));
    }
    let node = &nodes[id];
    if matches!(node.mark, Some(InlineMark::Code)) {
        if node.children.iter().any(|&child| nodes[child].mark.is_some()) {
            return Err(unsupported("formatting inside a code span is not Markdown"));
        }
        let text: String = node.children.iter().map(|&child| nodes[child].text.as_str()).collect();
        return code_span(&text);
    }
    let mut content = escape_text(&node.text);
    for &child in &node.children {
        content.push_str(&render_node(child, nodes, italic, depth + 1)?);
    }
    Ok(match &node.mark {
        None => content,
        Some(InlineMark::Bold) => format!("**{content}**"),
        Some(InlineMark::Italic) => format!("{italic}{content}{italic}"),
        Some(InlineMark::Strike) => format!("~~{content}~~"),
        Some(InlineMark::Link { href } | InlineMark::DocumentLink { href }) => {
            if href.contains(['\n', '\r']) {
                return Err(unsupported("line breaks in link destinations are unsupported"));
            }
            let href = href.replace('\\', "\\\\").replace('<', "\\<").replace('>', "\\>");
            format!("[{content}](<{href}>)")
        }
        _ => return Err(unsupported("unsupported inline mark")),
    })
}

pub(crate) fn code_span(text: &str) -> MarkdownResult<String> {
    if text.is_empty() || text.contains(['\n', '\r']) {
        return Err(unsupported("empty or multiline code span cannot preserve its text"));
    }
    let fence = "`".repeat(text.split(|c| c != '`').map(str::len).max().unwrap_or(0) + 1);
    let pad = text.starts_with('`') || text.ends_with('`')
        || (text.starts_with(' ') && text.ends_with(' ') && text.chars().any(|c| c != ' '));
    let padding = if pad { " " } else { "" };
    Ok(format!("{fence}{padding}{text}{padding}{fence}"))
}

pub(crate) fn escape_text(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        // Block syntax and HTML/entities can also be introduced by an inline edit.
        if ch.is_ascii_punctuation() {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

pub(crate) fn parse_inline_spans(markdown: &str) -> MarkdownResult<Vec<InlineSpan>> {
    let mut marks = Vec::new();
    let mut spans = Vec::new();
    let mut paragraphs = 0;
    for event in Parser::new_ext(markdown, markdown_options()) {
        match event {
            Event::Start(Tag::Paragraph) => paragraphs += 1,
            Event::End(TagEnd::Paragraph) => {},
            Event::Start(Tag::Strong) => marks.push(InlineMark::Bold),
            Event::Start(Tag::Emphasis) => marks.push(InlineMark::Italic),
            Event::Start(Tag::Strikethrough) => marks.push(InlineMark::Strike),
            Event::Start(Tag::Link { dest_url, .. }) => marks.push(InlineMark::Link { href: dest_url.to_string() }),
            Event::End(TagEnd::Strong | TagEnd::Emphasis | TagEnd::Strikethrough | TagEnd::Link) => { marks.pop(); },
            Event::Text(text) => spans.push(InlineSpan { text: text.to_string(), marks: marks.clone() }),
            Event::Code(text) => {
                let mut code_marks = marks.clone();
                code_marks.push(InlineMark::Code);
                spans.push(InlineSpan { text: text.to_string(), marks: code_marks });
            },
            Event::SoftBreak => spans.push(InlineSpan { text: "\n".into(), marks: marks.clone() }),
            _ => return Err(unsupported("generated inline text introduced unsupported syntax")),
        }
    }
    if paragraphs > 1 {
        return Err(unsupported("generated inline text introduced extra blocks"));
    }
    Ok(spans)
}

pub fn equivalent_spans(a: &[InlineSpan], b: &[InlineSpan]) -> bool {
    fn normalized(spans: &[InlineSpan]) -> Vec<InlineSpan> {
        let mut output: Vec<InlineSpan> = vec![];
        for span in spans.iter().filter(|s| !s.text.is_empty()) {
            let mut marks: Vec<_> = span.marks.iter().map(|mark| match mark {
                InlineMark::DocumentLink { href } => InlineMark::Link { href: href.clone() },
                _ => mark.clone(),
            }).collect();
            marks.sort_by_cached_key(|mark| format!("{mark:?}"));
            marks.dedup();
            if let Some(last) = output.last_mut().filter(|last| last.marks == marks) {
                last.text.push_str(&span.text);
            } else {
                output.push(InlineSpan { text: span.text.clone(), marks });
            }
        }
        output
    }
    normalized(a) == normalized(b)
}

pub(crate) fn unsupported(message: &str) -> MarkdownError {
    MarkdownError::Unsupported(message.to_owned())
}
