use std::collections::VecDeque;
use std::ops::Range;
use std::path::PathBuf;

use crate::core::document::{BlockIndexRecord, DocumentIndex, VisibleDocumentIndex};
use crate::core::ids::BlockId;
use crate::editor::scroll::ScrollAnchor;
use unicode_segmentation::UnicodeSegmentation;

pub type TransactionId = u64;
pub type SnapshotId = u64;
pub type TextOffset = usize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct InternalTextOffset(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct PlatformUtf16Offset(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct GraphemeIndex(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BidiDirection {
    Ltr,
    Rtl,
    Neutral,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BidiRun {
    pub range: Range<InternalTextOffset>,
    pub direction: BidiDirection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextOffsetMap {
    text_len: usize,
    internal_to_utf16: Vec<(InternalTextOffset, PlatformUtf16Offset)>,
    utf16_to_internal: Vec<(PlatformUtf16Offset, InternalTextOffset)>,
    grapheme_boundaries: Vec<InternalTextOffset>,
    bidi_runs: Vec<BidiRun>,
}

impl TextOffsetMap {
    pub fn build(text: &str) -> Self {
        let mut internal_to_utf16 = vec![(InternalTextOffset(0), PlatformUtf16Offset(0))];
        let mut utf16_to_internal = vec![(PlatformUtf16Offset(0), InternalTextOffset(0))];
        let mut utf16 = 0;
        for (byte_index, ch) in text.char_indices() {
            utf16 += ch.len_utf16();
            let internal = InternalTextOffset(byte_index + ch.len_utf8());
            let platform = PlatformUtf16Offset(utf16);
            internal_to_utf16.push((internal, platform));
            utf16_to_internal.push((platform, internal));
        }

        let mut grapheme_boundaries = Vec::new();
        grapheme_boundaries.push(InternalTextOffset(0));
        for (byte_index, grapheme) in text.grapheme_indices(true) {
            let end = InternalTextOffset(byte_index + grapheme.len());
            if grapheme_boundaries.last().copied() != Some(end) {
                grapheme_boundaries.push(end);
            }
        }
        if grapheme_boundaries.last().copied() != Some(InternalTextOffset(text.len())) {
            grapheme_boundaries.push(InternalTextOffset(text.len()));
        }

        let bidi_runs = build_bidi_runs(text);

        Self {
            text_len: text.len(),
            internal_to_utf16,
            utf16_to_internal,
            grapheme_boundaries,
            bidi_runs,
        }
    }

    pub fn text_len(&self) -> usize {
        self.text_len
    }

    pub fn grapheme_boundaries(&self) -> &[InternalTextOffset] {
        &self.grapheme_boundaries
    }

    pub fn bidi_runs(&self) -> &[BidiRun] {
        &self.bidi_runs
    }

    pub fn internal_to_utf16(
        &self,
        offset: InternalTextOffset,
    ) -> Result<PlatformUtf16Offset, TextOffsetError> {
        self.internal_to_utf16
            .iter()
            .find_map(|(internal, platform)| (*internal == offset).then_some(*platform))
            .ok_or(TextOffsetError::InvalidInternalOffset(offset))
    }

    pub fn utf16_to_internal(
        &self,
        offset: PlatformUtf16Offset,
    ) -> Result<InternalTextOffset, TextOffsetError> {
        self.utf16_to_internal
            .iter()
            .find_map(|(platform, internal)| (*platform == offset).then_some(*internal))
            .ok_or(TextOffsetError::InvalidUtf16Offset(offset))
    }

    pub fn utf16_range_to_internal_range(
        &self,
        range: Range<PlatformUtf16Offset>,
    ) -> Result<Range<InternalTextOffset>, TextOffsetError> {
        let start = self.utf16_to_internal(range.start)?;
        let end = self.utf16_to_internal(range.end)?;
        self.validate_grapheme_range(start..end)?;
        Ok(start..end)
    }

    pub fn is_grapheme_boundary(&self, offset: InternalTextOffset) -> bool {
        self.grapheme_boundaries.binary_search(&offset).is_ok()
    }

    pub fn grapheme_index_of(
        &self,
        offset: InternalTextOffset,
    ) -> Result<GraphemeIndex, TextOffsetError> {
        self.grapheme_boundaries
            .binary_search(&offset)
            .map(GraphemeIndex)
            .map_err(|_| TextOffsetError::NotGraphemeBoundary(offset))
    }

    pub fn validate_grapheme_range(
        &self,
        range: Range<InternalTextOffset>,
    ) -> Result<(), TextOffsetError> {
        if range.start > range.end || range.end.0 > self.text_len {
            return Err(TextOffsetError::InvalidInternalRange(range));
        }
        if !self.is_grapheme_boundary(range.start) {
            return Err(TextOffsetError::NotGraphemeBoundary(range.start));
        }
        if !self.is_grapheme_boundary(range.end) {
            return Err(TextOffsetError::NotGraphemeBoundary(range.end));
        }
        Ok(())
    }

    pub fn previous_grapheme_boundary(
        &self,
        offset: InternalTextOffset,
    ) -> Option<InternalTextOffset> {
        self.grapheme_boundaries
            .iter()
            .copied()
            .rev()
            .find(|boundary| *boundary < offset)
    }

    pub fn next_grapheme_boundary(&self, offset: InternalTextOffset) -> Option<InternalTextOffset> {
        self.grapheme_boundaries
            .iter()
            .copied()
            .find(|boundary| *boundary > offset)
    }

    pub fn backspace_range(
        &self,
        caret: InternalTextOffset,
    ) -> Result<Option<Range<InternalTextOffset>>, TextOffsetError> {
        if !self.is_grapheme_boundary(caret) {
            return Err(TextOffsetError::NotGraphemeBoundary(caret));
        }
        Ok(self
            .previous_grapheme_boundary(caret)
            .map(|previous| previous..caret))
    }

    pub fn delete_range(
        &self,
        caret: InternalTextOffset,
    ) -> Result<Option<Range<InternalTextOffset>>, TextOffsetError> {
        if !self.is_grapheme_boundary(caret) {
            return Err(TextOffsetError::NotGraphemeBoundary(caret));
        }
        Ok(self.next_grapheme_boundary(caret).map(|next| caret..next))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextOffsetError {
    InvalidInternalOffset(InternalTextOffset),
    InvalidUtf16Offset(PlatformUtf16Offset),
    InvalidInternalRange(Range<InternalTextOffset>),
    NotGraphemeBoundary(InternalTextOffset),
}

fn build_bidi_runs(text: &str) -> Vec<BidiRun> {
    let mut runs = Vec::new();
    let mut current_direction: Option<BidiDirection> = None;
    let mut current_start = 0;
    let mut last_end = 0;

    for (byte_index, ch) in text.char_indices() {
        let direction = bidi_direction(ch);
        let end = byte_index + ch.len_utf8();
        if direction == BidiDirection::Neutral {
            last_end = end;
            continue;
        }
        match current_direction {
            None => {
                current_direction = Some(direction);
                current_start = byte_index;
            }
            Some(existing) if existing == direction => {}
            Some(existing) => {
                runs.push(BidiRun {
                    range: InternalTextOffset(current_start)..InternalTextOffset(byte_index),
                    direction: existing,
                });
                current_direction = Some(direction);
                current_start = byte_index;
            }
        }
        last_end = end;
    }

    if let Some(direction) = current_direction {
        runs.push(BidiRun {
            range: InternalTextOffset(current_start)..InternalTextOffset(last_end),
            direction,
        });
    }
    runs
}

fn bidi_direction(ch: char) -> BidiDirection {
    match ch as u32 {
        0x0590..=0x08FF | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF => BidiDirection::Rtl,
        value if char::from_u32(value).is_some_and(|c| c.is_alphabetic() || c.is_numeric()) => {
            BidiDirection::Ltr
        }
        _ => BidiDirection::Neutral,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAffinity {
    Upstream,
    Downstream,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq)]
pub enum EditOperation {
    InsertText {
        block_id: BlockId,
        offset: usize,
        text: String,
    },
    DeleteText {
        block_id: BlockId,
        range: Range<usize>,
    },
    SplitBlock {
        block_id: BlockId,
        offset: usize,
        new_block_id: BlockId,
    },
    MergeBlocks {
        previous: BlockId,
        current: BlockId,
    },
    InsertBlock {
        index: usize,
        block: BlockIndexRecord,
    },
    DeleteBlock {
        block_id: BlockId,
    },
    MoveBlock {
        block_id: BlockId,
        target_index: usize,
    },
    MoveBlockToParent {
        block_id: BlockId,
        parent_id: Option<BlockId>,
        sibling_index: usize,
    },
    SetBlockKind {
        block_id: BlockId,
        kind: u16,
    },
    InsertBlocks {
        index: usize,
        blocks: Vec<BlockIndexRecord>,
    },
    DeleteBlockRange {
        range: Range<usize>,
    },
    MoveBlockRange {
        range: Range<usize>,
        target_index: usize,
    },
}

impl EditOperation {
    pub fn affected_blocks(&self) -> Vec<BlockId> {
        match self {
            Self::InsertText { block_id, .. }
            | Self::DeleteText { block_id, .. }
            | Self::SplitBlock { block_id, .. }
            | Self::DeleteBlock { block_id }
            | Self::MoveBlock { block_id, .. }
            | Self::MoveBlockToParent { block_id, .. }
            | Self::SetBlockKind { block_id, .. } => vec![*block_id],
            Self::MergeBlocks { previous, current } => vec![*previous, *current],
            Self::InsertBlock { block, .. } => vec![block.id],
            Self::InsertBlocks { blocks, .. } => blocks.iter().map(|block| block.id).collect(),
            Self::DeleteBlockRange { .. } | Self::MoveBlockRange { .. } => Vec::new(),
        }
    }

    pub fn is_text_input(&self) -> bool {
        matches!(self, Self::InsertText { .. } | Self::DeleteText { .. })
    }

    pub fn is_structure_operation(&self) -> bool {
        !self.is_text_input()
    }

    pub fn validate_text_range(&self, offsets: &TextOffsetMap) -> Result<(), TextOffsetError> {
        match self {
            Self::InsertText { offset, .. } => offsets
                .validate_grapheme_range(InternalTextOffset(*offset)..InternalTextOffset(*offset)),
            Self::DeleteText { range, .. } => offsets.validate_grapheme_range(
                InternalTextOffset(range.start)..InternalTextOffset(range.end),
            ),
            Self::SplitBlock { offset, .. } => offsets
                .validate_grapheme_range(InternalTextOffset(*offset)..InternalTextOffset(*offset)),
            _ => Ok(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditTransactionKind {
    Typing,
    CompositionCommit,
    Paste,
    DragDrop,
    Format,
    ExplicitCommand,
    BlockStructureChange,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditTransaction {
    pub id: TransactionId,
    pub ops: Vec<EditOperation>,
    pub inverse_ops: Vec<EditOperation>,
    pub affected_blocks: Vec<BlockId>,
    pub before_selection: Option<DocumentSelection>,
    pub after_selection: Option<DocumentSelection>,
    pub before_anchor: Option<ScrollAnchor>,
    pub after_anchor: Option<ScrollAnchor>,
    pub timestamp: u64,
    pub kind: EditTransactionKind,
}

impl EditTransaction {
    pub fn new(
        id: TransactionId,
        kind: EditTransactionKind,
        timestamp: u64,
        ops: Vec<EditOperation>,
        inverse_ops: Vec<EditOperation>,
    ) -> Self {
        let mut affected_blocks = Vec::new();
        for block_id in ops.iter().flat_map(EditOperation::affected_blocks) {
            if !affected_blocks.contains(&block_id) {
                affected_blocks.push(block_id);
            }
        }
        for block_id in inverse_ops.iter().flat_map(EditOperation::affected_blocks) {
            if !affected_blocks.contains(&block_id) {
                affected_blocks.push(block_id);
            }
        }

        Self {
            id,
            ops,
            inverse_ops,
            affected_blocks,
            before_selection: None,
            after_selection: None,
            before_anchor: None,
            after_anchor: None,
            timestamp,
            kind,
        }
    }

    pub fn with_selection(
        mut self,
        before_selection: Option<DocumentSelection>,
        after_selection: Option<DocumentSelection>,
    ) -> Self {
        self.before_selection = before_selection;
        self.after_selection = after_selection;
        self
    }

    pub fn with_anchor(
        mut self,
        before_anchor: Option<ScrollAnchor>,
        after_anchor: Option<ScrollAnchor>,
    ) -> Self {
        self.before_anchor = before_anchor;
        self.after_anchor = after_anchor;
        self
    }

    pub fn insert_text(
        id: TransactionId,
        timestamp: u64,
        block_id: BlockId,
        offset: usize,
        text: impl Into<String>,
    ) -> Self {
        let text = text.into();
        let end = offset + text.len();
        Self::new(
            id,
            EditTransactionKind::Typing,
            timestamp,
            vec![EditOperation::InsertText {
                block_id,
                offset,
                text: text.clone(),
            }],
            vec![EditOperation::DeleteText {
                block_id,
                range: offset..end,
            }],
        )
    }

    pub fn paste_blocks(
        id: TransactionId,
        timestamp: u64,
        index: usize,
        blocks: Vec<BlockIndexRecord>,
    ) -> Self {
        let end = index + blocks.len();
        Self::new(
            id,
            EditTransactionKind::Paste,
            timestamp,
            vec![EditOperation::InsertBlocks { index, blocks }],
            vec![EditOperation::DeleteBlockRange { range: index..end }],
        )
    }

    pub fn inverse_transaction(&self, id: TransactionId, timestamp: u64) -> Self {
        Self {
            id,
            ops: self.inverse_ops.clone(),
            inverse_ops: self.ops.clone(),
            affected_blocks: self.affected_blocks.clone(),
            before_selection: self.after_selection,
            after_selection: self.before_selection,
            before_anchor: self.after_anchor,
            after_anchor: self.before_anchor,
            timestamp,
            kind: self.kind,
        }
    }

    pub fn requires_single_restore(&self) -> bool {
        self.before_selection.is_some()
            || self.after_selection.is_some()
            || self.before_anchor.is_some()
            || self.after_anchor.is_some()
    }

    fn can_merge_typing_with(&self, next: &Self, max_gap_ms: u64) -> bool {
        if self.kind != EditTransactionKind::Typing || next.kind != EditTransactionKind::Typing {
            return false;
        }
        if next.timestamp.saturating_sub(self.timestamp) > max_gap_ms {
            return false;
        }
        if self.after_selection != next.before_selection {
            return false;
        }
        let Some(EditOperation::InsertText {
            block_id: left_block,
            offset: left_offset,
            text: left_text,
        }) = self.ops.last()
        else {
            return false;
        };
        let Some(EditOperation::InsertText {
            block_id: right_block,
            offset: right_offset,
            ..
        }) = next.ops.first()
        else {
            return false;
        };
        left_block == right_block && *right_offset == *left_offset + left_text.len()
    }

    fn merge_typing(&mut self, next: EditTransaction) {
        self.ops.extend(next.ops);
        self.inverse_ops.splice(0..0, next.inverse_ops);
        self.timestamp = next.timestamp;
        self.after_selection = next.after_selection;
        self.after_anchor = next.after_anchor;
        for block_id in next.affected_blocks {
            if !self.affected_blocks.contains(&block_id) {
                self.affected_blocks.push(block_id);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UndoGroupBoundary {
    TimeGap,
    SelectionChange,
    CompositionCommit,
    ExplicitCommand,
    BlockStructureChange,
    Paste,
    DragDrop,
    Format,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NonUndoableEditEvent {
    HeightCorrection,
    SyntaxHighlight,
    FtsUpdate,
    CacheWrite,
    AsyncPersistenceCallback,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UndoPayload {
    InlineSmall(EditTransaction),
    BlockRangeSnapshot {
        snapshot_id: SnapshotId,
        block_count: usize,
    },
    ExternalTempBlob {
        path: PathBuf,
        checksum: String,
    },
}

impl UndoPayload {
    pub fn block_count(&self) -> usize {
        match self {
            Self::InlineSmall(transaction) => transaction
                .ops
                .iter()
                .map(|op| match op {
                    EditOperation::InsertBlocks { blocks, .. } => blocks.len(),
                    EditOperation::DeleteBlockRange { range } => range.len(),
                    _ => 0,
                })
                .max()
                .unwrap_or(0),
            Self::BlockRangeSnapshot { block_count, .. } => *block_count,
            Self::ExternalTempBlob { .. } => 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UndoGroupingPolicy {
    pub typing_merge_window_ms: u64,
    pub inline_block_snapshot_limit: usize,
}

impl Default for UndoGroupingPolicy {
    fn default() -> Self {
        Self {
            typing_merge_window_ms: 1_000,
            inline_block_snapshot_limit: 1_024,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct UndoStep {
    pub payload: UndoPayload,
    pub boundary: Option<UndoGroupBoundary>,
    pub selection_restore_count: u8,
    pub anchor_restore_count: u8,
}

impl UndoStep {
    pub fn inline_transaction(&self) -> Option<&EditTransaction> {
        match &self.payload {
            UndoPayload::InlineSmall(transaction) => Some(transaction),
            _ => None,
        }
    }

    pub fn restore_user_position_once(&mut self) -> bool {
        if self.selection_restore_count > 0 || self.anchor_restore_count > 0 {
            return false;
        }
        self.selection_restore_count = 1;
        self.anchor_restore_count = 1;
        true
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct UndoStack {
    undo: VecDeque<UndoStep>,
    redo: VecDeque<UndoStep>,
    policy: UndoGroupingPolicy,
    next_snapshot_id: SnapshotId,
}

impl UndoStack {
    pub fn new(policy: UndoGroupingPolicy) -> Self {
        Self {
            undo: VecDeque::new(),
            redo: VecDeque::new(),
            policy,
            next_snapshot_id: 1,
        }
    }

    pub fn record_transaction(
        &mut self,
        transaction: EditTransaction,
    ) -> Option<UndoGroupBoundary> {
        self.redo.clear();
        let boundary = self.boundary_for(&transaction);
        if boundary.is_none() {
            if let Some(previous) = self.undo.back_mut() {
                if let Some(previous_transaction) = match &mut previous.payload {
                    UndoPayload::InlineSmall(transaction) => Some(transaction),
                    _ => None,
                } {
                    if previous_transaction
                        .can_merge_typing_with(&transaction, self.policy.typing_merge_window_ms)
                    {
                        previous_transaction.merge_typing(transaction);
                        return None;
                    }
                }
            }
        }

        let payload = self.payload_for(transaction);
        self.undo.push_back(UndoStep {
            payload,
            boundary,
            selection_restore_count: 0,
            anchor_restore_count: 0,
        });
        boundary
    }

    pub fn record_non_undoable_event(&mut self, _event: NonUndoableEditEvent) {
        // Intentionally ignored: background layout/cache/FTS/persistence events are not user undo.
    }

    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }

    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }

    pub fn last_undo_step(&self) -> Option<&UndoStep> {
        self.undo.back()
    }

    pub fn pop_undo(&mut self) -> Option<UndoStep> {
        let step = self.undo.pop_back()?;
        self.redo.push_back(step.clone());
        Some(step)
    }

    fn boundary_for(&self, transaction: &EditTransaction) -> Option<UndoGroupBoundary> {
        match transaction.kind {
            EditTransactionKind::Typing => {
                let Some(previous) = self.undo.back().and_then(UndoStep::inline_transaction) else {
                    return None;
                };
                if previous.kind != EditTransactionKind::Typing {
                    return Some(UndoGroupBoundary::ExplicitCommand);
                }
                if transaction.timestamp.saturating_sub(previous.timestamp)
                    > self.policy.typing_merge_window_ms
                {
                    return Some(UndoGroupBoundary::TimeGap);
                }
                if previous.after_selection != transaction.before_selection {
                    return Some(UndoGroupBoundary::SelectionChange);
                }
                None
            }
            EditTransactionKind::CompositionCommit => Some(UndoGroupBoundary::CompositionCommit),
            EditTransactionKind::Paste => Some(UndoGroupBoundary::Paste),
            EditTransactionKind::DragDrop => Some(UndoGroupBoundary::DragDrop),
            EditTransactionKind::Format => Some(UndoGroupBoundary::Format),
            EditTransactionKind::ExplicitCommand => Some(UndoGroupBoundary::ExplicitCommand),
            EditTransactionKind::BlockStructureChange => {
                Some(UndoGroupBoundary::BlockStructureChange)
            }
        }
    }

    fn payload_for(&mut self, transaction: EditTransaction) -> UndoPayload {
        let touched_block_count = transaction
            .ops
            .iter()
            .map(|op| match op {
                EditOperation::InsertBlocks { blocks, .. } => blocks.len(),
                EditOperation::DeleteBlockRange { range } => range.len(),
                EditOperation::MoveBlockRange { range, .. } => range.len(),
                _ => 0,
            })
            .max()
            .unwrap_or(0);

        if touched_block_count > self.policy.inline_block_snapshot_limit {
            let snapshot_id = self.next_snapshot_id;
            self.next_snapshot_id = self.next_snapshot_id.saturating_add(1);
            UndoPayload::BlockRangeSnapshot {
                snapshot_id,
                block_count: touched_block_count,
            }
        } else {
            UndoPayload::InlineSmall(transaction)
        }
    }
}

impl Default for UndoStack {
    fn default() -> Self {
        Self::new(UndoGroupingPolicy::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consecutive_typing_merges_by_time_and_selection_continuity() {
        let mut undo = UndoStack::default();
        let first = typing_tx(1, 0, 42, 0, "a");
        let second = typing_tx(2, 100, 42, 1, "b");

        assert_eq!(undo.record_transaction(first), None);
        assert_eq!(undo.record_transaction(second), None);

        assert_eq!(undo.undo_len(), 1);
        let step = undo.last_undo_step().unwrap();
        let tx = step.inline_transaction().unwrap();
        assert_eq!(tx.ops.len(), 2);
        assert_eq!(tx.inverse_ops.len(), 2);
        assert_eq!(tx.before_selection, Some(selection(42, 0)));
        assert_eq!(tx.after_selection, Some(selection(42, 2)));
    }

    #[test]
    fn selection_change_and_time_gap_create_boundaries() {
        let mut undo = UndoStack::default();
        undo.record_transaction(typing_tx(1, 0, 42, 0, "a"));

        let mut selection_jump = typing_tx(2, 100, 42, 10, "b");
        selection_jump.before_selection = Some(selection(42, 99));
        assert_eq!(
            undo.record_transaction(selection_jump),
            Some(UndoGroupBoundary::SelectionChange)
        );

        let gap = typing_tx(3, 5_000, 42, 11, "c");
        assert_eq!(
            undo.record_transaction(gap),
            Some(UndoGroupBoundary::TimeGap)
        );
        assert_eq!(undo.undo_len(), 3);
    }

    #[test]
    fn composition_commit_is_independent_undo_step() {
        let mut undo = UndoStack::default();
        let mut ime = EditTransaction::insert_text(1, 10, 42, 0, "你好")
            .with_selection(Some(selection(42, 0)), Some(selection(42, 6)));
        ime.kind = EditTransactionKind::CompositionCommit;

        assert_eq!(
            undo.record_transaction(ime),
            Some(UndoGroupBoundary::CompositionCommit)
        );
        assert_eq!(undo.undo_len(), 1);
    }

    #[test]
    fn paste_10k_blocks_uses_snapshot_payload_instead_of_inline_blocks() {
        let mut undo = UndoStack::default();
        let blocks = (0..10_000)
            .map(|index| BlockIndexRecord::new(index as BlockId + 1, None, 0, 1, 0))
            .collect::<Vec<_>>();
        let tx = EditTransaction::paste_blocks(1, 0, 0, blocks);

        assert_eq!(undo.record_transaction(tx), Some(UndoGroupBoundary::Paste));

        let step = undo.last_undo_step().unwrap();
        assert!(matches!(
            step.payload,
            UndoPayload::BlockRangeSnapshot {
                snapshot_id: 1,
                block_count: 10_000
            }
        ));
    }

    #[test]
    fn delete_50k_blocks_undo_does_not_hold_inline_payload() {
        let mut undo = UndoStack::default();
        let tx = EditTransaction::new(
            1,
            EditTransactionKind::BlockStructureChange,
            0,
            vec![EditOperation::DeleteBlockRange { range: 0..50_000 }],
            vec![EditOperation::InsertBlocks {
                index: 0,
                blocks: Vec::new(),
            }],
        );

        assert_eq!(
            undo.record_transaction(tx),
            Some(UndoGroupBoundary::BlockStructureChange)
        );

        let step = undo.last_undo_step().unwrap();
        assert!(matches!(
            step.payload,
            UndoPayload::BlockRangeSnapshot {
                snapshot_id: 1,
                block_count: 50_000
            }
        ));
    }

    #[test]
    fn inverse_transaction_restores_selection_and_anchor_once() {
        let anchor_before = ScrollAnchor {
            block_id: 42,
            offset_in_block: 10.0,
            viewport_y: 100.0,
        };
        let anchor_after = ScrollAnchor {
            block_id: 42,
            offset_in_block: 20.0,
            viewport_y: 100.0,
        };
        let tx = EditTransaction::insert_text(1, 0, 42, 0, "a")
            .with_selection(Some(selection(42, 0)), Some(selection(42, 1)))
            .with_anchor(Some(anchor_before), Some(anchor_after));

        let inverse = tx.inverse_transaction(2, 10);

        assert_eq!(inverse.ops, tx.inverse_ops);
        assert_eq!(inverse.inverse_ops, tx.ops);
        assert_eq!(inverse.before_selection, tx.after_selection);
        assert_eq!(inverse.after_selection, tx.before_selection);
        assert_eq!(inverse.before_anchor, tx.after_anchor);
        assert_eq!(inverse.after_anchor, tx.before_anchor);

        let mut step = UndoStep {
            payload: UndoPayload::InlineSmall(tx),
            boundary: None,
            selection_restore_count: 0,
            anchor_restore_count: 0,
        };
        assert!(step.restore_user_position_once());
        assert!(!step.restore_user_position_once());
    }

    #[test]
    fn background_events_never_enter_undo_stack() {
        let mut undo = UndoStack::default();

        undo.record_non_undoable_event(NonUndoableEditEvent::HeightCorrection);
        undo.record_non_undoable_event(NonUndoableEditEvent::SyntaxHighlight);
        undo.record_non_undoable_event(NonUndoableEditEvent::FtsUpdate);
        undo.record_non_undoable_event(NonUndoableEditEvent::CacheWrite);

        assert_eq!(undo.undo_len(), 0);
    }

    #[test]
    fn text_offset_map_handles_emoji_zwj_as_single_grapheme() {
        let text = "a👨‍👩‍👧‍👦b";
        let map = TextOffsetMap::build(text);
        let emoji_start = InternalTextOffset(1);
        let emoji_end = InternalTextOffset(text.len() - 1);

        assert!(map.is_grapheme_boundary(emoji_start));
        assert!(map.is_grapheme_boundary(emoji_end));
        assert_eq!(
            map.backspace_range(emoji_end).unwrap(),
            Some(emoji_start..emoji_end)
        );
        assert_eq!(
            map.delete_range(emoji_start).unwrap(),
            Some(emoji_start..emoji_end)
        );
        assert!(
            EditOperation::DeleteText {
                block_id: 42,
                range: emoji_start.0..emoji_end.0,
            }
            .validate_text_range(&map)
            .is_ok()
        );
    }

    #[test]
    fn text_offset_map_rejects_combining_mark_middle_boundary() {
        let text = "e\u{301}x";
        let map = TextOffsetMap::build(text);
        let middle_of_grapheme = InternalTextOffset("e".len());
        let first_cluster_end = InternalTextOffset("e\u{301}".len());

        assert!(!map.is_grapheme_boundary(middle_of_grapheme));
        assert_eq!(
            map.backspace_range(first_cluster_end).unwrap(),
            Some(InternalTextOffset(0)..first_cluster_end)
        );
        assert_eq!(
            EditOperation::DeleteText {
                block_id: 42,
                range: 0..middle_of_grapheme.0,
            }
            .validate_text_range(&map),
            Err(TextOffsetError::NotGraphemeBoundary(middle_of_grapheme))
        );
    }

    #[test]
    fn cjk_internal_and_utf16_offsets_match_at_char_boundaries() {
        let text = "你好吗";
        let map = TextOffsetMap::build(text);

        assert_eq!(
            map.internal_to_utf16(InternalTextOffset("你".len()))
                .unwrap(),
            PlatformUtf16Offset(1)
        );
        assert_eq!(
            map.utf16_to_internal(PlatformUtf16Offset(2)).unwrap(),
            InternalTextOffset("你好".len())
        );
        assert_eq!(
            map.grapheme_index_of(InternalTextOffset("你好".len()))
                .unwrap(),
            GraphemeIndex(2)
        );
    }

    #[test]
    fn rtl_ltr_mixed_text_builds_bidi_runs() {
        let text = "abc שלום def";
        let map = TextOffsetMap::build(text);

        assert!(
            map.bidi_runs()
                .iter()
                .any(|run| run.direction == BidiDirection::Ltr)
        );
        assert!(
            map.bidi_runs()
                .iter()
                .any(|run| run.direction == BidiDirection::Rtl)
        );
    }

    #[test]
    fn ime_marked_range_converts_from_utf16_to_internal_grapheme_range() {
        let text = "a😀中";
        let map = TextOffsetMap::build(text);

        let range = map
            .utf16_range_to_internal_range(PlatformUtf16Offset(1)..PlatformUtf16Offset(3))
            .unwrap();

        assert_eq!(
            range,
            InternalTextOffset(1)..InternalTextOffset("a😀".len())
        );
        assert_eq!(
            map.utf16_to_internal(PlatformUtf16Offset(2)),
            Err(TextOffsetError::InvalidUtf16Offset(PlatformUtf16Offset(2)))
        );
    }

    #[test]
    fn reversed_anchor_focus_normalizes_by_document_order_and_offset() {
        let index = document_index(5);
        let selection = DocumentSelection {
            anchor: TextPosition::downstream(4, 1),
            focus: TextPosition::downstream(2, 3),
        };

        let normalized = selection.normalize(&index).unwrap();

        assert_eq!(normalized.start.block_id, 2);
        assert_eq!(normalized.end.block_id, 4);
        assert!(normalized.is_reversed);
    }

    #[test]
    fn cross_page_selection_fragments_only_current_visible_window() {
        let index = document_index(100);
        let visible = VisibleDocumentIndex::from_document_index(&index);
        let selection = DocumentSelection {
            anchor: TextPosition::downstream(10, 2),
            focus: TextPosition::downstream(90, 5),
        }
        .normalize(&index)
        .unwrap();

        let fragments = selection
            .visible_selection_fragments(30..35, &index, &visible, |_| 10)
            .unwrap();

        assert_eq!(fragments.len(), 5);
        assert!(
            fragments
                .iter()
                .all(|fragment| fragment.range == SelectionRange::Full)
        );
        assert_eq!(fragments[0].block_id, 31);
        assert_eq!(fragments[4].block_id, 35);
    }

    #[test]
    fn start_and_end_blocks_get_partial_fragments() {
        let index = document_index(5);
        let visible = VisibleDocumentIndex::from_document_index(&index);
        let selection = DocumentSelection {
            anchor: TextPosition::downstream(2, 3),
            focus: TextPosition::downstream(4, 1),
        }
        .normalize(&index)
        .unwrap();

        let fragments = selection
            .visible_selection_fragments(0..5, &index, &visible, |_| 10)
            .unwrap();

        assert_eq!(
            fragments[0],
            BlockSelectionFragment {
                block_id: 2,
                range: SelectionRange::Partial(3..10),
            }
        );
        assert_eq!(
            fragments[1],
            BlockSelectionFragment {
                block_id: 3,
                range: SelectionRange::Full,
            }
        );
        assert_eq!(
            fragments[2],
            BlockSelectionFragment {
                block_id: 4,
                range: SelectionRange::Partial(0..1),
            }
        );
    }

    #[test]
    fn hidden_subtree_selection_degrades_endpoint_to_visible_ancestor() {
        use std::collections::HashSet;

        let records = vec![
            BlockIndexRecord::new(1, None, 0, 1, 0),
            BlockIndexRecord::new(2, Some(1), 1, 1, 0),
            BlockIndexRecord::new(3, Some(2), 2, 1, 0),
            BlockIndexRecord::new(4, None, 0, 1, 0),
        ];
        let index = DocumentIndex::new(1, records, 1).unwrap();
        let visible = VisibleDocumentIndex::with_folded_blocks(&index, HashSet::from([2]), 1);
        let selection = DocumentSelection {
            anchor: TextPosition::downstream(3, 7),
            focus: TextPosition::downstream(4, 1),
        };

        let degraded = selection.degrade_hidden_endpoints(&index, &visible);

        assert_eq!(degraded.anchor.block_id, 2);
        assert_eq!(degraded.anchor.offset, 0);
        assert_eq!(degraded.focus.block_id, 4);
    }

    #[test]
    fn accessibility_projection_does_not_require_ui_entity_hydration() {
        let index = document_index(100);
        let normalized = DocumentSelection {
            anchor: TextPosition::downstream(20, 0),
            focus: TextPosition::downstream(80, 0),
        }
        .normalize(&index)
        .unwrap();

        let projection = normalized.accessibility_projection(&index, 50, 2).unwrap();

        assert_eq!(projection.focused_block_id, 50);
        assert_eq!(projection.semantic_block_range, 19..80);
        assert!(!projection.hydrated_ui_entities_required);
    }

    fn typing_tx(
        id: TransactionId,
        timestamp: u64,
        block_id: BlockId,
        offset: usize,
        text: &str,
    ) -> EditTransaction {
        EditTransaction::insert_text(id, timestamp, block_id, offset, text).with_selection(
            Some(selection(block_id, offset)),
            Some(selection(block_id, offset + text.len())),
        )
    }

    fn selection(block_id: BlockId, offset: usize) -> DocumentSelection {
        DocumentSelection::caret(TextPosition::downstream(block_id, offset))
    }

    fn document_index(count: usize) -> DocumentIndex {
        let records = (0..count)
            .map(|index| BlockIndexRecord::new(index as BlockId + 1, None, 0, 1, 0))
            .collect::<Vec<_>>();
        DocumentIndex::new(1, records, 1).unwrap()
    }
}
