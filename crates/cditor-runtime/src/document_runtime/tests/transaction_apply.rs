use cditor_core::edit::{
    ChangeOrigin, ScrollAnchor, TableEditOperation, TransactionPermission,
    TransactionPermissionSet, TransactionPrecondition,
};

use super::*;
use crate::document_runtime::TransactionApplyError;

// P4-005/P4-006：external transaction 原子应用的测试。
// 语义状态复用 undo property test 的口径：结构 + 全部 payload 文本。

fn semantic_state(runtime: &DocumentRuntime) -> String {
    let mut state = String::new();
    for position in 0..runtime.document.index.total_count() {
        let block_id = runtime
            .document
            .index
            .id_at(position)
            .expect("index position");
        state.push_str(&format!(
            "{}:{:?}:{}:{};",
            block_id,
            runtime
                .document
                .index
                .parent_id_at(position)
                .expect("parent"),
            runtime.document.index.depth_at(position).expect("depth"),
            runtime.document.index.kind_tag_at(position).expect("kind"),
        ));
    }
    state.push('\n');
    let mut ids: Vec<BlockId> = runtime
        .document
        .payload_window
        .payloads
        .keys()
        .copied()
        .collect();
    ids.sort_unstable();
    for block_id in ids {
        let record = runtime.document.payload_window.get(block_id).unwrap();
        state.push_str(&format!("{block_id}->{}\n", record.plain_text()));
    }
    state
}

fn base_runtime() -> DocumentRuntime {
    DocumentRuntime::from_payloads(
        1,
        vec![
            BlockPayloadRecord::rich_text(1, RichBlockKind::Paragraph, "hello world"),
            BlockPayloadRecord::rich_text(2, RichBlockKind::Paragraph, "second"),
            BlockPayloadRecord::rich_text(3, RichBlockKind::Quote, "quoted"),
        ],
        720.0,
    )
}

fn transaction(ops: Vec<EditOperation>, inverse_ops: Vec<EditOperation>) -> EditTransaction {
    EditTransaction::new(
        77,
        EditTransactionKind::ExplicitCommand,
        1_000,
        ops,
        inverse_ops,
    )
}

#[test]
fn text_ops_apply_with_version_and_dirty_range() {
    let mut runtime = base_runtime();
    let content_before = runtime
        .document
        .payload_window
        .get(1)
        .unwrap()
        .content_version;
    let revision_before = runtime.revision();

    let applied = runtime
        .apply_external_transaction(
            &transaction(
                vec![
                    EditOperation::InsertText {
                        block_id: 1,
                        offset: 5,
                        text: ", 世界".to_owned(),
                    },
                    EditOperation::DeleteText {
                        block_id: 2,
                        range: 0..3,
                    },
                ],
                vec![],
            ),
            ChangeOrigin::Remote,
        )
        .expect("apply");

    assert_eq!(
        runtime.document.payload_window.get(1).unwrap().plain_text(),
        "hello, 世界 world"
    );
    assert_eq!(
        runtime.document.payload_window.get(2).unwrap().plain_text(),
        "ond"
    );
    assert_eq!(
        runtime
            .document
            .payload_window
            .get(1)
            .unwrap()
            .content_version,
        content_before + 1
    );
    assert!(runtime.revision() > revision_before);
    assert_eq!(applied.affected_blocks, vec![1, 2]);
    assert_eq!(applied.dirty_range, Some(0..2));
    assert!(!applied.structure_changed);
    assert!(!applied.recorded_undo, "remote must not record local undo");
    assert!(!runtime.can_undo());
}

#[test]
fn failing_op_rolls_back_the_whole_transaction() {
    let mut runtime = base_runtime();
    let before = semantic_state(&runtime);
    let structure_before = runtime.structure_version();

    let error = runtime
        .apply_external_transaction(
            &transaction(
                vec![
                    // op 0 合法，op 1 越界：整个事务必须回滚。
                    EditOperation::InsertText {
                        block_id: 1,
                        offset: 0,
                        text: "X".to_owned(),
                    },
                    EditOperation::DeleteText {
                        block_id: 2,
                        range: 0..999,
                    },
                ],
                vec![],
            ),
            ChangeOrigin::Remote,
        )
        .expect_err("must fail");

    assert!(matches!(
        error,
        TransactionApplyError::Precondition { op_index: 1, .. }
    ));
    assert_eq!(semantic_state(&runtime), before, "state must be untouched");
    assert_eq!(runtime.structure_version(), structure_before);
}

