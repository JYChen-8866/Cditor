use cditor_core::edit::{
    AssetEditOperation, AssetSnapshot, AssetState, BlockEditOperation, CollectionEditOperation,
    CollectionPropertyValue, CollectionRecordSnapshot, CommentAnchor, CommentEditOperation,
    CommentMessageSnapshot, CommentThreadSnapshot, TextEditOperation, TransactionPermission,
    TransactionPermissionSet,
};
use cditor_core::rich_text::{
    CollectionPayload, CollectionPropertyKind, CollectionPropertyPayload, CollectionViewLayout,
    CollectionViewPayload, RichTextContent,
};

use super::*;

fn tx(ops: Vec<EditOperation>, inverse_ops: Vec<EditOperation>) -> EditTransaction {
    EditTransaction::new(
        900,
        EditTransactionKind::ExplicitCommand,
        1_000,
        ops,
        inverse_ops,
    )
    .with_origin(cditor_core::edit::ChangeOrigin::Host)
}

fn collection_runtime() -> (DocumentRuntime, CollectionPayload) {
    let collection = CollectionPayload::for_block(1, "Projects");
    let runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord {
            block_id: 1,
            content_version: 4,
            kind: RichBlockKind::Database,
            payload: BlockPayload::Collection(collection.clone()),
        }],
        720.0,
    );
    (runtime, collection)
}

#[test]
fn typed_text_operations_cover_block_and_auxiliary_surfaces_once() {
    let collection = CollectionPayload::for_block(2, "Projects");
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![
            BlockPayloadRecord::rich_text(1, RichBlockKind::Paragraph, "hello"),
            BlockPayloadRecord {
                block_id: 2,
                content_version: 7,
                kind: RichBlockKind::Database,
                payload: BlockPayload::Collection(collection),
            },
        ],
        720.0,
    );
    let layout_before = runtime.document.index.layout_meta[0].layout_version;

    let applied = runtime
        .apply_transaction(
            &tx(
                vec![
                    EditOperation::Text(TextEditOperation::ReplaceSpans {
                        surface_id: SurfaceId::Block(1),
                        range: 0..5,
                        old_spans: vec![InlineSpan::plain("hello")],
                        new_spans: vec![InlineSpan::plain("world")],
                    }),
                    EditOperation::Text(TextEditOperation::ReplaceSpans {
                        surface_id: SurfaceId::CollectionTitle { block_id: 2 },
                        range: 0..8,
                        old_spans: vec![InlineSpan::plain("Projects")],
                        new_spans: vec![InlineSpan::plain("Roadmap")],
                    }),
                ],
                Vec::new(),
            ),
            TransactionPermissionSet::FULL_ACCESS,
        )
        .expect("typed text transaction");

    assert_eq!(
        runtime.block_payload_record(1).unwrap().plain_text(),
        "world"
    );
    assert_eq!(
        runtime.block_payload_record(2).unwrap().plain_text(),
        "Roadmap"
    );
    assert_eq!(applied.content_versions, vec![(1, 2), (2, 8)]);
    assert_eq!(
        applied.layout_versions[0],
        (1, layout_before.saturating_add(1))
    );
    assert_eq!(applied.document_revision, runtime.revision());
    assert_eq!(applied.structure_version, runtime.structure_version());
    assert_eq!(applied.dirty_range, Some(0..2));
}

#[test]
fn typed_text_operation_supports_code_without_accepting_inline_marks() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord {
            block_id: 1,
            content_version: 1,
            kind: RichBlockKind::Code {
                language: Some("rust".to_owned()),
            },
            payload: BlockPayload::Code {
                language: Some("rust".to_owned()),
                text: "let a = 1;".to_owned(),
            },
        }],
        720.0,
    );
    runtime
        .apply_transaction(
            &tx(
                vec![EditOperation::Text(TextEditOperation::ReplaceSpans {
                    surface_id: SurfaceId::Block(1),
                    range: 8..9,
                    old_spans: vec![InlineSpan::plain("1")],
                    new_spans: vec![InlineSpan::plain("2")],
                })],
                Vec::new(),
            ),
            TransactionPermissionSet::FULL_ACCESS,
        )
        .expect("code edit");
    assert_eq!(
        runtime.block_payload_record(1).unwrap().plain_text(),
        "let a = 2;"
    );
}

