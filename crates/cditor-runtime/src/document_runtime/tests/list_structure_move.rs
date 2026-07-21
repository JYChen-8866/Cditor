use super::*;

#[test]
fn move_block_subtree_before_moves_children_and_preserves_total_height() {
    let mut runtime = runtime_with_kind_depths(vec![
        (RichBlockKind::NumberedList, 0, None),
        (RichBlockKind::Todo { checked: false }, 1, Some(1)),
        (RichBlockKind::NumberedList, 0, None),
        (RichBlockKind::NumberedList, 0, None),
    ]);
    let total_height = runtime.height_index.total_height();
    let before_version = runtime.index.structure_version;

    assert!(runtime.move_block_subtree_before(1, Some(4)).unwrap());

    assert_eq!(runtime.index.structure_version, before_version + 1);
    assert_eq!(runtime.index.block_ids, vec![3, 1, 2, 4]);
    assert_eq!(runtime.index.parent_ids[1], None);
    assert_eq!(runtime.index.parent_ids[2], Some(1));
    assert_eq!(runtime.index.depths[1], 0);
    assert_eq!(runtime.index.depths[2], 1);
    assert_eq!(runtime.height_index.total_height(), total_height);
    let projection = runtime.projection();
    assert_eq!(
        projection.blocks[0].chrome.prefix,
        BlockPrefixSnapshot::Number { ordinal: 1 }
    );
    assert_eq!(
        projection.blocks[1].chrome.prefix,
        BlockPrefixSnapshot::Number { ordinal: 2 }
    );
    assert_eq!(
        projection.blocks[3].chrome.prefix,
        BlockPrefixSnapshot::Number { ordinal: 3 }
    );
}

#[test]
fn move_block_subtree_commit_preserves_scroll_top_and_total_height() {
    let mut runtime = runtime_with_kind_depths(vec![
        (RichBlockKind::NumberedList, 0, None),
        (RichBlockKind::Todo { checked: false }, 1, Some(1)),
        (RichBlockKind::NumberedList, 0, None),
        (RichBlockKind::NumberedList, 0, None),
        (RichBlockKind::BulletedList, 0, None),
    ]);
    runtime
        .scroll
        .scroll_to_global_offset(96.0, cditor_editor::scroll::ScrollOrigin::UserWheel)
        .unwrap();
    let before_scroll_top = runtime.scroll.global_scroll_top;
    let before_total_height = runtime.height_index.total_height();

    assert!(runtime.move_block_subtree_before(1, Some(4)).unwrap());

    assert_eq!(runtime.scroll.global_scroll_top, before_scroll_top);
    assert_eq!(runtime.height_index.total_height(), before_total_height);
    assert_eq!(
        runtime.scroll.model_total_height,
        runtime.scroll_extent_height(before_total_height)
    );
    assert_eq!(
        runtime.scroll.displayed_total_height,
        runtime.scroll_extent_height(before_total_height)
    );
}

#[test]
fn move_block_subtree_to_parent_reparents_and_updates_depth_delta() {
    let mut runtime = runtime_with_kind_depths(vec![
        (RichBlockKind::BulletedList, 0, None),
        (RichBlockKind::Paragraph, 0, None),
        (RichBlockKind::Todo { checked: false }, 1, Some(2)),
    ]);
    let total_height = runtime.height_index.total_height();

    assert!(runtime.move_block_subtree_to_parent(2, Some(1), 0).unwrap());

    assert_eq!(runtime.index.block_ids, vec![1, 2, 3]);
    assert_eq!(runtime.index.parent_ids[1], Some(1));
    assert_eq!(runtime.index.depths[1], 1);
    assert_eq!(runtime.index.parent_ids[2], Some(2));
    assert_eq!(runtime.index.depths[2], 2);
    assert_eq!(runtime.height_index.total_height(), total_height);
    let projection = runtime.projection();
    assert!(projection.blocks[0].chrome.has_children);
    assert_eq!(projection.blocks[1].chrome.list_info.depth, 1);
    assert_eq!(projection.blocks[2].chrome.list_info.depth, 2);
}

