use super::ai::RuntimeAiSession;
use super::*;

#[derive(Debug)]
pub struct DocumentRuntime {
    pub document_id: DocumentId,
    pub index: DocumentIndex,
    pub visible_index: VisibleDocumentIndex,
    pub height_index: BlockHeightIndex,
    pub page_layout: PageLayoutIndex,
    pub scroll: VirtualScrollState,
    pub editing: Option<EditingSession>,
    pub payload_window: PayloadWindow,
    pub(super) table_runtimes: HashMap<BlockId, TableRuntime>,
    pub(super) table_horizontal_scroll_offsets: HashMap<BlockId, f32>,
    pub(super) text_models: HashMap<BlockId, PieceTableTextModel>,
    pub(super) selected_block_ids: HashSet<BlockId>,
    pub(super) list_projection_cache: ListProjectionCache,
    pub(super) document_selection: Option<DocumentSelection>,
    pub(super) ai_session: Option<RuntimeAiSession>,
    pub(super) next_ai_request_id: u64,
    pub(super) focused_text_selection: Option<FocusedTextSelection>,
    pub(super) focused_table_cell: Option<FocusedTableCell>,
    pub(super) undo_stacks: HashMap<BlockId, Vec<TextSnapshot>>,
    pub(super) redo_stacks: HashMap<BlockId, Vec<TextSnapshot>>,
    pub(super) structure_undo_stack: Vec<StructureMoveUndoStep>,
    pub(super) structure_redo_stack: Vec<StructureMoveUndoStep>,
    pub(super) paste_undo_stack: Vec<StructurePasteUndoStep>,
    pub(super) paste_redo_stack: Vec<StructurePasteUndoStep>,
    pub(super) undo_events: Vec<RuntimeUndoEvent>,
    pub(super) redo_events: Vec<RuntimeUndoEvent>,
    pub(super) pending_structure_transactions: Vec<EditTransaction>,
    pub(super) next_transaction_id: u64,
    pub(super) hot_path: SingleCharInputHotPath,
    pub(super) payload_window_generation: u64,
    pub(super) window_planner: WindowPlanner,
    pub(super) last_planned_scroll_top: f64,
    pub(super) window_plan_clock_ms: u64,
    pub(super) pending_measured_heights: HashMap<BlockId, PendingMeasuredHeight>,
    pub(super) layout_dirty: bool,
    pub(super) scrollbar_drag: Option<ScrollbarDragSession>,
    pub(super) last_successful_projection: Option<EditorViewProjection>,
    pub(super) demo_payload_count: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct PendingMeasuredHeight {
    pub(super) content_version: u64,
    pub(super) height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EnterSplitMode {
    InheritV1Kind,
    #[cfg_attr(not(test), allow(dead_code))]
    ForceParagraph,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct StructureMoveUndoStep {
    pub(super) block_id: BlockId,
    pub(super) old_parent_id: Option<BlockId>,
    pub(super) old_sibling_index: usize,
    pub(super) new_parent_id: Option<BlockId>,
    pub(super) new_sibling_index: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct StructurePasteUndoStep {
    pub(super) current_block_id: BlockId,
    pub(super) before_current_record: BlockIndexRecord,
    pub(super) before_current_payload: BlockPayloadRecord,
    pub(super) after_current_record: BlockIndexRecord,
    pub(super) after_current_payload: BlockPayloadRecord,
    pub(super) inserted_records: Vec<BlockIndexRecord>,
    pub(super) inserted_payloads: Vec<BlockPayloadRecord>,
    pub(super) deleted_records: Vec<BlockIndexRecord>,
    pub(super) deleted_payloads: Vec<BlockPayloadRecord>,
    pub(super) before_focus: Option<(BlockId, usize)>,
    pub(super) after_focus: Option<(BlockId, usize)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FocusedTableCell {
    pub(super) block_id: BlockId,
    pub(super) row: usize,
    pub(super) col: usize,
    pub(super) offset: usize,
    pub(super) selected_range_start: usize,
    pub(super) selected_range_end: usize,
    pub(super) selection_reversed: bool,
    pub(super) marked_range_start: Option<usize>,
    pub(super) marked_range_end: Option<usize>,
}

impl FocusedTableCell {
    pub(super) fn collapsed(block_id: BlockId, row: usize, col: usize, offset: usize) -> Self {
        Self {
            block_id,
            row,
            col,
            offset,
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
        self.offset = selected_range.end;
        self.selected_range_start = selected_range.start;
        self.selected_range_end = selected_range.end;
        self.selection_reversed = selection_reversed;
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
    StructureMove,
    StructurePaste,
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
    pub precision: cditor_editor::scroll::ScrollPrecision,
}
#[derive(Debug, Clone, PartialEq)]
pub(super) struct TextSnapshot {
    pub(super) kind: RichBlockKind,
    pub(super) payload: BlockPayload,
    pub(super) content_version: u64,
    pub(super) focused_table_cell: Option<FocusedTableCell>,
}
