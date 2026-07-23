use cditor_core::edit::{BlockEditOperation, ChangeOrigin, TransactionPrecondition};

use super::*;

fn paragraph_runtime(text: &str) -> DocumentRuntime {
    DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            text,
        )],
        720.0,
    )
}

fn replay_on(mut runtime: DocumentRuntime, transaction: &EditTransaction) -> DocumentRuntime {
    runtime
        .apply_external_transaction(transaction, ChangeOrigin::Remote)
        .expect("recorded local transaction must replay");
    runtime
}

#[test]
fn platform_typing_records_compact_replayable_transaction_with_ux_metadata() {
    let mut edited = paragraph_runtime("hello");
    edited.focus_block_at_offset(1, 5).unwrap();

    assert!(edited.replace_text_from_platform(None, "!").unwrap());
    let transactions = edited.drain_pending_structure_transactions();
    assert_eq!(transactions.len(), 1);
    let transaction = &transactions[0];
    assert_eq!(transaction.kind, EditTransactionKind::Typing);
    assert_eq!(transaction.origin, ChangeOrigin::User);
    assert_eq!(
        transaction.preconditions,
        vec![TransactionPrecondition::BlockContentVersion {
            block_id: 1,
            version: 1,
        }]
    );
    assert!(transaction.before_selection.is_some());
    assert!(transaction.after_selection.is_some());
    assert!(transaction.before_anchor.is_some());
    assert!(transaction.after_anchor.is_some());
    let [
        EditOperation::Text(TextEditOperation::ReplaceSpans {
            surface_id,
            range,
            old_spans,
            new_spans,
        }),
    ] = transaction.ops.as_slice()
    else {
        panic!("platform typing must produce one typed text operation");
    };
    assert_eq!(*surface_id, SurfaceId::Block(1));
    assert_eq!(range.clone(), 5..5);
    assert!(old_spans.is_empty());
    assert_eq!(new_spans, &vec![InlineSpan::plain("!")]);

    let replayed = replay_on(paragraph_runtime("hello"), transaction);
    assert_eq!(
        replayed.block_payload_record(1).unwrap().payload,
        edited.block_payload_record(1).unwrap().payload
    );
}

#[test]
fn explicit_replacement_carries_exact_forward_and_inverse_spans() {
    let mut edited = paragraph_runtime("hello");
    edited.focus_block_at_offset(1, 4).unwrap();
    assert!(
        edited
            .replace_text_in_focused_range(Some(1..4), "i")
            .unwrap()
    );
    let transaction = edited.drain_pending_structure_transactions().remove(0);
    assert_eq!(transaction.kind, EditTransactionKind::ExplicitCommand);
    let [
        EditOperation::Text(TextEditOperation::ReplaceSpans {
            range,
            old_spans,
            new_spans,
            ..
        }),
    ] = transaction.ops.as_slice()
    else {
        panic!("replacement must use typed spans");
    };
    assert_eq!(range.clone(), 1..4);
    assert_eq!(old_spans, &vec![InlineSpan::plain("ell")]);
    assert_eq!(new_spans, &vec![InlineSpan::plain("i")]);
    let [
        EditOperation::Text(TextEditOperation::ReplaceSpans {
            range,
            old_spans,
            new_spans,
            ..
        }),
    ] = transaction.inverse_ops.as_slice()
    else {
        panic!("replacement inverse must use typed spans");
    };
    assert_eq!(range.clone(), 1..2);
    assert_eq!(old_spans, &vec![InlineSpan::plain("i")]);
    assert_eq!(new_spans, &vec![InlineSpan::plain("ell")]);

    let replayed = replay_on(paragraph_runtime("hello"), &transaction);
    assert_eq!(
        replayed.block_payload_record(1).unwrap().plain_text(),
        "hio"
    );
}

#[test]
fn empty_preapplied_replacement_is_a_true_zero_mutation_noop() {
    let mut runtime = paragraph_runtime("hello");
    runtime.focus_block_at_offset(1, 2).unwrap();
    let before_payload = runtime.block_payload_record(1).unwrap();
    let before_transaction_id = runtime.next_transaction_id;
    let before_undo_events = runtime.undo_events.clone();
    let before_layout = runtime.document.index.layout_meta[0];

    assert!(
        !runtime
            .replace_text_in_focused_range(Some(2..2), "")
            .unwrap()
    );

    assert_eq!(runtime.block_payload_record(1).unwrap(), before_payload);
    assert_eq!(runtime.next_transaction_id, before_transaction_id);
    assert_eq!(runtime.undo_events, before_undo_events);
    assert_eq!(runtime.document.index.layout_meta[0], before_layout);
    assert_eq!(runtime.pending_structure_transaction_count(), 0);
}

