use cditor_core::edit::{ChangeOrigin, TableEditOperation};

use super::*;

#[test]
fn table_resize_commits_one_typed_drag_transaction() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);
    let revision_before = runtime.revision();
    let content_before = runtime.block_content_version(10).unwrap();
    let table_index = runtime.document.index.index_of(10).unwrap();
    let layout_before = runtime.document.index.layout_meta[table_index].layout_version;
    assert!(
        runtime
            .set_table_column_width(10, 0, TableTrackSize::Px(180))
            .unwrap()
    );
    assert_eq!(runtime.revision(), revision_before + 1);
    assert_eq!(runtime.block_content_version(10), Some(content_before + 1));
    assert_eq!(
        runtime.document.index.layout_meta[table_index].layout_version,
        layout_before + 1
    );
    let transactions = runtime.drain_pending_structure_transactions();
    assert_eq!(transactions.len(), 1);
    assert_eq!(transactions[0].kind, EditTransactionKind::DragDrop);
    assert_eq!(transactions[0].origin, ChangeOrigin::User);
    assert!(matches!(
        transactions[0].ops.as_slice(),
        [EditOperation::Table(TableEditOperation::ResizeColumn {
            block_id: 10,
            column: 0,
            old_width: TableTrackSize::Auto,
            new_width: TableTrackSize::Px(180),
        })]
    ));
}

#[test]
fn stale_table_resize_old_value_is_rejected_atomically() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);
    assert!(
        runtime
            .set_table_column_width(10, 0, TableTrackSize::Px(160))
            .unwrap()
    );
    let revision_before = runtime.revision();
    let transaction = EditTransaction::new(
        900,
        EditTransactionKind::DragDrop,
        900,
        vec![EditOperation::Table(TableEditOperation::ResizeColumn {
            block_id: 10,
            column: 0,
            old_width: TableTrackSize::Auto,
            new_width: TableTrackSize::Px(220),
        })],
        Vec::new(),
    );
    let error = runtime
        .apply_external_transaction(&transaction, ChangeOrigin::Remote)
        .expect_err("stale old width must be rejected");
    assert!(matches!(
        error,
        TransactionApplyError::Precondition { op_index: 0, .. }
    ));
    assert_eq!(runtime.revision(), revision_before);
    let BlockPayload::Table(table) = runtime.block_payload_record(10).unwrap().payload else {
        panic!("table payload");
    };
    assert_eq!(table.columns[0].width, TableTrackSize::Px(160));
}

#[test]
fn table_reorder_transaction_remaps_inner_focus_through_undo_redo() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);
    runtime.focus_table_cell_at_offset(10, 0, 0, 1).unwrap();
    assert!(runtime.move_table_row(10, 0, 1).unwrap());
    assert_eq!(
        runtime.focused_table_cell_for_block(10),
        Some(TableCellPosition { row: 1, col: 0 })
    );
    let transactions = runtime.drain_pending_structure_transactions();
    assert_eq!(transactions.len(), 1);
    assert!(matches!(
        transactions[0].ops.as_slice(),
        [EditOperation::Table(TableEditOperation::MoveRows {
            from: 0,
            to: 1,
            count: 1,
            ..
        })]
    ));

    assert!(runtime.undo_focused_block().unwrap());
    assert_eq!(
        runtime.focused_table_cell_for_block(10),
        Some(TableCellPosition { row: 0, col: 0 })
    );
    assert!(runtime.redo_focused_block().unwrap());
    assert_eq!(
        runtime.focused_table_cell_for_block(10),
        Some(TableCellPosition { row: 1, col: 0 })
    );
}

#[test]
fn table_clipboard_paste_is_one_import_transaction() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);
    runtime.focus_table_cell_at_offset(10, 1, 1, 1).unwrap();
    let source = runtime.table_clipboard_for_whole_table(10).unwrap();
    assert!(
        runtime
            .paste_table_clipboard_at_focused_cell(&source)
            .unwrap()
    );
    let transactions = runtime.drain_pending_structure_transactions();
    assert_eq!(transactions.len(), 1);
    assert_eq!(transactions[0].kind, EditTransactionKind::Paste);
    assert_eq!(transactions[0].origin, ChangeOrigin::Import);
    assert!(matches!(
        transactions[0].ops.as_slice(),
        [EditOperation::Block(
            cditor_core::edit::BlockEditOperation::ReplacePayload { block_id: 10, .. }
        )]
    ));
}
