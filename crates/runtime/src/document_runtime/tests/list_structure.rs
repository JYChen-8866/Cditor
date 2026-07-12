use super::*;

#[test]
fn enter_on_bulleted_list_splits_and_inherits_kind() {
    let mut runtime = runtime_with_kind_depths_and_text(vec![(
        RichBlockKind::BulletedList,
        0,
        None,
        "hello world",
    )]);
    runtime.focus_block_at_offset(1, 5).unwrap();

    runtime.handle_enter().unwrap();

    assert_eq!(runtime.index.total_count(), 2);
    assert_eq!(runtime.kind_at_index(0), RichBlockKind::BulletedList);
    assert_eq!(runtime.kind_at_index(1), RichBlockKind::BulletedList);
    assert_eq!(runtime.payload_window.get(1).unwrap().plain_text(), "hello");
    assert_eq!(
        runtime.payload_window.get(2).unwrap().plain_text(),
        " world"
    );
    assert_eq!(runtime.focused_block_id(), Some(2));
    assert_eq!(runtime.caret_offset_for_block(2), Some(0));
    assert_eq!(
        runtime.input_session_target(),
        Some(InputTarget::BlockText { block_id: 2 })
    );
    assert_eq!(runtime.input_session_selected_range(), Some(0..0));
}

#[test]
fn enter_on_numbered_list_splits_and_inherits_kind() {
    let mut runtime =
        runtime_with_kind_depths_and_text(vec![(RichBlockKind::NumberedList, 0, None, "one two")]);
    runtime.focus_block_at_offset(1, 3).unwrap();

    runtime.handle_enter().unwrap();

    assert_eq!(runtime.kind_at_index(0), RichBlockKind::NumberedList);
    assert_eq!(runtime.kind_at_index(1), RichBlockKind::NumberedList);
    assert_eq!(runtime.payload_window.get(1).unwrap().plain_text(), "one");
    assert_eq!(runtime.payload_window.get(2).unwrap().plain_text(), " two");
    let projection = runtime.projection_for_window_planned();
    assert_eq!(
        projection.blocks[0].chrome.prefix,
        BlockPrefixSnapshot::Number { ordinal: 1 }
    );
    assert_eq!(
        projection.blocks[1].chrome.prefix,
        BlockPrefixSnapshot::Number { ordinal: 2 }
    );
}

#[test]
fn enter_on_todo_splits_and_new_item_is_unchecked() {
    let mut runtime = runtime_with_kind_depths_and_text(vec![(
        RichBlockKind::Todo { checked: true },
        0,
        None,
        "done later",
    )]);
    runtime.focus_block_at_offset(1, 4).unwrap();

    runtime.handle_enter().unwrap();

    assert_eq!(
        runtime.kind_at_index(0),
        RichBlockKind::Todo { checked: true }
    );
    assert_eq!(
        runtime.kind_at_index(1),
        RichBlockKind::Todo { checked: false }
    );
    assert_eq!(runtime.payload_window.get(1).unwrap().plain_text(), "done");
    assert_eq!(
        runtime.payload_window.get(2).unwrap().plain_text(),
        " later"
    );
}

#[test]
fn enter_splits_trailing_rich_spans_and_preserves_marks() {
    let record = BlockIndexRecord::new(
        1,
        None,
        0,
        kind_tag_for_rich_block_kind(&RichBlockKind::BulletedList),
        0,
    )
    .with_layout_meta(cditor_core::layout::BlockLayoutMeta::new(1, 32.0));
    let payload = BlockPayloadRecord {
        block_id: 1,
        content_version: 1,
        kind: RichBlockKind::BulletedList,
        payload: BlockPayload::RichText {
            spans: vec![
                InlineSpan::plain("ab"),
                InlineSpan {
                    text: "cd".to_owned(),
                    marks: vec![InlineMark::Bold],
                },
            ],
        },
    };
    let mut runtime = DocumentRuntime::from_index_records(1, vec![record], vec![payload], 1, 720.0);
    runtime.focus_block_at_offset(1, 3).unwrap();

    runtime.handle_enter().unwrap();

    assert_eq!(runtime.payload_window.get(1).unwrap().plain_text(), "abc");
    assert_eq!(runtime.payload_window.get(2).unwrap().plain_text(), "d");
    match &runtime.payload_window.get(2).unwrap().payload {
        BlockPayload::RichText { spans } => {
            assert_eq!(spans.len(), 1);
            assert_eq!(spans[0].text, "d");
            assert_eq!(spans[0].marks, vec![InlineMark::Bold]);
        }
        other => panic!("expected rich text payload, got {other:?}"),
    }
}