#[test]
fn inline_format_entrypoint_records_one_typed_transaction() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "format me",
        )],
        720.0,
    );
    runtime.focus_block_at_offset(1, 0).unwrap();
    assert!(runtime.select_focused_text_all());

    assert!(
        runtime
            .toggle_inline_mark_on_selection(InlineMark::Bold)
            .unwrap()
    );
    assert!(
        runtime
            .history
            .undo_stacks
            .get(&1)
            .is_none_or(Vec::is_empty)
    );
    assert_eq!(runtime.history.external_undo_stack.len(), 1);
    let transaction = runtime.history.external_undo_stack.last().unwrap();
    let persistent = runtime.pending_structure_transactions.last().unwrap();
    assert!(std::sync::Arc::ptr_eq(&transaction.ops, &persistent.ops));
    assert!(std::sync::Arc::ptr_eq(
        &transaction.inverse_ops,
        &persistent.inverse_ops
    ));
    assert_eq!(transaction.kind, EditTransactionKind::Format);
    assert!(matches!(
        transaction.ops.as_slice(),
        [EditOperation::Text(_)]
    ));
    assert!(matches!(
        transaction.inverse_ops.as_slice(),
        [EditOperation::Text(_)]
    ));
}

#[test]
fn single_char_hot_path_records_replayable_typed_spans_with_inherited_marks() {
    fn runtime() -> DocumentRuntime {
        DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord {
                block_id: 1,
                content_version: 1,
                kind: RichBlockKind::Paragraph,
                payload: BlockPayload::RichText {
                    spans: vec![InlineSpan {
                        text: "a".to_owned(),
                        marks: vec![InlineMark::Bold],
                    }],
                },
            }],
            720.0,
        )
    }

    let mut edited = runtime();
    edited.focus_block_at_offset(1, 1).unwrap();
    edited.insert_char('x').unwrap();
    let transactions = edited.drain_pending_structure_transactions();
    assert_eq!(transactions.len(), 1);
    let [EditOperation::Text(TextEditOperation::ReplaceSpans { new_spans, .. })] =
        transactions[0].ops.as_slice()
    else {
        panic!("single-char path must record one typed text operation");
    };
    assert_eq!(new_spans[0].marks, vec![InlineMark::Bold]);

    let mut replayed = runtime();
    replayed
        .apply_external_transaction(&transactions[0], cditor_core::edit::ChangeOrigin::Remote)
        .expect("replay typed hot-path transaction");
    assert_eq!(
        replayed.block_payload_record(1).unwrap().payload,
        edited.block_payload_record(1).unwrap().payload
    );
}

#[test]
fn stale_block_attrs_roll_back_an_earlier_typed_text_edit() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "hello",
        )],
        720.0,
    );
    let revision_before = runtime.revision();
    let stale_attrs = BlockAttrs {
        indent: 2,
        ..BlockAttrs::default()
    };
    let after_attrs = BlockAttrs {
        color: Some("#d44c47".to_owned()),
        ..BlockAttrs::default()
    };

    let error = runtime
        .apply_transaction(
            &tx(
                vec![
                    EditOperation::Text(TextEditOperation::ReplaceSpans {
                        surface_id: SurfaceId::Block(1),
                        range: 0..5,
                        old_spans: vec![InlineSpan::plain("hello")],
                        new_spans: vec![InlineSpan::plain("changed")],
                    }),
                    EditOperation::Block(BlockEditOperation::SetAttrs {
                        block_id: 1,
                        before: stale_attrs,
                        after: after_attrs,
                    }),
                ],
                Vec::new(),
            ),
            TransactionPermissionSet::FULL_ACCESS,
        )
        .expect_err("stale attrs must reject all staged operations");

    assert!(matches!(
        error,
        TransactionApplyError::Precondition { op_index: 1, .. }
    ));
    assert_eq!(
        runtime.block_payload_record(1).unwrap().plain_text(),
        "hello"
    );
    assert_eq!(runtime.block_attrs(1), BlockAttrs::default());
    assert_eq!(runtime.revision(), revision_before);
}