#[test]
fn permission_is_checked_before_any_operation_is_staged() {
    let mut runtime = base_runtime();
    let before = semantic_state(&runtime);
    let revision_before = runtime.revision();
    let tx = transaction(
        vec![EditOperation::InsertText {
            block_id: 1,
            offset: 0,
            text: "forbidden".to_owned(),
        }],
        Vec::new(),
    );

    let error = runtime
        .apply_transaction(&tx, TransactionPermissionSet::READ_ONLY)
        .expect_err("readonly permission set must reject edit");
    assert_eq!(
        error,
        TransactionApplyError::PermissionDenied(TransactionPermission::EditDocument)
    );
    assert_eq!(semantic_state(&runtime), before);
    assert_eq!(runtime.revision(), revision_before);
}

#[test]
fn stale_version_precondition_rejects_before_staging() {
    let mut runtime = base_runtime();
    let before = semantic_state(&runtime);
    let tx = transaction(
        vec![EditOperation::DeleteText {
            block_id: 1,
            range: 0..1,
        }],
        Vec::new(),
    )
    .with_precondition(TransactionPrecondition::DocumentRevision(99))
    .with_precondition(TransactionPrecondition::BlockContentVersion {
        block_id: 1,
        version: 1,
    });

    let error = runtime
        .apply_transaction(&tx, TransactionPermissionSet::FULL_ACCESS)
        .expect_err("stale revision must reject transaction");
    assert!(matches!(
        error,
        TransactionApplyError::StalePrecondition { index: 0, .. }
    ));
    assert_eq!(semantic_state(&runtime), before);
}

#[test]
fn transaction_origin_drives_undo_without_a_side_channel() {
    let mut runtime = base_runtime();
    let tx = transaction(
        vec![EditOperation::InsertText {
            block_id: 1,
            offset: 0,
            text: "remote:".to_owned(),
        }],
        vec![EditOperation::DeleteText {
            block_id: 1,
            range: 0..7,
        }],
    )
    .with_origin(ChangeOrigin::Remote);

    let applied = runtime
        .apply_transaction(&tx, TransactionPermissionSet::FULL_ACCESS)
        .expect("remote transaction applies");
    assert!(!applied.recorded_undo);
    assert!(!runtime.can_undo());
}

#[test]
fn non_char_boundary_offsets_are_rejected() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "中文",
        )],
        720.0,
    );
    let error = runtime
        .apply_external_transaction(
            &transaction(
                vec![EditOperation::InsertText {
                    block_id: 1,
                    offset: 1,
                    text: "x".to_owned(),
                }],
                vec![],
            ),
            ChangeOrigin::Remote,
        )
        .expect_err("must fail");
    assert!(matches!(
        error,
        TransactionApplyError::Precondition { op_index: 0, .. }
    ));
}

#[test]
fn structural_ops_apply_and_validate_preorder() {
    let mut runtime = base_runtime();

    let applied = runtime
        .apply_external_transaction(
            &transaction(
                vec![
                    EditOperation::InsertBlock {
                        index: 1,
                        block: BlockIndexRecord::new(10, None, 0, 1, 0),
                    },
                    EditOperation::InsertText {
                        block_id: 10,
                        offset: 0,
                        text: "inserted".to_owned(),
                    },
                    EditOperation::MoveBlockToParent {
                        block_id: 3,
                        parent_id: Some(1),
                        sibling_index: 0,
                    },
                ],
                vec![],
            ),
            ChangeOrigin::Remote,
        )
        .expect("apply");

    assert!(applied.structure_changed);
    // preorder: 1, 3(child of 1), 10, 2
    let order: Vec<BlockId> = (0..runtime.document.index.total_count())
        .map(|position| runtime.document.index.id_at(position).unwrap())
        .collect();
    assert_eq!(order, vec![1, 3, 10, 2]);
    assert_eq!(runtime.document.index.parent_id_at(1).unwrap(), Some(1));
    assert_eq!(runtime.document.index.depth_at(1).unwrap(), 1);
    assert_eq!(
        runtime
            .document
            .payload_window
            .get(10)
            .unwrap()
            .plain_text(),
        "inserted"
    );
}

#[test]
fn invalid_parent_reference_is_rejected_atomically() {
    let mut runtime = base_runtime();
    let before = semantic_state(&runtime);

    // 插入的块声称 parent=999：整树校验失败，事务拒绝。
    let error = runtime
        .apply_external_transaction(
            &transaction(
                vec![EditOperation::InsertBlock {
                    index: 1,
                    block: BlockIndexRecord::new(10, Some(999), 1, 1, 0),
                }],
                vec![],
            ),
            ChangeOrigin::Remote,
        )
        .expect_err("must fail");
    assert!(matches!(
        error,
        TransactionApplyError::InvalidResultingStructure(_)
    ));
    assert_eq!(semantic_state(&runtime), before);
}