#[test]
fn command_enter_inserts_empty_paragraph_without_splitting_list_content() {
    let mut runtime =
        runtime_with_kind_depths_and_text(vec![(RichBlockKind::NumberedList, 0, None, "abcde")]);
    runtime.focus_block_at_offset(1, 2).unwrap();

    let new_block_id = runtime.insert_paragraph_after_focused().unwrap();

    assert_eq!(new_block_id, 2);
    assert_eq!(runtime.kind_at_index(0), RichBlockKind::NumberedList);
    assert_eq!(runtime.kind_at_index(1), RichBlockKind::Paragraph);
    assert_eq!(runtime.payload_window.get(1).unwrap().plain_text(), "abcde");
    assert_eq!(runtime.payload_window.get(2).unwrap().plain_text(), "");
    assert_eq!(runtime.focused_block_id(), Some(2));
}

#[test]
fn indent_focused_block_requires_previous_block_that_supports_children() {
    let mut runtime = runtime_with_kind_depths(vec![
        (RichBlockKind::BulletedList, 0, None),
        (RichBlockKind::BulletedList, 0, None),
    ]);
    runtime.focus_block(2);
    let before_version = runtime.index.structure_version;

    assert!(runtime.indent_focused_block().unwrap());

    assert_eq!(runtime.index.structure_version, before_version + 1);
    assert_eq!(runtime.index.parent_ids[1], Some(1));
    assert_eq!(runtime.index.depths[1], 1);
    let projection = runtime.projection();
    assert!(projection.blocks[0].chrome.has_children);
    assert_eq!(projection.blocks[1].chrome.list_info.depth, 1);

    let mut runtime = runtime_with_kind_depths(vec![
        (RichBlockKind::Paragraph, 0, None),
        (RichBlockKind::BulletedList, 0, None),
    ]);
    runtime.focus_block(2);
    assert!(!runtime.indent_focused_block().unwrap());
    assert_eq!(runtime.index.parent_ids[1], None);
    assert_eq!(runtime.index.depths[1], 0);
}

fn runtime_with_single_payload(kind: RichBlockKind, payload: BlockPayload) -> DocumentRuntime {
    let record = BlockIndexRecord::new(1, None, 0, kind_tag_for_rich_block_kind(&kind), 0)
        .with_layout_meta(cditor_core::layout::BlockLayoutMeta::new(1, 32.0));
    let payload = BlockPayloadRecord {
        block_id: 1,
        content_version: 1,
        kind,
        payload,
    };
    DocumentRuntime::from_index_records(1, vec![record], vec![payload], 1, 720.0)
}

#[test]
fn code_tab_inserts_four_spaces_without_structure_change() {
    let mut runtime = runtime_with_single_payload(
        RichBlockKind::Code {
            language: Some("rust".to_owned()),
        },
        BlockPayload::Code {
            language: Some("rust".to_owned()),
            text: "fn main()".to_owned(),
        },
    );
    runtime.focus_block_at_offset(1, 2).unwrap();
    let before_structure_version = runtime.index.structure_version;

    assert!(runtime.indent_focused_block().unwrap());

    assert_eq!(runtime.focused_text().unwrap(), "fn     main()");
    assert_eq!(runtime.caret_offset_for_block(1), Some(6));
    assert_eq!(runtime.index.structure_version, before_structure_version);
    assert_eq!(runtime.pending_structure_transaction_count(), 0);
    match &runtime.payload_window.get(1).unwrap().payload {
        BlockPayload::Code { text, .. } => assert_eq!(text, "fn     main()"),
        other => panic!("expected code payload, got {other:?}"),
    }
}