#[test]
fn collection_schema_view_record_and_value_share_one_undoable_transaction() {
    let (mut runtime, initial) = collection_runtime();
    let collection_id = initial.collection_id;
    let title_property_id = initial.properties[0].property_id;
    let initial_view_id = initial.active_view_id.unwrap();
    let number_property = CollectionPropertyPayload {
        property_id: 44,
        name: "Score".to_owned(),
        kind: CollectionPropertyKind::Number,
        required: false,
    };
    let board_view = CollectionViewPayload {
        view_id: 55,
        name: "Board".to_owned(),
        layout: CollectionViewLayout::Board,
    };
    let record = CollectionRecordSnapshot {
        record_id: 66,
        collection_id,
        page_block_id: None,
        values: vec![
            (
                title_property_id,
                CollectionPropertyValue::RichText(RichTextContent::plain("Alpha")),
            ),
            (44, CollectionPropertyValue::Number(1.0)),
        ],
    };
    let forward = vec![
        EditOperation::Collection(CollectionEditOperation::InsertProperty {
            block_id: 1,
            collection_id,
            index: 1,
            property: number_property.clone(),
        }),
        EditOperation::Collection(CollectionEditOperation::InsertView {
            block_id: 1,
            collection_id,
            index: 1,
            view: board_view.clone(),
        }),
        EditOperation::Collection(CollectionEditOperation::SetActiveView {
            block_id: 1,
            collection_id,
            before: initial_view_id,
            after: board_view.view_id,
        }),
        EditOperation::Collection(CollectionEditOperation::InsertRecord {
            block_id: 1,
            index: 0,
            record: record.clone(),
        }),
        EditOperation::Collection(CollectionEditOperation::SetRecordValue {
            block_id: 1,
            collection_id,
            record_id: record.record_id,
            property_id: number_property.property_id,
            before: CollectionPropertyValue::Number(1.0),
            after: CollectionPropertyValue::Number(2.0),
        }),
    ];
    let inverse = vec![
        EditOperation::Collection(CollectionEditOperation::SetRecordValue {
            block_id: 1,
            collection_id,
            record_id: record.record_id,
            property_id: number_property.property_id,
            before: CollectionPropertyValue::Number(2.0),
            after: CollectionPropertyValue::Number(1.0),
        }),
        EditOperation::Collection(CollectionEditOperation::DeleteRecord {
            block_id: 1,
            index: 0,
            record: record.clone(),
        }),
        EditOperation::Collection(CollectionEditOperation::SetActiveView {
            block_id: 1,
            collection_id,
            before: board_view.view_id,
            after: initial_view_id,
        }),
        EditOperation::Collection(CollectionEditOperation::DeleteView {
            block_id: 1,
            collection_id,
            index: 1,
            view: board_view.clone(),
        }),
        EditOperation::Collection(CollectionEditOperation::DeleteProperty {
            block_id: 1,
            collection_id,
            index: 1,
            property: number_property.clone(),
        }),
    ];

    runtime
        .apply_transaction(&tx(forward, inverse), TransactionPermissionSet::FULL_ACCESS)
        .expect("collection transaction");
    let payload = runtime.block_payload_record(1).unwrap();
    let BlockPayload::Collection(collection) = payload.payload else {
        panic!("collection payload");
    };
    assert_eq!(collection.active_view_id, Some(board_view.view_id));
    assert_eq!(collection.properties.len(), 2);
    assert_eq!(runtime.collection_records_snapshot(collection_id).len(), 1);
    assert!(runtime.undo_focused_block().unwrap());
    assert!(
        runtime
            .collection_records_snapshot(collection_id)
            .is_empty()
    );
    let BlockPayload::Collection(collection) = runtime.block_payload_record(1).unwrap().payload
    else {
        panic!("collection payload");
    };
    assert_eq!(collection.active_view_id, Some(initial_view_id));
    assert_eq!(collection.properties.len(), 1);
    assert_eq!(collection.views.len(), 1);
}

