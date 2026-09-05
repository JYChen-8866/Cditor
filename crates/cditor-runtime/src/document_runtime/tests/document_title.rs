use super::*;
use cditor_core::edit::ChangeOrigin;

fn title_id(runtime: &DocumentRuntime) -> BlockId {
    runtime
        .document
        .index
        .block_ids
        .iter()
        .copied()
        .find(|block_id| runtime.kind_for_block(*block_id).is_document_title())
        .expect("document title block")
}

#[test]
fn empty_document_starts_with_one_system_title_and_a_body_paragraph() {
    let runtime = DocumentRuntime::empty();

    assert_eq!(runtime.document_block_count(), 2);
    assert!(
        runtime
            .kind_for_block(runtime.document.index.block_ids[0])
            .is_document_title()
    );
    assert!(matches!(
        runtime.kind_for_block(runtime.document.index.block_ids[1]),
        RichBlockKind::Paragraph
    ));
    assert_eq!(
        runtime
            .document
            .index
            .block_ids
            .iter()
            .filter(|block_id| runtime.kind_for_block(**block_id).is_document_title())
            .count(),
        1
    );
}

#[test]
fn document_title_uses_the_shared_realtime_composition_pipeline() {
    let mut runtime = DocumentRuntime::empty();
    let title_id = title_id(&runtime);
    runtime.focus_block_at_offset(title_id, 0).unwrap();
    let expected = runtime.input_session_identity().unwrap();

    let preview = runtime
        .apply_realtime_input(RealtimeInputRequest {
            expected,
            input: RealtimeInput::UpdateComposition {
                range: 0..0,
                text: "中文标题",
                selected_range: Some("中文标题".len().."中文标题".len()),
            },
        })
        .unwrap();

    assert_eq!(runtime.document_name(), None);
    assert!(runtime.active_composition().is_some());
    let preview_identity = preview.input_identity.unwrap();
    assert_eq!(preview_identity.target, expected.target);
    assert_eq!(preview_identity.session_id, expected.session_id);

    runtime
        .apply_realtime_input(RealtimeInputRequest {
            expected: preview_identity,
            input: RealtimeInput::UnmarkComposition,
        })
        .unwrap();

    assert_eq!(runtime.document_name(), Some("中文标题"));
    assert!(runtime.active_composition().is_none());
    assert_eq!(
        runtime.block_payload_record(title_id).unwrap().plain_text(),
        "中文标题"
    );
}

#[test]
fn title_enter_and_boundary_delete_preserve_the_header_body_boundary() {
    let mut runtime = DocumentRuntime::empty();
    let title_id = title_id(&runtime);
    let body_id = runtime.document.index.block_ids[1];

    runtime.focus_block_at_offset(title_id, 0).unwrap();
    assert!(!runtime.delete_backward().unwrap());
    runtime.handle_enter().unwrap();
    assert_eq!(runtime.focused_block_id(), Some(body_id));
    assert_eq!(runtime.document_block_count(), 2);

    runtime.focus_block_at_offset(body_id, 0).unwrap();
    assert!(!runtime.delete_backward().unwrap());
    assert_eq!(runtime.document.index.block_ids, vec![title_id, body_id]);
    assert!(!runtime.can_delete_block(title_id));
    assert!(!runtime.can_indent_focused_block());
}

#[test]
fn body_h1_never_becomes_the_document_name() {
    let mut document = RichTextDocument::empty(7);
    document.metadata.name = Some("Document name".to_owned());
    document.push_root_block(RichBlockRecord::heading(1, 1, "Body heading"));

    let runtime = DocumentRuntime::from_rich_text_document(document, 720.0);
    assert_eq!(runtime.document_name(), Some("Document name"));
    assert!(
        runtime
            .kind_for_block(runtime.document.index.block_ids[0])
            .is_document_title()
    );
    assert!(matches!(
        runtime.kind_for_block(1),
        RichBlockKind::Heading { level: 1 }
    ));
}

#[test]
fn empty_system_title_does_not_fall_back_to_stale_metadata() {
    let mut document = RichTextDocument::empty(7);
    document.metadata.name = Some("Legacy name".to_owned());
    document.push_root_block(RichBlockRecord::rich_text(
        2,
        RichBlockKind::DocumentTitle,
        "",
    ));
    document.push_root_block(RichBlockRecord::paragraph(1, "body"));

    let runtime = DocumentRuntime::from_rich_text_document(document, 720.0);
    assert_eq!(runtime.document_name(), None);
    assert_eq!(runtime.rich_text_document().metadata.name, None);
}

#[test]
fn untitled_legacy_page_is_migrated_but_embedded_composer_has_no_page_header() {
    let mut document = RichTextDocument::empty(7);
    document.push_root_block(RichBlockRecord::paragraph(1, "body"));

    let runtime = DocumentRuntime::from_rich_text_document(document, 720.0);
    assert!(
        runtime
            .kind_for_block(runtime.document.index.block_ids[0])
            .is_document_title()
    );
    assert_eq!(runtime.document_name(), None);

    let composer = DocumentRuntime::empty_composer();
    assert_eq!(composer.document_block_count(), 1);
    assert!(matches!(
        composer.kind_for_block(1),
        RichBlockKind::Paragraph
    ));
}

#[test]
fn whole_document_export_syncs_title_text_back_to_compatibility_metadata() {
    let mut runtime = DocumentRuntime::empty();
    let title_id = title_id(&runtime);
    runtime.focus_block_at_offset(title_id, 0).unwrap();
    runtime
        .replace_focused_range_with_rich_text_spans(&[InlineSpan::plain("Exported title")])
        .unwrap();

    let exported = runtime.rich_text_document();
    assert_eq!(exported.metadata.name.as_deref(), Some("Exported title"));
    assert!(exported.metadata.title.is_none());
    assert!(exported.blocks[0].kind.is_document_title());
}

#[test]
fn external_transactions_cannot_delete_move_or_duplicate_the_system_title() {
    let runtime = DocumentRuntime::empty();
    let title_id = title_id(&runtime);
    let body_id = runtime.document.index.block_ids[1];

    let attempts = [
        vec![EditOperation::DeleteBlock { block_id: title_id }],
        vec![EditOperation::MoveBlockToParent {
            block_id: title_id,
            parent_id: Some(body_id),
            sibling_index: 0,
        }],
        vec![EditOperation::InsertBlock {
            index: 1,
            block: BlockIndexRecord::new(
                99,
                None,
                0,
                kind_tag_for_rich_block_kind(&RichBlockKind::DocumentTitle),
                0,
            ),
        }],
    ];

    for operations in attempts {
        let mut candidate = DocumentRuntime::empty();
        let before = candidate.rich_text_document();
        let transaction = EditTransaction::new(
            900,
            EditTransactionKind::ExplicitCommand,
            1,
            operations,
            Vec::new(),
        );

        assert!(
            candidate
                .apply_external_transaction(&transaction, ChangeOrigin::Remote)
                .is_err()
        );
        assert_eq!(candidate.rich_text_document(), before);
    }
}