#[test]
fn code_shift_tab_removes_line_indent_without_structure_change() {
    let mut runtime = runtime_with_single_payload(
        RichBlockKind::Code {
            language: Some("rust".to_owned()),
        },
        BlockPayload::Code {
            language: Some("rust".to_owned()),
            text: "fn main() {\n    value\n}".to_owned(),
        },
    );
    runtime
        .focus_block_at_offset(1, "fn main() {\n    va".len())
        .unwrap();
    let before_structure_version = runtime.index.structure_version;

    assert!(runtime.outdent_focused_block().unwrap());

    assert_eq!(runtime.focused_text().unwrap(), "fn main() {\nvalue\n}");
    assert_eq!(
        runtime.caret_offset_for_block(1),
        Some("fn main() {\nva".len())
    );
    assert_eq!(runtime.index.structure_version, before_structure_version);
    assert_eq!(runtime.pending_structure_transaction_count(), 0);
}

#[test]
fn raw_markdown_tab_and_shift_tab_are_payload_only() {
    let mut runtime = runtime_with_single_payload(
        RichBlockKind::RawMarkdown,
        BlockPayload::RichText {
            spans: vec![InlineSpan::plain("alpha")],
        },
    );
    runtime.focus_block_at_offset(1, 0).unwrap();
    let before_structure_version = runtime.index.structure_version;

    assert!(runtime.indent_focused_block().unwrap());
    assert_eq!(runtime.focused_text().unwrap(), "    alpha");
    assert!(runtime.outdent_focused_block().unwrap());
    assert_eq!(runtime.focused_text().unwrap(), "alpha");
    assert_eq!(runtime.index.structure_version, before_structure_version);
    assert_eq!(runtime.pending_structure_transaction_count(), 0);
}

#[test]
fn tab_indents_block_under_previous_sibling_children_tail() {
    let mut runtime = runtime_with_kind_depths(vec![
        (RichBlockKind::BulletedList, 0, None),
        (RichBlockKind::BulletedList, 1, Some(1)),
        (RichBlockKind::BulletedList, 0, None),
    ]);
    runtime.focus_block(3);

    assert!(runtime.indent_focused_block().unwrap());

    assert_eq!(runtime.index.block_ids, vec![1, 2, 3]);
    assert_eq!(runtime.index.parent_ids[2], Some(1));
    assert_eq!(runtime.index.depths[2], 1);
    assert_eq!(runtime.direct_child_position(Some(1), 3), Some(1));
    assert_eq!(runtime.focused_block_id(), Some(3));
    assert_eq!(runtime.pending_structure_transaction_count(), 1);
}

#[test]
fn indent_and_outdent_preserve_caret_and_input_session_selection() {
    let mut runtime = runtime_with_kind_depths_and_text(vec![
        (RichBlockKind::BulletedList, 0, None, "parent"),
        (RichBlockKind::BulletedList, 0, None, "abcdef"),
    ]);
    runtime.focus_block_at_offset(2, 2).unwrap();

    assert!(runtime.indent_focused_block().unwrap());

    assert_eq!(runtime.focused_block_id(), Some(2));
    assert_eq!(runtime.caret_offset_for_block(2), Some(2));
    assert_eq!(
        runtime.input_session_target(),
        Some(InputTarget::BlockText { block_id: 2 })
    );
    assert_eq!(runtime.input_session_selected_range(), Some(2..2));

    assert!(runtime.outdent_focused_block().unwrap());

    assert_eq!(runtime.focused_block_id(), Some(2));
    assert_eq!(runtime.caret_offset_for_block(2), Some(2));
    assert_eq!(
        runtime.input_session_target(),
        Some(InputTarget::BlockText { block_id: 2 })
    );
    assert_eq!(runtime.input_session_selected_range(), Some(2..2));
}

#[test]
fn tab_first_sibling_does_nothing() {
    let mut runtime = runtime_with_kind_depths(vec![
        (RichBlockKind::BulletedList, 0, None),
        (RichBlockKind::BulletedList, 0, None),
    ]);
    runtime.focus_block(1);
    let before_version = runtime.index.structure_version;

    assert!(!runtime.indent_focused_block().unwrap());

    assert_eq!(runtime.index.structure_version, before_version);
    assert_eq!(runtime.index.parent_ids, vec![None, None]);
    assert_eq!(runtime.pending_structure_transaction_count(), 0);
}