#[test]
fn divergent_live_text_is_rejected_before_any_preapplied_mutation() {
    let mut runtime = paragraph_runtime("hello");
    runtime.focus_block_at_offset(1, 2).unwrap();
    runtime
        .document
        .text_models
        .insert(1, PieceTableTextModel::new("stale live model"));
    let before_payload = runtime.block_payload_record(1).unwrap();
    let before_transaction_id = runtime.next_transaction_id;
    let before_undo_events = runtime.undo_events.clone();
    let before_pending = runtime.pending_structure_transaction_count();

    let error = runtime
        .replace_text_in_focused_range(Some(2..2), "X")
        .expect_err("divergent live state must fail preflight");

    assert!(error.contains("diverges from authoritative payload"));
    assert_eq!(runtime.block_payload_record(1).unwrap(), before_payload);
    assert_eq!(
        runtime.document.text_models.get(&1).unwrap().text(),
        "stale live model"
    );
    assert_eq!(runtime.next_transaction_id, before_transaction_id);
    assert_eq!(runtime.undo_events, before_undo_events);
    assert_eq!(
        runtime.pending_structure_transaction_count(),
        before_pending
    );
}

#[test]
fn forbidden_hot_path_work_fails_before_undo_typing_or_document_mutation() {
    let mut runtime = paragraph_runtime("hello");
    runtime.focus_block_at_offset(1, 2).unwrap();
    runtime.selection.selected_block_ids.insert(1);
    runtime.editing.hot_path.forbidden_sync_work.sqlite_write = true;
    let before_payload = runtime.block_payload_record(1).unwrap();
    let before_model = runtime.document.text_models.get(&1).unwrap().clone();
    let before_editing = runtime.editing.session.clone();
    let before_undo_stacks = runtime.undo_stacks.clone();
    let before_redo_stacks = runtime.redo_stacks.clone();
    let before_undo_events = runtime.undo_events.clone();
    let before_redo_events = runtime.redo_events.clone();
    let before_selected_blocks = runtime.selection.selected_block_ids.clone();
    let before_transaction_id = runtime.next_transaction_id;
    let before_layout = runtime.document.index.layout_meta[0];

    let error = runtime
        .insert_char('X')
        .expect_err("forbidden synchronous work must reject input");

    assert!(error.contains("ForbiddenSyncWork(\"sqlite_write\")"));
    assert_eq!(runtime.block_payload_record(1).unwrap(), before_payload);
    assert_eq!(runtime.document.text_models.get(&1), Some(&before_model));
    assert_eq!(runtime.editing.session, before_editing);
    assert_eq!(runtime.undo_stacks, before_undo_stacks);
    assert_eq!(runtime.redo_stacks, before_redo_stacks);
    assert_eq!(runtime.undo_events, before_undo_events);
    assert_eq!(runtime.redo_events, before_redo_events);
    assert_eq!(runtime.selection.selected_block_ids, before_selected_blocks);
    assert_eq!(runtime.next_transaction_id, before_transaction_id);
    assert_eq!(runtime.document.index.layout_meta[0], before_layout);
    assert_eq!(runtime.pending_structure_transaction_count(), 0);
    assert!(runtime.typing_undo_group.is_none());
    assert!(runtime.pending_typing_undo.is_none());
}

#[test]
fn block_backspace_records_grapheme_safe_replayable_transaction() {
    let family = "👨‍👩‍👧‍👦";
    let initial = format!("a{family}b");
    let mut edited = paragraph_runtime(&initial);
    let caret = 1 + family.len();
    edited.focus_block_at_offset(1, caret).unwrap();

    assert!(edited.delete_backward().unwrap());
    assert_eq!(edited.block_payload_record(1).unwrap().plain_text(), "ab");
    let transactions = edited.drain_pending_structure_transactions();
    assert_eq!(transactions.len(), 1);
    let transaction = &transactions[0];
    assert_eq!(transaction.kind, EditTransactionKind::ExplicitCommand);
    assert_eq!(transaction.origin, ChangeOrigin::User);
    let [
        EditOperation::Text(TextEditOperation::ReplaceSpans {
            range,
            old_spans,
            new_spans,
            ..
        }),
    ] = transaction.ops.as_slice()
    else {
        panic!("backspace must produce one typed text operation");
    };
    assert_eq!(range.clone(), 1..caret);
    assert_eq!(old_spans, &vec![InlineSpan::plain(family)]);
    assert!(new_spans.is_empty());
    let [
        EditOperation::Text(TextEditOperation::ReplaceSpans {
            range,
            old_spans,
            new_spans,
            ..
        }),
    ] = transaction.inverse_ops.as_slice()
    else {
        panic!("backspace inverse must restore one typed text range");
    };
    assert_eq!(range.clone(), 1..1);
    assert!(old_spans.is_empty());
    assert_eq!(new_spans, &vec![InlineSpan::plain(family)]);

    let replayed = replay_on(paragraph_runtime(&initial), transaction);
    assert_eq!(replayed.block_payload_record(1).unwrap().plain_text(), "ab");
    assert!(edited.undo_focused_block().unwrap());
    assert_eq!(
        edited.block_payload_record(1).unwrap().plain_text(),
        initial
    );
}

