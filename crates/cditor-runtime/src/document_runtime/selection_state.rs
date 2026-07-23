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