#[test]
fn tab_previous_non_container_does_nothing() {
    let mut runtime = runtime_with_kind_depths(vec![
        (RichBlockKind::Paragraph, 0, None),
        (RichBlockKind::BulletedList, 0, None),
    ]);
    runtime.focus_block(2);
    let before_version = runtime.index.structure_version;

    assert!(!runtime.indent_focused_block().unwrap());

    assert_eq!(runtime.index.structure_version, before_version);
    assert_eq!(runtime.index.parent_ids[1], None);
    assert_eq!(runtime.pending_structure_transaction_count(), 0);
}

#[test]
fn outdent_focused_block_moves_subtree_up_one_level() {
    let mut runtime = runtime_with_kind_depths(vec![
        (RichBlockKind::BulletedList, 0, None),
        (RichBlockKind::BulletedList, 1, Some(1)),
        (RichBlockKind::Todo { checked: false }, 2, Some(2)),
    ]);
    runtime.focus_block(2);
    let before_version = runtime.index.structure_version;

    assert!(runtime.outdent_focused_block().unwrap());

    assert_eq!(runtime.index.structure_version, before_version + 1);
    assert_eq!(runtime.index.parent_ids[1], None);
    assert_eq!(runtime.index.depths[1], 0);
    assert_eq!(runtime.index.parent_ids[2], Some(2));
    assert_eq!(runtime.index.depths[2], 1);
    let projection = runtime.projection();
    assert_eq!(projection.blocks[1].chrome.list_info.depth, 0);
    assert_eq!(projection.blocks[2].chrome.list_info.depth, 1);
}

#[test]
fn shift_tab_outdents_block_after_parent_subtree() {
    let mut runtime = runtime_with_kind_depths(vec![
        (RichBlockKind::BulletedList, 0, None),
        (RichBlockKind::BulletedList, 1, Some(1)),
        (RichBlockKind::Todo { checked: false }, 2, Some(2)),
        (RichBlockKind::BulletedList, 1, Some(1)),
    ]);
    runtime.focus_block(2);

    assert!(runtime.outdent_focused_block().unwrap());

    assert_eq!(runtime.index.block_ids, vec![1, 4, 2, 3]);
    assert_eq!(runtime.index.parent_ids[2], None);
    assert_eq!(runtime.index.depths[2], 0);
    assert_eq!(runtime.index.parent_ids[3], Some(2));
    assert_eq!(runtime.index.depths[3], 1);
    assert_eq!(runtime.focused_block_id(), Some(2));
    assert_eq!(runtime.pending_structure_transaction_count(), 1);
}

#[test]
fn shift_tab_root_block_does_nothing() {
    let mut runtime = runtime_with_kind_depths(vec![
        (RichBlockKind::BulletedList, 0, None),
        (RichBlockKind::BulletedList, 0, None),
    ]);
    runtime.focus_block(2);
    let before_version = runtime.index.structure_version;

    assert!(!runtime.outdent_focused_block().unwrap());

    assert_eq!(runtime.index.structure_version, before_version);
    assert_eq!(runtime.index.parent_ids, vec![None, None]);
    assert_eq!(runtime.pending_structure_transaction_count(), 0);
}

#[test]
fn indent_outdent_preserve_subtree_children_and_queue_transactions() {
    let mut runtime = runtime_with_kind_depths(vec![
        (RichBlockKind::BulletedList, 0, None),
        (RichBlockKind::BulletedList, 0, None),
        (RichBlockKind::Todo { checked: false }, 1, Some(2)),
    ]);
    runtime.focus_block(2);

    assert!(runtime.indent_focused_block().unwrap());
    assert_eq!(runtime.index.parent_ids[1], Some(1));
    assert_eq!(runtime.index.depths[1], 1);
    assert_eq!(runtime.index.parent_ids[2], Some(2));
    assert_eq!(runtime.index.depths[2], 2);
    assert_eq!(runtime.pending_structure_transaction_count(), 1);

    assert!(runtime.outdent_focused_block().unwrap());
    assert_eq!(runtime.index.parent_ids[1], None);
    assert_eq!(runtime.index.depths[1], 0);
    assert_eq!(runtime.index.parent_ids[2], Some(2));
    assert_eq!(runtime.index.depths[2], 1);
    assert_eq!(runtime.pending_structure_transaction_count(), 2);
}

