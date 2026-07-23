use super::*;

/// Ordered text and transaction history owned by the document runtime.
///
/// Text snapshots are stored per block, while `undo_events` and `redo_events`
/// preserve one chronological timeline across text and external transactions.
#[derive(Debug, Default)]
pub(super) struct HistoryState {
    pub(super) undo_stacks: HashMap<BlockId, Vec<TextSnapshot>>,
    pub(super) redo_stacks: HashMap<BlockId, Vec<TextSnapshot>>,
    pub(super) external_undo_stack: UndoStack,
    pub(super) typing_undo_group: Option<TypingUndoGroup>,
    pub(super) pending_typing_undo: Option<TypingUndoRequest>,
    pub(super) undo_events: Vec<RuntimeUndoEvent>,
    pub(super) redo_events: Vec<RuntimeUndoEvent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct TypingUndoGroup {
    pub(super) surface_id: SurfaceId,
    pub(super) next_offset: usize,
    pub(super) last_input_at: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct TypingUndoRequest {
    pub(super) surface_id: SurfaceId,
    pub(super) offset: usize,
    pub(super) started_at: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RuntimeUndoEvent {
    Text(BlockId),
    ExternalTransaction,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extraction_preserves_typing_coalescing_and_redo_invalidation() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord::rich_text(
                1,
                RichBlockKind::Paragraph,
                "",
            )],
            720.0,
        );
        runtime.focus_block_at_offset(1, 0).unwrap();

        runtime.insert_char('a').unwrap();
        runtime.insert_char('b').unwrap();
        assert_eq!(runtime.history.undo_events.len(), 1);
        assert!(runtime.history.typing_undo_group.is_some());

        assert!(runtime.undo_focused_block().unwrap());
        assert_eq!(runtime.block_payload_record(1).unwrap().plain_text(), "");
        assert_eq!(runtime.history.redo_events.len(), 1);

        runtime.insert_char('x').unwrap();
        assert!(runtime.history.redo_events.is_empty());
        assert!(runtime.history.redo_stacks.is_empty());
        assert!(!runtime.can_redo());
        assert!(
            runtime.estimated_text_undo_memory_bytes() <= runtime.text_undo_memory_budget_bytes()
        );
    }
}