#[test]
fn split_and_merge_ops_round_trip() {
    let mut runtime = base_runtime();
    runtime
        .apply_external_transaction(
            &transaction(
                vec![EditOperation::SplitBlock {
                    block_id: 1,
                    offset: 5,
                    new_block_id: 20,
                }],
                vec![],
            ),
            ChangeOrigin::Remote,
        )
        .expect("split");
    assert_eq!(
        runtime.document.payload_window.get(1).unwrap().plain_text(),
        "hello"
    );
    assert_eq!(
        runtime
            .document
            .payload_window
            .get(20)
            .unwrap()
            .plain_text(),
        " world"
    );

    runtime
        .apply_external_transaction(
            &transaction(
                vec![EditOperation::MergeBlocks {
                    previous: 1,
                    current: 20,
                }],
                vec![],
            ),
            ChangeOrigin::Remote,
        )
        .expect("merge");
    assert_eq!(
        runtime.document.payload_window.get(1).unwrap().plain_text(),
        "hello world"
    );
    assert!(runtime.document.index.index_of(20).is_none());
    assert!(runtime.document.payload_window.get(20).is_none());
}

#[test]
fn table_ops_apply_with_stale_state_rejection() {
    let mut table = cditor_core::rich_text::TablePayload {
        rows: vec![
            cditor_core::rich_text::TableRowPayload {
                cells: vec![
                    cditor_core::rich_text::TableCellPayload::plain("A"),
                    cditor_core::rich_text::TableCellPayload::plain("B"),
                ],
                height: Default::default(),
            },
            cditor_core::rich_text::TableRowPayload {
                cells: vec![
                    cditor_core::rich_text::TableCellPayload::plain("C"),
                    cditor_core::rich_text::TableCellPayload::plain("D"),
                ],
                height: Default::default(),
            },
        ],
        columns: Vec::new(),
        header_rows: 1,
        header_cols: 0,
        header_style: Default::default(),
    };
    table.normalize();
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord {
            block_id: 5,
            content_version: 1,
            kind: RichBlockKind::Table,
            payload: BlockPayload::Table(table),
        }],
        720.0,
    );

    // old_spans 匹配 -> 应用成功。
    runtime
        .apply_external_transaction(
            &transaction(
                vec![EditOperation::Table(TableEditOperation::SetCellText {
                    block_id: 5,
                    row: 0,
                    col: 0,
                    old_spans: vec![InlineSpan::plain("A")],
                    new_spans: vec![InlineSpan::plain("A2")],
                })],
                vec![],
            ),
            ChangeOrigin::Remote,
        )
        .expect("set cell");

    // old_spans 过期 -> 拒绝（OT 安全）。
    let error = runtime
        .apply_external_transaction(
            &transaction(
                vec![EditOperation::Table(TableEditOperation::SetCellText {
                    block_id: 5,
                    row: 0,
                    col: 0,
                    old_spans: vec![InlineSpan::plain("A")],
                    new_spans: vec![InlineSpan::plain("A3")],
                })],
                vec![],
            ),
            ChangeOrigin::Remote,
        )
        .expect_err("stale must fail");
    assert!(matches!(
        error,
        TransactionApplyError::Precondition { op_index: 0, .. }
    ));

    // 行插入。
    runtime
        .apply_external_transaction(
            &transaction(
                vec![EditOperation::Table(TableEditOperation::InsertRows {
                    block_id: 5,
                    index: 1,
                    rows: vec![cditor_core::rich_text::TableRowPayload {
                        cells: vec![
                            cditor_core::rich_text::TableCellPayload::plain("X"),
                            cditor_core::rich_text::TableCellPayload::plain("Y"),
                        ],
                        height: Default::default(),
                    }],
                })],
                vec![],
            ),
            ChangeOrigin::Remote,
        )
        .expect("insert rows");
    let BlockPayload::Table(table) = &runtime.document.payload_window.get(5).unwrap().payload
    else {
        panic!("expected table");
    };
    assert_eq!(table.rows.len(), 3);
}