#[test]
fn numbered_ordinal_recomputes_after_enter_indent_outdent() {
    let mut runtime = runtime_with_kind_depths_and_text(vec![
        (RichBlockKind::NumberedList, 0, None, "one"),
        (RichBlockKind::NumberedList, 0, None, "two"),
    ]);
    runtime.focus_block_at_offset(1, 3).unwrap();
    runtime.handle_enter().unwrap();
    assert_eq!(runtime.index.block_ids, vec![1, 3, 2]);

    let projection = runtime.projection_for_window_planned();
    assert_eq!(
        projection.blocks[0].chrome.prefix,
        BlockPrefixSnapshot::Number { ordinal: 1 }
    );
    assert_eq!(
        projection.blocks[1].chrome.prefix,
        BlockPrefixSnapshot::Number { ordinal: 2 }
    );
    assert_eq!(
        projection.blocks[2].chrome.prefix,
        BlockPrefixSnapshot::Number { ordinal: 3 }
    );

    runtime.focus_block(2);
    assert!(runtime.indent_focused_block().unwrap());
    let projection = runtime.projection_for_window_planned();
    assert_eq!(
        projection.blocks[0].chrome.prefix,
        BlockPrefixSnapshot::Number { ordinal: 1 }
    );
    assert_eq!(
        projection.blocks[1].chrome.prefix,
        BlockPrefixSnapshot::Number { ordinal: 2 }
    );
    assert_eq!(
        projection.blocks[2].chrome.prefix,
        BlockPrefixSnapshot::Number { ordinal: 1 }
    );

    assert!(runtime.outdent_focused_block().unwrap());
    let projection = runtime.projection_for_window_planned();
    assert_eq!(
        projection.blocks[0].chrome.prefix,
        BlockPrefixSnapshot::Number { ordinal: 1 }
    );
    assert_eq!(
        projection.blocks[1].chrome.prefix,
        BlockPrefixSnapshot::Number { ordinal: 2 }
    );
    assert_eq!(
        projection.blocks[2].chrome.prefix,
        BlockPrefixSnapshot::Number { ordinal: 3 }
    );
}

#[test]
fn indent_outdent_undo_redo_restore_tree() {
    let mut runtime = runtime_with_kind_depths(vec![
        (RichBlockKind::BulletedList, 0, None),
        (RichBlockKind::BulletedList, 0, None),
        (RichBlockKind::Todo { checked: false }, 1, Some(2)),
    ]);
    runtime.focus_block(2);

    assert!(runtime.indent_focused_block().unwrap());
    assert_eq!(runtime.index.parent_ids[1], Some(1));
    assert_eq!(runtime.index.depths[2], 2);

    assert!(runtime.undo_focused_block().unwrap());
    assert_eq!(runtime.index.parent_ids[1], None);
    assert_eq!(runtime.index.depths[1], 0);
    assert_eq!(runtime.index.parent_ids[2], Some(2));
    assert_eq!(runtime.index.depths[2], 1);

    assert!(runtime.redo_focused_block().unwrap());
    assert_eq!(runtime.index.parent_ids[1], Some(1));
    assert_eq!(runtime.index.depths[1], 1);
    assert_eq!(runtime.index.parent_ids[2], Some(2));
    assert_eq!(runtime.index.depths[2], 2);
}

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
        txs[0].ops,
        vec![EditOperation::MoveBlockToParent {
            block_id: 1,
            parent_id: None,
            sibling_index: 1,
        }]
    );
    assert_eq!(
        txs[0].inverse_ops,
        vec![EditOperation::MoveBlockToParent {
            block_id: 1,
            parent_id: None,
            sibling_index: 0,
        }]
    );

    assert!(runtime.undo_focused_block().unwrap());
    let undo_txs = runtime.drain_pending_structure_transactions();
    assert_eq!(
        undo_txs[0].ops,
        vec![EditOperation::MoveBlockToParent {
            block_id: 1,
            parent_id: None,
            sibling_index: 0,
        }]
    );

    assert!(runtime.redo_focused_block().unwrap());
    let redo_txs = runtime.drain_pending_structure_transactions();
    assert_eq!(
        redo_txs[0].ops,
        vec![EditOperation::MoveBlockToParent {
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