#[test]
fn comment_and_asset_overlays_roll_back_together_on_stale_metadata() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "hello",
        )],
        720.0,
    );
    let thread = CommentThreadSnapshot {
        thread_id: 10,
        anchor: CommentAnchor {
            surface_id: SurfaceId::Block(1),
            range: 0..5,
            quoted_text: "hello".to_owned(),
        },
        resolved: false,
        messages: vec![CommentMessageSnapshot {
            comment_id: 11,
            author_id: 12,
            body: RichTextContent::plain("Review"),
            created_at_ms: 1,
            edited_at_ms: None,
        }],
    };
    let asset = AssetSnapshot {
        asset_id: 20,
        file_name: "image.png".to_owned(),
        media_type: "image/png".to_owned(),
        size_bytes: 128,
        source: "asset://20".to_owned(),
        checksum: Some("abc".to_owned()),
        state: AssetState::Ready,
    };
    runtime
        .apply_transaction(
            &tx(
                vec![
                    EditOperation::Comment(CommentEditOperation::CreateThread {
                        thread: thread.clone(),
                    }),
                    EditOperation::Asset(AssetEditOperation::Attach {
                        block_id: 1,
                        asset: asset.clone(),
                    }),
                ],
                Vec::new(),
            ),
            TransactionPermissionSet::FULL_ACCESS,
        )
        .expect("create metadata");
    assert_eq!(runtime.comment_thread_snapshot(10), Some(&thread));
    assert_eq!(runtime.asset_snapshot(20), Some(&asset));

    let mut stale_asset = asset.clone();
    stale_asset.checksum = Some("stale".to_owned());
    let error = runtime
        .apply_transaction(
            &tx(
                vec![
                    EditOperation::Comment(CommentEditOperation::SetResolved {
                        thread_id: 10,
                        before: false,
                        after: true,
                    }),
                    EditOperation::Asset(AssetEditOperation::Update {
                        block_id: 1,
                        before: stale_asset,
                        after: asset.clone(),
                    }),
                ],
                Vec::new(),
            ),
            TransactionPermissionSet::FULL_ACCESS,
        )
        .expect_err("stale asset metadata must roll back comment update");
    assert!(matches!(
        error,
        TransactionApplyError::Precondition { op_index: 1, .. }
    ));
    assert!(!runtime.comment_thread_snapshot(10).unwrap().resolved);
    assert_eq!(runtime.asset_snapshot(20), Some(&asset));
    assert_eq!(runtime.attached_asset_ids(1), vec![20]);
}

#[test]
fn domain_permissions_are_checked_before_staging() {
    let (mut runtime, collection) = collection_runtime();
    let before = runtime.revision();
    let error = runtime
        .apply_transaction(
            &tx(
                vec![EditOperation::Collection(
                    CollectionEditOperation::SetTitle {
                        block_id: 1,
                        collection_id: collection.collection_id,
                        before: collection.title,
                        after: RichTextContent::plain("Denied"),
                    },
                )],
                Vec::new(),
            ),
            TransactionPermissionSet {
                edit_document: true,
                manage_collection: false,
                comment: true,
                manage_assets: true,
            },
        )
        .expect_err("collection permission is required");
    assert_eq!(
        error,
        TransactionApplyError::PermissionDenied(TransactionPermission::ManageCollection)
    );
    assert_eq!(runtime.revision(), before);
}
