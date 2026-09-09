//! Optional Markdown source model, independent of the rich-text runtime and UI.
//!
//! `MarkdownSource` owns source bytes; syntax and spans are derived projections.
//! Parsing currently rebuilds the syntax index and belongs on a worker for large
//! documents, not in a keystroke callback. Existing rich-text documents are unchanged.

mod projection;
mod serialize;
mod source;
mod syntax;

pub use projection::{InlineProjection, MappingKind, ProjectionSegment};
pub use serialize::{equivalent_spans, render_inline_spans};
pub use source::{Affinity, ByteOffset, MarkdownSource, SourceEdit, SourceSelection, SourceTransaction};
pub use syntax::{MarkdownSyntax, SyntaxKind, SyntaxNode, SyntaxNodeId, markdown_options};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarkdownError {
    InvalidRange,
    OverlappingEdits,
    StaleRevision,
    Unsupported(String),
}

impl std::fmt::Display for MarkdownError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRange => f.write_str("invalid Markdown byte/UTF-16 boundary"),
            Self::OverlappingEdits => f.write_str("Markdown source edits overlap"),
            Self::StaleRevision => f.write_str("Markdown source revision is stale"),
            Self::Unsupported(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for MarkdownError {}
pub type MarkdownResult<T> = Result<T, MarkdownError>;