#[test]
fn local_origin_records_undo_and_round_trips() {
    let mut runtime = base_runtime();
    let before = semantic_state(&runtime);

    let forward = transaction(
        vec![EditOperation::InsertText {
            block_id: 1,
            offset: 0,
            text: "SDK:".to_owned(),
        }],
        vec![EditOperation::DeleteText {
            block_id: 1,
            range: 0..4,
        }],
    );
    let applied = runtime
        .apply_external_transaction(&forward, ChangeOrigin::Host)
        .expect("apply");
    assert!(applied.recorded_undo);
    let after = semantic_state(&runtime);
    assert!(runtime.can_undo());

    assert!(runtime.undo_focused_block().unwrap());
    assert_eq!(semantic_state(&runtime), before, "undo must restore");
    assert!(runtime.can_redo());

    assert!(runtime.redo_focused_block().unwrap());
    assert_eq!(semantic_state(&runtime), after, "redo must restore");
}

#[test]
fn external_transaction_undo_redo_restore_selection_and_scroll_anchors() {
    let mut runtime = runtime_with_paragraph_blocks(1_000);
    let before_selection = DocumentSelection::caret(TextPosition::downstream(500, 0));
    let after_selection = DocumentSelection::caret(TextPosition::downstream(500, 1));
    let before_anchor = ScrollAnchor {
        block_id: 101,
        offset_in_block: 0.0,
        viewport_y: 0.0,
    };
    let after_anchor = ScrollAnchor {
        block_id: 201,
        offset_in_block: 0.0,
        viewport_y: 0.0,
    };
    let forward = EditTransaction::insert_text(88, 1_001, 500, 0, "X")
        .with_selection(Some(before_selection), Some(after_selection))
        .with_anchor(Some(before_anchor), Some(after_anchor));
    runtime
        .apply_external_transaction(&forward, ChangeOrigin::Host)
        .expect("apply");
    runtime.set_document_selection(after_selection).unwrap();
    runtime
        .restore_scroll_anchor(Some(after_anchor), 0.0)
        .unwrap();

    assert!(runtime.undo_focused_block().unwrap());
    assert_eq!(
        runtime.document_selection_snapshot(),
        Some(before_selection)
    );
    assert_eq!(runtime.layout.scroll.global_scroll_top, 3_200.0);

    assert!(runtime.redo_focused_block().unwrap());
    assert_eq!(runtime.document_selection_snapshot(), Some(after_selection));
    assert_eq!(runtime.layout.scroll.global_scroll_top, 6_400.0);
}

#[test]
fn deleting_focused_block_clears_editing_state() {
    let mut runtime = base_runtime();
    runtime.focus_block_at_offset(2, 3).unwrap();
    assert_eq!(runtime.focused_block_id(), Some(2));

    runtime
        .apply_external_transaction(
            &transaction(vec![EditOperation::DeleteBlock { block_id: 2 }], vec![]),
            ChangeOrigin::Remote,
        )
        .expect("delete");
    assert_eq!(runtime.focused_block_id(), None);
    assert!(runtime.document.index.index_of(2).is_none());
}

#[test]
fn subtree_splitting_range_delete_is_rejected() {
    // 构造含子树的文档：1(p) -> 2(child), 3(root)
    let records = vec![
        BlockIndexRecord::new(1, None, 0, 1, 0),
        BlockIndexRecord::new(2, Some(1), 1, 1, 0),
        BlockIndexRecord::new(3, None, 0, 1, 0),
    ];
    let payloads = vec![
        BlockPayloadRecord::rich_text(1, RichBlockKind::Paragraph, "parent"),
        BlockPayloadRecord::rich_text(2, RichBlockKind::Paragraph, "child"),
        BlockPayloadRecord::rich_text(3, RichBlockKind::Paragraph, "tail"),
    ];
    let mut runtime = DocumentRuntime::from_index_records(1, records, payloads, 1, 720.0);
    let before = semantic_state(&runtime);

    // 删除 range 0..1 只删 parent、留下孤儿 child -> 拒绝。
    let error = runtime
        .apply_external_transaction(
            &transaction(
                vec![EditOperation::DeleteBlockRange { range: 0..1 }],
                vec![],
            ),
            ChangeOrigin::Remote,
        )
        .expect_err("orphaning delete must fail");
    assert!(matches!(
        error,
        TransactionApplyError::Precondition { op_index: 0, .. }
    ));
    assert_eq!(semantic_state(&runtime), before);

    // 完整子树 range 0..2 允许。
    runtime
        .apply_external_transaction(
            &transaction(
                vec![EditOperation::DeleteBlockRange { range: 0..2 }],
                vec![],
            ),
            ChangeOrigin::Remote,
        )
        .expect("subtree delete");
    assert_eq!(runtime.document.index.total_count(), 1);
    assert!(runtime.document.payload_window.get(1).is_none());
    assert!(runtime.document.payload_window.get(2).is_none());
}
