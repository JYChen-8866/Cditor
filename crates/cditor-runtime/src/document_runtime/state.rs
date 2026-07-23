use super::ai::RuntimeAiSession;
use super::document_state::DocumentState;
use super::editing_state::EditingState;
use super::selection_state::SelectionState;
use super::*;

#[derive(Debug)]
pub struct DocumentRuntime {
    pub document_id: DocumentId,
    pub(super) document: DocumentState,
    pub height_index: BlockHeightIndex,
    pub page_layout: PageLayoutIndex,
    pub scroll: VirtualScrollState,
    pub(super) editing: EditingState,
    pub(super) table_horizontal_scroll_offsets: HashMap<BlockId, f32>,
    pub(super) selection: SelectionState,
    pub(super) ai_session: Option<RuntimeAiSession>,
    pub(super) next_ai_request_id: u64,
    pub(super) undo_stacks: HashMap<BlockId, Vec<TextSnapshot>>,
    pub(super) redo_stacks: HashMap<BlockId, Vec<TextSnapshot>>,
    pub(super) external_undo_stack: UndoStack,
    pub(super) typing_undo_group: Option<TypingUndoGroup>,
    pub(super) pending_typing_undo: Option<TypingUndoRequest>,
    pub(super) undo_events: Vec<RuntimeUndoEvent>,
    pub(super) redo_events: Vec<RuntimeUndoEvent>,
    pub(super) pending_structure_transactions: Vec<EditTransaction>,
    pub(super) last_committed_transaction_id: Option<u64>,
    pub(super) next_transaction_id: u64,
    pub(super) payload_window_generation: u64,
    pub(super) window_planner: WindowPlanner,
    pub(super) last_planned_scroll_top: f64,
    pub(super) window_plan_clock_ms: u64,
    pub(super) window_memory_pressure: WindowMemoryPressure,
    pub(super) pending_measured_heights: HashMap<BlockId, PendingMeasuredHeight>,
    pub(super) layout_dirty: bool,
    pub(super) scrollbar_drag: Option<ScrollbarDragSession>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct PendingMeasuredHeight {
    pub(super) content_version: u64,
    pub(super) height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct VisualCaretPosition {
    pub(super) position: TextPosition,
    pub(super) content_version: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct TypingUndoGroup {
    pub(super) surface_id: cditor_core::ids::SurfaceId,
    pub(super) next_offset: usize,
    pub(super) last_input_at: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct TypingUndoRequest {
    pub(super) surface_id: cditor_core::ids::SurfaceId,
    pub(super) offset: usize,
    pub(super) started_at: Instant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TypingMarkOverride {
    pub(super) surface_id: SurfaceId,
    pub(super) offset: usize,
    pub(super) marks: Vec<InlineMark>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EnterSplitMode {
    InheritV1Kind,
    #[cfg_attr(not(test), allow(dead_code))]
    ForceParagraph,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FocusedTableCell {
    pub(super) block_id: BlockId,
    pub(super) row: usize,
    pub(super) col: usize,
    pub(super) offset: usize,
    pub(super) affinity: TextAffinity,
    pub(super) selected_range_start: usize,
    pub(super) selected_range_end: usize,
    pub(super) selection_reversed: bool,
    pub(super) marked_range_start: Option<usize>,
    pub(super) marked_range_end: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FocusedInnerSelection {
    pub(super) block_id: BlockId,
    pub(super) anchor: InnerSelectionAnchor,
    pub(super) focus: InnerSelectionAnchor,
}

impl FocusedTableCell {
    pub(super) fn collapsed(block_id: BlockId, row: usize, col: usize, offset: usize) -> Self {
        Self {
            block_id,
            row,
            col,
            offset,
            affinity: TextAffinity::Downstream,
            selected_range_start: offset,
            selected_range_end: offset,
            selection_reversed: false,
            marked_range_start: None,
            marked_range_end: None,
        }
    }

    pub(super) fn selected_range(self) -> Range<usize> {
        self.selected_range_start..self.selected_range_end
    }

    pub(super) fn marked_range(self) -> Option<Range<usize>> {
        Some(self.marked_range_start?..self.marked_range_end?)
    }

    pub(super) fn with_selected_range(
        mut self,
        selected_range: Range<usize>,
        selection_reversed: bool,
    ) -> Self {
        self.offset = if selection_reversed {
            selected_range.start
        } else {
            selected_range.end
        };
        self.selected_range_start = selected_range.start;
        self.selected_range_end = selected_range.end;
        self.selection_reversed = selection_reversed;
        self.affinity = TextAffinity::Downstream;
        self
    }

    pub(super) fn with_affinity(mut self, affinity: TextAffinity) -> Self {
        self.affinity = affinity;
        self
    }

    pub(super) fn with_marked_range(mut self, marked_range: Option<Range<usize>>) -> Self {
        self.marked_range_start = marked_range.as_ref().map(|range| range.start);
        self.marked_range_end = marked_range.as_ref().map(|range| range.end);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RuntimeUndoEvent {
    Text(BlockId),
    ExternalTransaction,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlobalScrollTarget {
    pub global_scroll_top: f64,
    pub block_index: usize,
    pub block_id: BlockId,
    pub block_top: f64,
    pub offset_in_block: f64,
    pub page_index: usize,
    pub page_top: f64,
    pub offset_in_page: f64,
    pub precision: cditor_viewport::scroll::ScrollPrecision,
}
#[derive(Debug, Clone, PartialEq)]
pub(super) struct TextSnapshot {
    pub(super) kind: RichBlockKind,
    pub(super) payload: BlockPayload,
    pub(super) content_version: u64,
    pub(super) focused_table_cell: Option<FocusedTableCell>,
    pub(super) input_target: Option<InputTarget>,
    pub(super) selected_range: Option<Range<usize>>,
    pub(super) selection_reversed: bool,
    pub(super) scroll: UndoScrollSnapshot,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct UndoScrollSnapshot {
    pub(super) anchor: Option<cditor_core::edit::ScrollAnchor>,
    pub(super) fallback_global_scroll_top: f64,
}
