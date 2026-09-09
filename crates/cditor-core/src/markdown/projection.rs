use std::ops::Range;

use crate::rich_text::{InlineMark, InlineSpan};
use super::{Affinity, MarkdownError, MarkdownResult, MarkdownSyntax, SyntaxKind, SyntaxNodeId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MappingKind { Exact, Transformed }

#[derive(Debug, Clone)]
pub struct ProjectionSegment {
    pub source: Range<usize>,
    pub visible: Range<usize>,
    pub mapping: MappingKind,
}

#[derive(Debug, Clone, Default)]
pub struct InlineProjection {
    pub text: String,
    pub spans: Vec<InlineSpan>,
    pub segments: Vec<ProjectionSegment>,
}

impl MarkdownSyntax {
    /// Project a paragraph/heading/inline subtree, not a whole document layout.
    pub fn project_inline(&self, id: SyntaxNodeId, source: &str) -> MarkdownResult<InlineProjection> {
        if self.node(id).is_none() { return Err(MarkdownError::InvalidRange); }
        let mut result = InlineProjection::default();
        let mut stack = vec![(id, Vec::<InlineMark>::new())];
        while let Some((id, mut marks)) = stack.pop() {
            let node = &self.nodes()[id];
            let raw = source.get(node.source.clone()).ok_or(MarkdownError::InvalidRange)?;
            let text = match &node.kind {
                SyntaxKind::Strong => { marks.push(InlineMark::Bold); None },
                SyntaxKind::Emphasis => { marks.push(InlineMark::Italic); None },
                SyntaxKind::Strike => { marks.push(InlineMark::Strike); None },
                SyntaxKind::Link { destination, .. } => {
                    marks.push(InlineMark::Link { href: destination.clone() }); None
                },
                SyntaxKind::Code(text) => { marks.push(InlineMark::Code); Some(text.as_str()) },
                SyntaxKind::Text(text) => Some(text.as_str()),
                SyntaxKind::SoftBreak | SyntaxKind::HardBreak => Some("\n"),
                SyntaxKind::Image { .. } | SyntaxKind::Literal => {
                    return Err(MarkdownError::Unsupported("inline subtree contains non-text syntax".into()));
                },
                _ => None,
            };
            if let Some(text) = text {
                let start = result.text.len();
                result.text.push_str(text);
                result.spans.push(InlineSpan { text: text.to_owned(), marks: marks.clone() });
                result.segments.push(ProjectionSegment {
                    source: node.source.clone(), visible: start..result.text.len(),
                    mapping: if raw == text { MappingKind::Exact } else { MappingKind::Transformed },
                });
            }
            for &child in node.children.iter().rev() { stack.push((child, marks.clone())); }
        }
        Ok(result)
    }
}

impl InlineProjection {
    /// Transformed tokens (entities, code spans) are indivisible mappings.
    /// Internal positions are rejected rather than treated as a byte delta.
    pub fn visible_to_source(&self, offset: usize, affinity: Affinity) -> MarkdownResult<usize> {
        if !self.text.is_char_boundary(offset) { return Err(MarkdownError::InvalidRange); }
        let mut candidates = Vec::new();
        for segment in &self.segments {
            if offset == segment.visible.start { candidates.push(segment.source.start); }
            if offset == segment.visible.end { candidates.push(segment.source.end); }
            if segment.visible.start < offset && offset < segment.visible.end {
                return match segment.mapping {
                    MappingKind::Exact => Ok(segment.source.start + offset - segment.visible.start),
                    MappingKind::Transformed => Err(MarkdownError::Unsupported("position inside a transformed token".into())),
                };
            }
        }
        match affinity {
            Affinity::Before => candidates.into_iter().min(),
            Affinity::After => candidates.into_iter().max(),
        }.ok_or(MarkdownError::InvalidRange)
    }

    pub fn source_to_visible(&self, offset: usize, affinity: Affinity) -> MarkdownResult<usize> {
        let mut before = 0;
        for segment in &self.segments {
            if offset < segment.source.start {
                return Ok(if affinity == Affinity::Before { before } else { segment.visible.start });
            }
            if offset <= segment.source.end {
                if offset == segment.source.start { return Ok(segment.visible.start); }
                if offset == segment.source.end { return Ok(segment.visible.end); }
                let mapped = segment.visible.start + offset - segment.source.start;
                return match segment.mapping {
                    MappingKind::Exact if self.text.is_char_boundary(mapped) => Ok(mapped),
                    MappingKind::Exact => Err(MarkdownError::InvalidRange),
                    MappingKind::Transformed => Ok(if affinity == Affinity::Before { segment.visible.start } else { segment.visible.end }),
                };
            }
            before = segment.visible.end;
        }
        Ok(before)
    }
}