#[test]
fn ime_commit_is_one_ime_origin_transaction_for_block_and_table_surfaces() {
    let mut block = paragraph_runtime("ab");
    block.focus_block_at_offset(1, 1).unwrap();
    block.begin_or_update_composition(1, 1..1, "中").unwrap();
    assert!(block.commit_composition().unwrap());
    let transactions = block.drain_pending_structure_transactions();
    assert_eq!(transactions.len(), 1);
    assert_eq!(transactions[0].kind, EditTransactionKind::CompositionCommit);
    assert_eq!(transactions[0].origin, ChangeOrigin::Ime);
    assert!(matches!(
        transactions[0].ops.as_slice(),
        [EditOperation::Text(TextEditOperation::ReplaceSpans {
            surface_id: SurfaceId::Block(1),
            ..
        })]
    ));

    let table_record = sample_table_payload();
    let mut table = DocumentRuntime::from_payloads(1, vec![table_record.clone()], 720.0);
    table.focus_table_cell_at_offset(10, 0, 0, 1).unwrap();
    table.begin_or_update_composition(10, 1..1, "文").unwrap();
    assert!(table.commit_composition().unwrap());
    let transaction = table.drain_pending_structure_transactions().remove(0);
    assert_eq!(transaction.kind, EditTransactionKind::CompositionCommit);
    assert_eq!(transaction.origin, ChangeOrigin::Ime);
    assert!(matches!(
        transaction.ops.as_slice(),
        [EditOperation::Text(TextEditOperation::ReplaceSpans {
            surface_id: SurfaceId::TableCell {
                block_id: 10,
                row: 0,
                column: 0,
            },
            ..
        })]
    ));
    let replayed = replay_on(
        DocumentRuntime::from_payloads(1, vec![table_record], 720.0),
        &transaction,
    );
    assert_eq!(replayed.table_cell_plain_text(10, 0, 0).unwrap(), "A文");
}

#[test]
fn auxiliary_surface_typing_records_stable_surface_ids_and_replays() {
    let collection = cditor_core::rich_text::CollectionPayload::for_block(20, "Projects");
    let image = cditor_core::rich_text::ImagePayload {
        caption: cditor_core::rich_text::RichTextContent::plain("caption"),
        ..Default::default()
    };
    let records = vec![
        BlockPayloadRecord {
            block_id: 10,
            content_version: 1,
            kind: RichBlockKind::Image,
            payload: BlockPayload::Image(image),
        },
        BlockPayloadRecord {
            block_id: 20,
            content_version: 1,
            kind: RichBlockKind::Database,
            payload: BlockPayload::Collection(collection),
        },
    ];
    let mut edited = DocumentRuntime::from_payloads(1, records.clone(), 720.0);
    edited
        .focus_text_surface_at_offset(SurfaceId::ImageCaption { block_id: 10 }, 7)
        .unwrap();
    assert!(edited.replace_text_from_platform(None, "!").unwrap());
    edited
        .focus_text_surface_at_offset(SurfaceId::CollectionTitle { block_id: 20 }, 8)
        .unwrap();
    assert!(edited.replace_text_from_platform(None, "!").unwrap());
    let transactions = edited.drain_pending_structure_transactions();
    assert_eq!(transactions.len(), 2);
    assert!(matches!(
        transactions[0].ops.as_slice(),
        [EditOperation::Text(TextEditOperation::ReplaceSpans {
            surface_id: SurfaceId::ImageCaption { block_id: 10 },
            ..
        })]
    ));
    assert!(matches!(
        transactions[1].ops.as_slice(),
        [EditOperation::Text(TextEditOperation::ReplaceSpans {
            surface_id: SurfaceId::CollectionTitle { block_id: 20 },
            ..
        })]
    ));

    let mut replayed = DocumentRuntime::from_payloads(1, records, 720.0);
    for transaction in &transactions {
        replayed
            .apply_external_transaction(transaction, ChangeOrigin::Remote)
            .unwrap();
    }
    assert_eq!(
        replayed.block_payload_record(10).unwrap().payload,
        edited.block_payload_record(10).unwrap().payload
    );
    assert_eq!(
        replayed.block_payload_record(20).unwrap().payload,
        edited.block_payload_record(20).unwrap().payload
    );
}

