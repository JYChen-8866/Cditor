use cditor_core::edit::{BlockEditOperation, ChangeOrigin};

use super::*;

fn image_runtime() -> DocumentRuntime {
    DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord {
            block_id: 1,
            content_version: 1,
            kind: RichBlockKind::Image,
            payload: BlockPayload::Image(ImagePayload {
                source: "asset://hero".to_owned(),
                alt: "hero".to_owned(),
                caption: "caption".into(),
                display_width_ratio_milli: None,
            }),
        }],
        720.0,
    )
}

fn whiteboard_runtime() -> DocumentRuntime {
    DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord {
            block_id: 1,
            content_version: 1,
            kind: RichBlockKind::Whiteboard,
            payload: BlockPayload::Whiteboard(cditor_core::rich_text::WhiteboardPayload {
                scene_json: "{}".to_owned(),
            }),
        }],
        720.0,
    )
}

#[test]
fn image_resize_is_one_drag_transaction_and_undo_redo_round_trips() {
    let mut runtime = image_runtime();
    let revision_before = runtime.revision();
    assert!(runtime.update_image_display_width_ratio(1, 650).unwrap());
    assert_eq!(runtime.revision(), revision_before + 1);
    let transactions = runtime.drain_pending_structure_transactions();
    assert_eq!(transactions.len(), 1);
    assert_eq!(transactions[0].kind, EditTransactionKind::DragDrop);
    assert_eq!(transactions[0].origin, ChangeOrigin::User);
    assert!(matches!(
        transactions[0].ops.as_slice(),
        [EditOperation::Block(BlockEditOperation::ReplacePayload {
            block_id: 1,
            ..
        })]
    ));

    assert!(runtime.undo_focused_block().unwrap());
    let BlockPayload::Image(image) = runtime.block_payload_record(1).unwrap().payload else {
        panic!("image payload");
    };
    assert_eq!(image.display_width_ratio_milli, None);
    assert!(runtime.redo_focused_block().unwrap());
    let BlockPayload::Image(image) = runtime.block_payload_record(1).unwrap().payload else {
        panic!("image payload");
    };
    assert_eq!(image.display_width_ratio_milli, Some(650));
}

#[test]
fn whiteboard_scene_commit_is_replayable_block_transaction() {
    let mut edited = whiteboard_runtime();
    assert!(
        edited
            .update_whiteboard_scene_json(1, r#"{"elements":[{"id":"a"}]}"#)
            .unwrap()
    );
    let transaction = edited.drain_pending_structure_transactions().remove(0);
    assert_eq!(transaction.kind, EditTransactionKind::ExplicitCommand);
    assert_eq!(transaction.origin, ChangeOrigin::User);
    assert!(transaction.before_selection.is_none());
    assert!(transaction.after_selection.is_none());

    let mut replayed = whiteboard_runtime();
    replayed
        .apply_external_transaction(&transaction, ChangeOrigin::Remote)
        .unwrap();
    assert_eq!(
        replayed.block_payload_record(1).unwrap().payload,
        edited.block_payload_record(1).unwrap().payload
    );
}

#[test]
fn block_kind_language_and_todo_commands_use_replayable_block_transactions() {
    let mut conversion = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "heading",
        )],
        720.0,
    );
    conversion.focus_block_at_offset(1, 3).unwrap();
    assert!(
        conversion
            .convert_focused_block_kind(RichBlockKind::Heading { level: 2 })
            .unwrap()
    );
    let transaction = conversion.drain_pending_structure_transactions().remove(0);
    assert_eq!(transaction.kind, EditTransactionKind::ExplicitCommand);
    assert!(matches!(
        transaction.ops.as_slice(),
        [EditOperation::Block(BlockEditOperation::ReplacePayload {
            block_id: 1,
            ..
        })]
    ));
    assert!(
        conversion
            .history
            .undo_stacks
            .get(&1)
            .is_none_or(Vec::is_empty)
    );
    assert!(conversion.undo_focused_block().unwrap());
    assert_eq!(conversion.block_kind(1), Some(RichBlockKind::Paragraph));
    assert!(conversion.redo_focused_block().unwrap());
    assert_eq!(
        conversion.block_kind(1),
        Some(RichBlockKind::Heading { level: 2 })
    );

    let mut todo = DocumentRuntime::from_rich_text_document(
        {
            let mut document = RichTextDocument::empty(1);
            document.push_root_block(RichBlockRecord::todo(1, false, "ship"));
            document
        },
        720.0,
    );
    assert!(todo.toggle_todo_checked(1).unwrap());
    assert_eq!(todo.drain_pending_structure_transactions().len(), 1);
    assert!(todo.undo_focused_block().unwrap());
    assert_eq!(
        todo.block_kind(1),
        Some(RichBlockKind::Todo { checked: false })
    );

    let mut code = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord {
            block_id: 1,
            content_version: 1,
            kind: RichBlockKind::Code { language: None },
            payload: BlockPayload::Code {
                language: None,
                text: "fn main() {}".to_owned(),
            },
        }],
        720.0,
    );
    assert!(
        code.set_code_block_language(1, Some("Rust".to_owned()))
            .unwrap()
    );
    assert_eq!(code.drain_pending_structure_transactions().len(), 1);
    assert!(code.undo_focused_block().unwrap());
    assert_eq!(
        code.block_kind(1),
        Some(RichBlockKind::Code { language: None })
    );
}

#[test]
fn paragraph_insert_and_enter_split_use_payload_complete_structure_transactions() {
    let mut inserted = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "anchor",
        )],
        720.0,
    );
    inserted.focus_block_at_offset(1, 6).unwrap();
    let new_block_id = inserted.insert_paragraph_after_focused().unwrap();
    let transaction = inserted.drain_pending_structure_transactions().remove(0);
    assert!(matches!(
        transaction.ops.as_slice(),
        [EditOperation::InsertBlocks { payloads, .. }]
            if payloads.len() == 1 && payloads[0].block_id == new_block_id
    ));
    assert!(inserted.undo_focused_block().unwrap());
    assert_eq!(inserted.document.index.block_ids, vec![1]);
    assert!(inserted.redo_focused_block().unwrap());
    assert_eq!(inserted.document.index.block_ids, vec![1, new_block_id]);

    let mut split = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "abcd",
        )],
        720.0,
    );
    split.focus_block_at_offset(1, 2).unwrap();
    split.handle_enter().unwrap();
    assert_eq!(split.block_payload_record(1).unwrap().plain_text(), "ab");
    assert_eq!(split.block_payload_record(2).unwrap().plain_text(), "cd");
    let transaction = split.drain_pending_structure_transactions().remove(0);
    assert!(matches!(
        transaction.ops.as_slice(),
        [
            EditOperation::Block(_),
            EditOperation::InsertBlocks { payloads, .. }
        ] if payloads.len() == 1
    ));
    assert!(split.undo_focused_block().unwrap());
    assert_eq!(split.document.index.block_ids, vec![1]);
    assert_eq!(split.block_payload_record(1).unwrap().plain_text(), "abcd");
    assert!(split.redo_focused_block().unwrap());
    assert_eq!(split.document.index.block_ids, vec![1, 2]);
    assert_eq!(split.block_payload_record(2).unwrap().plain_text(), "cd");
}
