use super::*;

/// Canonical document, block, and nested-surface selection state.
///
/// Editing session ownership remains separate until R4-003. This state records
/// semantic selection only and is observed through Runtime queries/projection.
#[derive(Debug, Default)]
pub(super) struct SelectionState {
    pub(super) selected_block_ids: HashSet<BlockId>,
    pub(super) document_selection: Option<DocumentSelection>,
    pub(super) visual_caret_position: Option<VisualCaretPosition>,
    pub(super) focused_text_selection: Option<FocusedTextSelection>,
    pub(super) focused_table_cell: Option<FocusedTableCell>,
    pub(super) focused_inner_selection: Option<FocusedInnerSelection>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct VisualCaretPosition {
    pub(super) position: TextPosition,
    pub(super) content_version: u64,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extraction_preserves_reversed_text_selection_without_document_mutation() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord::rich_text(
                1,
                RichBlockKind::Paragraph,
                "abcdef",
            )],
            720.0,
        );
        runtime.focus_block_at_offset(1, 1).unwrap();
        let revision = runtime.revision();
        let transaction = runtime.last_committed_transaction_id();

        runtime.set_document_text_selection(1, 5, 1, 2).unwrap();

        assert_eq!(runtime.input_session_selected_range(), Some(2..5));
        assert!(runtime.input_session_selection_reversed());
        assert_eq!(runtime.selected_document_text().as_deref(), Some("cde"));
        assert_eq!(runtime.revision(), revision);
        assert_eq!(runtime.last_committed_transaction_id(), transaction);
    }

    #[test]
    fn extraction_preserves_block_range_as_selection_only_state() {
        let mut runtime = DocumentRuntime::demo();
        let revision = runtime.revision();
        let before_payloads = runtime.loaded_payload_records_snapshot();

        assert!(runtime.select_visible_block_range(1, 3));

        assert!(runtime.has_selected_blocks());
        assert_eq!(runtime.selected_block_ids_snapshot(), vec![1, 2, 3]);
        assert_eq!(runtime.revision(), revision);
        assert_eq!(runtime.loaded_payload_records_snapshot(), before_payloads);
    }
}