#[test]
fn every_preapplied_text_surface_replays_to_identical_payload_and_versions() {
    let records = vec![
        BlockPayloadRecord {
            block_id: 1,
            content_version: 1,
            kind: RichBlockKind::Paragraph,
            payload: BlockPayload::RichText {
                spans: vec![InlineSpan {
                    text: "ab".to_owned(),
                    marks: vec![InlineMark::Bold],
                }],
            },
        },
        BlockPayloadRecord {
            block_id: 2,
            content_version: 1,
            kind: RichBlockKind::Code {
                language: Some("rust".to_owned()),
            },
            payload: BlockPayload::Code {
                language: Some("rust".to_owned()),
                text: "fn".to_owned(),
            },
        },
        sample_table_payload(),
        BlockPayloadRecord {
            block_id: 20,
            content_version: 1,
            kind: RichBlockKind::Image,
            payload: BlockPayload::Image(cditor_core::rich_text::ImagePayload {
                caption: "cap".into(),
                ..Default::default()
            }),
        },
        BlockPayloadRecord {
            block_id: 30,
            content_version: 1,
            kind: RichBlockKind::Database,
            payload: BlockPayload::Collection(
                cditor_core::rich_text::CollectionPayload::for_block(30, "Tasks"),
            ),
        },
    ];
    let mut edited = DocumentRuntime::from_payloads(1, records.clone(), 720.0);

    edited.focus_block_at_offset(1, 1).unwrap();
    edited.insert_char('X').unwrap();
    edited.focus_block_at_offset(2, 2).unwrap();
    assert!(edited.replace_text_from_platform(None, "!").unwrap());
    edited.focus_table_cell_at_offset(10, 0, 0, 1).unwrap();
    assert!(edited.replace_text_from_platform(None, "中").unwrap());
    edited
        .focus_text_surface_at_offset(SurfaceId::ImageCaption { block_id: 20 }, 3)
        .unwrap();
    assert!(edited.replace_text_from_platform(None, "!").unwrap());
    edited
        .focus_text_surface_at_offset(SurfaceId::CollectionTitle { block_id: 30 }, 5)
        .unwrap();
    assert!(edited.replace_text_from_platform(None, "!").unwrap());

    let transactions = edited.drain_pending_structure_transactions();
    assert_eq!(transactions.len(), 5);
    assert!(transactions.iter().all(|transaction| matches!(
        transaction.ops.as_slice(),
        [EditOperation::Text(TextEditOperation::ReplaceSpans { .. })]
    )));

    let mut replayed = DocumentRuntime::from_payloads(1, records, 720.0);
    for transaction in &transactions {
        replayed
            .apply_external_transaction(transaction, ChangeOrigin::Remote)
            .unwrap();
    }
    for block_id in [1, 2, 10, 20, 30] {
        assert_eq!(
            replayed.block_payload_record(block_id),
            edited.block_payload_record(block_id),
            "payload/content version diverged for block {block_id}"
        );
        assert_eq!(
            replayed.block_layout_version(block_id),
            edited.block_layout_version(block_id),
            "layout version diverged for block {block_id}"
        );
    }
}

#[test]
fn block_markdown_shortcut_records_payload_transition_instead_of_partial_text() {
    let mut edited = paragraph_runtime("#");
    edited.focus_block_at_offset(1, 1).unwrap();
    assert!(edited.replace_text_from_platform(None, " ").unwrap());
    let transaction = edited.drain_pending_structure_transactions().remove(0);
    assert_eq!(transaction.kind, EditTransactionKind::Typing);
    assert!(matches!(
        transaction.ops.as_slice(),
        [EditOperation::Block(BlockEditOperation::ReplacePayload {
            block_id: 1,
            before_kind: RichBlockKind::Paragraph,
            after_kind: RichBlockKind::Heading { level: 1 },
            ..
        })]
    ));

    let replayed = replay_on(paragraph_runtime("#"), &transaction);
    let payload = replayed.block_payload_record(1).unwrap();
    assert_eq!(payload.kind, RichBlockKind::Heading { level: 1 });
    assert_eq!(payload.plain_text(), "");
}

