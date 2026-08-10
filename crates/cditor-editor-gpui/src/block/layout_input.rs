//! Single source of truth for a document block's text layout input.
//!
//! Both the paint path (`block_content`) and the prewarm path
//! (`text_layout_prewarm`) must shape the exact same input, or the two agents
//! populate the layout cache with diverging shapes for the same surface and
//! the block on screen stops matching the runtime's projection. This has
//! bitten repeatedly for code blocks: paint suppresses syntax-highlight spans
//! while an IME composition is active (the projected payload carries the
//! preview text; highlight entries are keyed to the committed content), but a
//! prewarm that applies highlights unconditionally shapes the *old* text and
//! the preview letters never reach the screen. Building the input here, once,
//! makes that divergence structurally impossible.

use crate::features::code::highlight::CodeHighlightCache;
use crate::text::RichTextLayoutInput;
use cditor_core::rich_text::RichBlockKind;
use cditor_runtime::ViewBlockSnapshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CodeSpansSource {
    /// Not a code block; spans come straight from the projected payload.
    NotCode,
    /// Committed code text with syntax-highlight spans applied.
    Highlight,
    /// Code text rendered plain because no highlight entry is ready yet.
    Plain,
    /// An IME composition is active: the projected payload contains the
    /// preview text, which the highlight cache knows nothing about. Highlight
    /// spans must not replace it.
    SkippedMarked,
}

pub(crate) struct DocumentBlockLayoutInput {
    pub(crate) input: RichTextLayoutInput,
    pub(crate) code_spans: CodeSpansSource,
}

/// Builds the canonical text layout input for a document block. Every caller
/// that shapes or looks up a document block layout must go through this.
pub(crate) fn document_block_layout_input(
    block: &ViewBlockSnapshot,
    text_layout_width_px: f64,
    code_highlights: &CodeHighlightCache,
) -> Option<DocumentBlockLayoutInput> {
    let mut input = RichTextLayoutInput::from_snapshot(block, text_layout_width_px, 1, 1)?;
    let mut code_spans = CodeSpansSource::NotCode;
    if matches!(block.kind, RichBlockKind::Code { .. }) {
        if block.marked_range.is_some() {
            code_spans = CodeSpansSource::SkippedMarked;
        } else {
            match code_highlights.spans(block.block_id, input.content_version) {
                Some(spans) => {
                    input.spans = spans;
                    code_spans = CodeSpansSource::Highlight;
                }
                None => code_spans = CodeSpansSource::Plain,
            }
        }
    }
    Some(DocumentBlockLayoutInput { input, code_spans })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use cditor_core::block::BlockChromeSnapshot;
    use cditor_core::layout::BlockLayoutMeta;
    use cditor_core::rich_text::{BlockAttrs, BlockPayload, BlockPayloadRecord, BlockPayloadView};

    use super::*;

    fn code_block(text: &str, marked: Option<std::ops::Range<usize>>) -> ViewBlockSnapshot {
        let kind = RichBlockKind::Code {
            language: Some("rust".to_owned()),
        };
        ViewBlockSnapshot {
            block_id: 1,
            visible_index: 0,
            depth: 0,
            chrome: BlockChromeSnapshot::plain(),
            kind: kind.clone(),
            attrs: BlockAttrs::default(),
            payload: BlockPayloadView::Loaded(Arc::new(BlockPayloadRecord {
                block_id: 1,
                content_version: 1,
                kind,
                payload: BlockPayload::Code {
                    language: Some("rust".to_owned()),
                    text: text.to_owned(),
                },
            })),
            layout: BlockLayoutMeta::new(1, 32.0),
            selected: false,
            selection_range: None,
            selection_overlay: false,
            focused: true,
            caret_offset: Some(text.len()),
            caret_affinity: None,
            marked_range: marked,
            table_view: None,
            focused_table_cell: None,
            focused_table_cell_offset: None,
            pinned: true,
            placeholder: false,
        }
    }

    fn spans_text(input: &RichTextLayoutInput) -> String {
        input.spans.iter().map(|span| span.text).collect()
    }

    #[test]
    fn an_active_composition_keeps_the_projected_preview_text() {
        // The projected payload already contains the IME preview ("nihao");
        // the highlight cache only knows the committed text. The composition
        // gate must win, for paint AND prewarm alike.
        let highlights = CodeHighlightCache::default();
        let built = document_block_layout_input(
            &code_block("fn mainnihao", Some(7..12)),
            766.0,
            &highlights,
        )
        .expect("code payload builds a layout input");
        assert_eq!(built.code_spans, CodeSpansSource::SkippedMarked);
        assert_eq!(spans_text(&built.input), "fn mainnihao");
    }

    #[test]
    fn committed_code_without_a_highlight_entry_renders_plain() {
        let highlights = CodeHighlightCache::default();
        let built = document_block_layout_input(&code_block("fn main", None), 766.0, &highlights)
            .expect("code payload builds a layout input");
        assert_eq!(built.code_spans, CodeSpansSource::Plain);
        assert_eq!(spans_text(&built.input), "fn main");
    }
}
