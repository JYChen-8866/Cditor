use super::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextAffinity {
    Upstream,
    Downstream,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextPosition {
    pub block_id: BlockId,
    pub offset: TextOffset,
    pub affinity: TextAffinity,
}

impl TextPosition {
    pub const fn downstream(block_id: BlockId, offset: TextOffset) -> Self {
        Self {
            block_id,
            offset,
            affinity: TextAffinity::Downstream,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentSelection {
    pub anchor: TextPosition,
    pub focus: TextPosition,
}

impl DocumentSelection {
    pub const fn caret(position: TextPosition) -> Self {
        Self {
            anchor: position,
            focus: position,
        }
    }

    pub const fn is_caret(&self) -> bool {
        self.anchor.block_id == self.focus.block_id && self.anchor.offset == self.focus.offset
    }

    pub fn normalize(
        self,
        index: &DocumentIndex,
    ) -> Result<NormalizedSelection, SelectionResolveError> {
        let anchor_index = index
            .index_of(self.anchor.block_id)
            .ok_or(SelectionResolveError::UnknownBlock(self.anchor.block_id))?;
        let focus_index = index
            .index_of(self.focus.block_id)
            .ok_or(SelectionResolveError::UnknownBlock(self.focus.block_id))?;

        let anchor_before_focus = anchor_index < focus_index
            || (anchor_index == focus_index && self.anchor.offset <= self.focus.offset);
        let (start, end, reversed) = if anchor_before_focus {
            (self.anchor, self.focus, false)
        } else {
            (self.focus, self.anchor, true)
        };

        Ok(NormalizedSelection {
            start,
            end,
            is_reversed: reversed,
        })
    }

    pub fn degrade_hidden_endpoints(
        self,
        document_index: &DocumentIndex,
        visible_index: &VisibleDocumentIndex,
    ) -> Self {
        Self {
            anchor: degrade_hidden_position(self.anchor, document_index, visible_index),
            focus: degrade_hidden_position(self.focus, document_index, visible_index),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NormalizedSelection {
    pub start: TextPosition,
    pub end: TextPosition,
    pub is_reversed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockSelectionFragment {
    pub block_id: BlockId,
    pub range: SelectionRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectionRange {
    Full,
    Partial(Range<usize>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessibilitySelectionProjection {
    pub selection: DocumentSelection,
    pub focused_block_id: BlockId,
    pub semantic_block_range: Range<usize>,
    pub hydrated_ui_entities_required: bool,
}

impl NormalizedSelection {
    pub fn visible_selection_fragments(
        &self,
        visible_blocks: Range<usize>,
        document_index: &DocumentIndex,
        visible_index: &VisibleDocumentIndex,
        block_text_len: impl Fn(BlockId) -> usize,
    ) -> Result<Vec<BlockSelectionFragment>, SelectionResolveError> {
        let start_doc_index = document_index
            .index_of(self.start.block_id)
            .ok_or(SelectionResolveError::UnknownBlock(self.start.block_id))?;
        let end_doc_index = document_index
            .index_of(self.end.block_id)
            .ok_or(SelectionResolveError::UnknownBlock(self.end.block_id))?;

        let mut fragments = Vec::new();
        let visible_end = visible_blocks.end.min(visible_index.total_visible_count());
        for visible_idx in visible_blocks.start..visible_end {
            let Some(block_id) = visible_index.id_at_visible_index(visible_idx) else {
                continue;
            };
            let Some(doc_index) = document_index.index_of(block_id) else {
                continue;
            };
            if doc_index < start_doc_index || doc_index > end_doc_index {
                continue;
            }

            let range = if self.start.block_id == self.end.block_id {
                SelectionRange::Partial(self.start.offset..self.end.offset)
            } else if block_id == self.start.block_id {
                SelectionRange::Partial(self.start.offset..block_text_len(block_id))
            } else if block_id == self.end.block_id {
                SelectionRange::Partial(0..self.end.offset)
            } else {
                SelectionRange::Full
            };
            fragments.push(BlockSelectionFragment { block_id, range });
        }
        Ok(fragments)
    }

    pub fn accessibility_projection(
        &self,
        document_index: &DocumentIndex,
        focused_block_id: BlockId,
        context_blocks: usize,
    ) -> Result<AccessibilitySelectionProjection, SelectionResolveError> {
        let start = document_index
            .index_of(self.start.block_id)
            .ok_or(SelectionResolveError::UnknownBlock(self.start.block_id))?;
        let end = document_index
            .index_of(self.end.block_id)
            .ok_or(SelectionResolveError::UnknownBlock(self.end.block_id))?;
        let focused = document_index
            .index_of(focused_block_id)
            .ok_or(SelectionResolveError::UnknownBlock(focused_block_id))?;
        let semantic_start = start.min(focused.saturating_sub(context_blocks));
        let semantic_end =
            (end + 1).max((focused + context_blocks + 1).min(document_index.total_count()));

        Ok(AccessibilitySelectionProjection {
            selection: DocumentSelection {
                anchor: self.start,
                focus: self.end,
            },
            focused_block_id,
            semantic_block_range: semantic_start..semantic_end,
            hydrated_ui_entities_required: false,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionResolveError {
    UnknownBlock(BlockId),
}

fn degrade_hidden_position(
    position: TextPosition,
    document_index: &DocumentIndex,
    visible_index: &VisibleDocumentIndex,
) -> TextPosition {
    if visible_index.is_visible(position.block_id) {
        return position;
    }
    let Some(target) = visible_index.resolve_scroll_target(document_index, position.block_id)
    else {
        return position;
    };
    TextPosition {
        block_id: target.target_block_id,
        offset: 0,
        affinity: position.affinity,
    }
}