#[test]
fn inline_markdown_shortcut_records_full_replayable_transition_and_resets_typing_marks() {
    let mut edited = paragraph_runtime("");
    edited.focus_block_at_offset(1, 0).unwrap();
    for ch in "**ad**x".chars() {
        edited.insert_char(ch).unwrap();
    }
    let BlockPayload::RichText { spans } = edited.block_payload_record(1).unwrap().payload else {
        panic!("rich text payload");
    };
    assert_eq!(spans.len(), 2);
    assert_eq!(spans[0].text, "ad");
    assert_eq!(spans[0].marks, vec![InlineMark::Bold]);
    assert_eq!(spans[1], InlineSpan::plain("x"));

    let transactions = edited.drain_pending_structure_transactions();
    assert_eq!(transactions.len(), "**ad**x".chars().count());
    assert!(transactions.iter().any(|transaction| {
        matches!(
            transaction.ops.as_slice(),
            [EditOperation::Text(TextEditOperation::ReplaceSpans {
                range,
                new_spans,
                ..
            })] if range.start == 0
                && new_spans.iter().any(|span| span.marks == vec![InlineMark::Bold])
        )
    }));

    let mut replayed = paragraph_runtime("");
    for transaction in &transactions {
        replayed
            .apply_external_transaction(transaction, ChangeOrigin::Remote)
            .unwrap();
    }
    assert_eq!(
        replayed.block_payload_record(1).unwrap().payload,
        BlockPayload::RichText { spans }
    );
}

#[test]
fn markdown_shortcuts_advance_content_and_layout_versions_once_per_transaction() {
    let mut block_shortcut = paragraph_runtime("#");
    block_shortcut.focus_block_at_offset(1, 1).unwrap();
    let before_content = block_shortcut.block_content_version(1).unwrap();
    let block_index = block_shortcut.document.index.index_of(1).unwrap();
    let before_layout = block_shortcut.document.index.layout_meta[block_index].layout_version;

    assert!(
        block_shortcut
            .replace_text_from_platform(None, " ")
            .unwrap()
    );
    assert_eq!(
        block_shortcut.block_content_version(1),
        Some(before_content + 1)
    );
    assert_eq!(
        block_shortcut.document.index.layout_meta[block_index].layout_version,
        before_layout + 1
    );

    let mut inline_shortcut = paragraph_runtime("");
    inline_shortcut.focus_block_at_offset(1, 0).unwrap();
    for ch in "**ad**".chars() {
        let before_content = inline_shortcut.block_content_version(1).unwrap();
        let block_index = inline_shortcut.document.index.index_of(1).unwrap();
        let before_layout = inline_shortcut.document.index.layout_meta[block_index].layout_version;

        inline_shortcut.insert_char(ch).unwrap();

        assert_eq!(
            inline_shortcut.block_content_version(1),
            Some(before_content + 1),
            "content version must advance once for input {ch:?}"
        );
        assert_eq!(
            inline_shortcut.document.index.layout_meta[block_index].layout_version,
            before_layout + 1,
            "layout version must advance once for input {ch:?}"
        );
    }
    let BlockPayload::RichText { spans } = inline_shortcut.block_payload_record(1).unwrap().payload
    else {
        panic!("inline shortcut must preserve a rich text payload");
    };
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].text, "ad");
    assert_eq!(spans[0].marks, vec![InlineMark::Bold]);
}

#[test]
fn rich_clipboard_paste_uses_import_origin_and_one_external_undo_transaction() {
    let mut runtime = paragraph_runtime("hello");
    runtime.focus_block_at_offset(1, 5).unwrap();
    runtime.set_document_text_selection(1, 0, 1, 5).unwrap();
    let selection = cditor_import_export::clipboard::ClipboardSelection::Inline {
        spans: vec![InlineSpan {
            text: "hi".to_owned(),
            marks: vec![InlineMark::Bold],
        }],
    };
    assert!(runtime.paste_clipboard_selection(&selection).unwrap());
    let transactions = runtime.drain_pending_structure_transactions();
    assert_eq!(transactions.len(), 1);
    assert_eq!(transactions[0].kind, EditTransactionKind::Paste);
    assert_eq!(transactions[0].origin, ChangeOrigin::Import);
    assert!(runtime.undo_stacks.get(&1).is_none_or(Vec::is_empty));
    assert_eq!(runtime.external_undo_stack.len(), 1);
    assert!(runtime.undo_focused_block().unwrap());
    assert_eq!(
        runtime.block_payload_record(1).unwrap().plain_text(),
        "hello"
    );
    assert!(runtime.redo_focused_block().unwrap());
    let BlockPayload::RichText { spans } = runtime.block_payload_record(1).unwrap().payload else {
        panic!("rich text payload");
    };
    assert_eq!(spans[0].text, "hi");
    assert_eq!(spans[0].marks, vec![InlineMark::Bold]);
}