#[test]
fn undo_and_redo_restore_structure_move_without_full_snapshot() {
    let mut runtime = runtime_with_kind_depths(vec![
        (RichBlockKind::NumberedList, 0, None),
        (RichBlockKind::Todo { checked: false }, 1, Some(1)),
        (RichBlockKind::NumberedList, 0, None),
        (RichBlockKind::NumberedList, 0, None),
    ]);

    assert!(runtime.move_block_subtree_before(1, Some(4)).unwrap());
    assert_eq!(runtime.index.block_ids, vec![3, 1, 2, 4]);

    assert!(runtime.undo_focused_block().unwrap());
    assert_eq!(runtime.index.block_ids, vec![1, 2, 3, 4]);
    assert_eq!(runtime.index.parent_ids[1], Some(1));
    assert_eq!(runtime.index.depths[1], 1);

    assert!(runtime.redo_focused_block().unwrap());
    assert_eq!(runtime.index.block_ids, vec![3, 1, 2, 4]);
    assert_eq!(runtime.index.parent_ids[2], Some(1));
    assert_eq!(runtime.index.depths[2], 1);
}

#[test]
fn structure_move_queues_persistable_transactions_for_move_undo_and_redo() {
    let mut runtime = runtime_with_kind_depths(vec![
        (RichBlockKind::NumberedList, 0, None),
        (RichBlockKind::Todo { checked: false }, 1, Some(1)),
        (RichBlockKind::NumberedList, 0, None),
        (RichBlockKind::NumberedList, 0, None),
    ]);

    assert!(runtime.move_block_subtree_before(1, Some(4)).unwrap());
    assert_eq!(runtime.pending_structure_transaction_count(), 1);
    let txs = runtime.drain_pending_structure_transactions();
    assert_eq!(txs.len(), 1);
    assert_eq!(txs[0].kind, EditTransactionKind::BlockStructureChange);
    assert_eq!(
        txs[0].ops.as_ref(),
        &vec![EditOperation::MoveBlockToParent {
            block_id: 1,
            parent_id: None,
            sibling_index: 1,
        }]
    );
    assert_eq!(
        txs[0].inverse_ops.as_ref(),
        &vec![EditOperation::MoveBlockToParent {
            block_id: 1,
            parent_id: None,
            sibling_index: 0,
        }]
    );

    assert!(runtime.undo_focused_block().unwrap());
    let undo_txs = runtime.drain_pending_structure_transactions();
    assert_eq!(
        undo_txs[0].ops.as_ref(),
        &vec![EditOperation::MoveBlockToParent {
            block_id: 1,
            parent_id: None,
            sibling_index: 0,
        }]
    );

    assert!(runtime.redo_focused_block().unwrap());
    let redo_txs = runtime.drain_pending_structure_transactions();
    assert_eq!(
        redo_txs[0].ops.as_ref(),
        &vec![EditOperation::MoveBlockToParent {
            block_id: 1,
            parent_id: None,
            sibling_index: 1,
        }]
    );
}

#[test]
fn undo_order_prefers_newer_text_edit_over_older_structure_move() {
    let mut runtime = runtime_with_kind_depths(vec![
        (RichBlockKind::NumberedList, 0, None),
        (RichBlockKind::Todo { checked: false }, 1, Some(1)),
        (RichBlockKind::Paragraph, 0, None),
        (RichBlockKind::NumberedList, 0, None),
    ]);

    assert!(runtime.move_block_subtree_before(1, Some(4)).unwrap());
    runtime.focus_block_at_offset(3, 0).unwrap();
    runtime.insert_char('x').unwrap();
    assert_eq!(runtime.focused_text(), Some("xitem"));

    assert!(runtime.undo_focused_block().unwrap());
    assert_eq!(runtime.focused_text(), Some("item"));
    assert_eq!(runtime.index.block_ids, vec![3, 1, 2, 4]);

    assert!(runtime.undo_focused_block().unwrap());
    assert_eq!(runtime.index.block_ids, vec![1, 2, 3, 4]);
}

#[test]
fn move_block_subtree_to_parent_rejects_invalid_parent() {
    let mut runtime = runtime_with_kind_depths(vec![
        (RichBlockKind::Paragraph, 0, None),
        (RichBlockKind::BulletedList, 0, None),
        (RichBlockKind::BulletedList, 1, Some(2)),
    ]);

    assert!(!runtime.move_block_subtree_to_parent(2, Some(1), 0).unwrap());
    assert!(!runtime.move_block_subtree_to_parent(2, Some(3), 0).unwrap());
    assert_eq!(runtime.index.block_ids, vec![1, 2, 3]);
}
