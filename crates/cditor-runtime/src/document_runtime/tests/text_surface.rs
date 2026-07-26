use cditor_core::ids::SurfaceId;
use cditor_core::rich_text::{
    BlockPayload, BlockPayloadRecord, ImagePayload, InlineMark, RichBlockKind,
};
use cditor_editor_protocol::{
    ProtocolErrorCode,
    command::{CommandEnvelope, CommandSource, EditorCommand},
};

use super::*;
use crate::{RichTextDelta, TextSurface, TextSurfaceRole};

fn runtime_with_auxiliary_surfaces() -> DocumentRuntime {
    DocumentRuntime::from_payloads(
        1,
        vec![
            BlockPayloadRecord {
                block_id: 10,
                content_version: 1,
                kind: RichBlockKind::Image,
                payload: BlockPayload::Image(ImagePayload {
                    source: "asset://hero".to_owned(),
                    alt: "hero".to_owned(),
                    caption: "caption".into(),
                    display_width_ratio_milli: None,
                }),
            },
            BlockPayloadRecord {
                block_id: 20,
                content_version: 1,
                kind: RichBlockKind::Database,
                payload: BlockPayload::Empty,
            },
        ],
        720.0,
    )
}

#[test]
fn registry_discovers_block_table_caption_and_collection_title_without_text_copies() {
    let mut table = sample_table_payload();
    table.block_id = 30;
    let mut payloads = vec![
        BlockPayloadRecord::rich_text(1, RichBlockKind::Paragraph, "body"),
        table,
    ];
    payloads.extend(
        runtime_with_auxiliary_surfaces()
            .document
            .payload_window
            .payloads
            .into_values()
            .map(Arc::unwrap_or_clone),
    );
    let runtime = DocumentRuntime::from_payloads(1, payloads, 720.0);
    let caption_id = SurfaceId::ImageCaption { block_id: 10 };
    let title_id = SurfaceId::CollectionTitle { block_id: 20 };

    assert_eq!(
        runtime.text_surface_ids_for_block(1),
        vec![SurfaceId::Block(1)]
    );
    assert!(
        runtime
            .text_surface_ids_for_block(30)
            .contains(&SurfaceId::TableCell {
                block_id: 30,
                row: 0,
                column: 0,
            })
    );
    assert_eq!(runtime.text_surface_ids_for_block(10), vec![caption_id]);
    assert_eq!(runtime.text_surface_ids_for_block(20), vec![title_id]);
    let caption = runtime.text_surface_snapshot(caption_id).unwrap();
    let title = runtime.text_surface_snapshot(title_id).unwrap();

    assert_eq!(caption.role, TextSurfaceRole::ImageCaption);
    assert_eq!(caption.plain_text(), "caption");
    assert!(caption.capabilities.rich_text);
    assert!(caption.capabilities.multiline);
    assert_eq!(title.role, TextSurfaceRole::CollectionTitle);
    assert!(!title.capabilities.multiline);
    assert!(title.is_empty());
    let BlockPayload::Collection(collection) =
        &runtime.document.payload_window.get(20).unwrap().payload
    else {
        panic!("database payload must normalize to a collection");
    };
    assert_eq!(collection.properties.len(), 1);
    assert_eq!(collection.views.len(), 1);
    assert!(runtime.validate_text_surface_identity(caption.identity));
}

#[test]
fn auxiliary_surface_selection_dispatch_is_session_only_and_stale_safe() {
    let mut runtime = runtime_with_auxiliary_surfaces();
    let surface_id = SurfaceId::ImageCaption { block_id: 10 };
    let before_revision = runtime.revision();
    let command = EditorCommand::SetTextSurfaceSelection {
        surface_id,
        anchor_offset: 1,
        focus_offset: 4,
        focus_affinity: TextAffinity::Upstream,
    };

    let outcome = runtime
        .dispatch(CommandEnvelope::new(command, CommandSource::Toolbar))
        .unwrap();

    assert!(outcome.changed());
    assert_eq!(outcome.affected_blocks, vec![10]);
    assert!(outcome.transaction_ids.is_empty());
    assert_eq!(runtime.focused_text_surface_id(), Some(surface_id));
    assert_eq!(runtime.text_surface_selection_range(surface_id), Some(1..4));
    assert!(!runtime.input_session_selection_reversed());
    assert_eq!(runtime.revision(), before_revision);

    let before_range = runtime.text_surface_selection_range(surface_id);
    let error = runtime
        .dispatch(
            CommandEnvelope::new(
                EditorCommand::SetTextSurfaceSelection {
                    surface_id,
                    anchor_offset: 0,
                    focus_offset: 0,
                    focus_affinity: TextAffinity::Downstream,
                },
                CommandSource::Toolbar,
            )
            .expecting_revision(before_revision + 1),
        )
        .unwrap_err();
    assert_eq!(error.code, ProtocolErrorCode::StalePrecondition);
    assert_eq!(
        runtime.text_surface_selection_range(surface_id),
        before_range
    );
    assert_eq!(runtime.revision(), before_revision);
}

#[test]
fn caption_replacement_undo_and_redo_preserve_surface_target_and_selection() {
    let mut runtime = runtime_with_auxiliary_surfaces();
    let surface_id = SurfaceId::ImageCaption { block_id: 10 };
    runtime.focus_text_surface_at_offset(surface_id, 3).unwrap();

    assert!(
        runtime
            .replace_text_in_focused_range(Some(0..3), "CAP")
            .unwrap()
    );
    assert_eq!(
        runtime
            .text_surface_snapshot(surface_id)
            .unwrap()
            .plain_text(),
        "CAPtion"
    );
    assert_eq!(runtime.focused_text_surface_id(), Some(surface_id));
    assert_eq!(runtime.text_surface_caret_offset(surface_id), Some(3));

    assert!(runtime.undo_focused_block().unwrap());
    assert_eq!(
        runtime
            .text_surface_snapshot(surface_id)
            .unwrap()
            .plain_text(),
        "caption"
    );
    assert_eq!(runtime.focused_text_surface_id(), Some(surface_id));
    assert_eq!(runtime.input_session_selected_range(), Some(3..3));

    assert!(runtime.redo_focused_block().unwrap());
    assert_eq!(
        runtime
            .text_surface_snapshot(surface_id)
            .unwrap()
            .plain_text(),
        "CAPtion"
    );
    assert_eq!(runtime.focused_text_surface_id(), Some(surface_id));
}

#[test]
fn caption_composition_is_projection_only_then_commits_as_one_undo_step() {
    let mut runtime = runtime_with_auxiliary_surfaces();
    let surface_id = SurfaceId::ImageCaption { block_id: 10 };
    runtime.focus_text_surface_at_offset(surface_id, 3).unwrap();

    runtime
        .begin_or_update_composition(10, 0..3, "说明")
        .unwrap();

    let BlockPayload::Image(image) = &runtime.document.payload_window.get(10).unwrap().payload
    else {
        panic!("expected image payload");
    };
    assert_eq!(image.caption.plain_text(), "caption");
    assert_eq!(
        runtime
            .text_surface_snapshot(surface_id)
            .unwrap()
            .plain_text(),
        "说明tion"
    );
    assert_eq!(
        runtime.active_composition_marked_range(),
        Some(0.."说明".len())
    );

    assert!(runtime.commit_composition().unwrap());
    assert_eq!(
        runtime
            .text_surface_snapshot(surface_id)
            .unwrap()
            .plain_text(),
        "说明tion"
    );
    assert_eq!(runtime.history.undo_stacks.get(&10).map(Vec::len), Some(1));
    assert!(runtime.undo_focused_block().unwrap());
    assert_eq!(
        runtime
            .text_surface_snapshot(surface_id)
            .unwrap()
            .plain_text(),
        "caption"
    );
}

#[test]
fn cross_auxiliary_surface_focus_commits_composition_before_switch() {
    let mut runtime = runtime_with_auxiliary_surfaces();
    let caption_id = SurfaceId::ImageCaption { block_id: 10 };
    let title_id = SurfaceId::CollectionTitle { block_id: 20 };
    runtime.focus_text_surface_at_offset(caption_id, 7).unwrap();
    runtime.begin_or_update_composition(10, 7..7, "中").unwrap();

    runtime.focus_text_surface_at_offset(title_id, 0).unwrap();

    assert_eq!(
        runtime
            .text_surface_snapshot(caption_id)
            .unwrap()
            .plain_text(),
        "caption中"
    );
    assert_eq!(runtime.focused_text_surface_id(), Some(title_id));
    assert!(runtime.active_composition().is_none());
}

#[test]
fn auxiliary_delete_respects_graphemes_and_never_deletes_the_owner_block_at_boundary() {
    let mut runtime = runtime_with_auxiliary_surfaces();
    let surface_id = SurfaceId::ImageCaption { block_id: 10 };
    runtime.focus_text_surface_at_offset(surface_id, 0).unwrap();
    runtime
        .replace_text_in_focused_range(Some(0..7), "A👨‍👩‍👧‍👦B")
        .unwrap();
    runtime
        .focus_text_surface_at_offset(surface_id, "A👨‍👩‍👧‍👦".len())
        .unwrap();

    assert!(runtime.delete_backward().unwrap());
    assert_eq!(
        runtime
            .text_surface_snapshot(surface_id)
            .unwrap()
            .plain_text(),
        "AB"
    );
    runtime.focus_text_surface_at_offset(surface_id, 0).unwrap();
    assert!(!runtime.delete_backward().unwrap());
    assert_eq!(runtime.document_block_count(), 2);
}

#[test]
fn auxiliary_inline_formatting_uses_the_same_spans_and_undo_boundary() {
    let mut runtime = runtime_with_auxiliary_surfaces();
    let surface_id = SurfaceId::ImageCaption { block_id: 10 };
    runtime.focus_text_surface_at_offset(surface_id, 0).unwrap();
    runtime
        .move_focused_text_surface_to_offset(
            surface_id,
            7,
            cditor_core::edit::TextAffinity::Downstream,
            true,
        )
        .unwrap();

    assert!(
        runtime
            .toggle_inline_mark_on_selection(InlineMark::Bold)
            .unwrap()
    );
    let snapshot = runtime.text_surface_snapshot(surface_id).unwrap();
    assert_eq!(snapshot.marks_at(2), &[InlineMark::Bold]);
    assert_eq!(runtime.input_session_selected_range(), Some(0..7));
    assert!(runtime.undo_focused_block().unwrap());
    assert!(
        runtime
            .text_surface_snapshot(surface_id)
            .unwrap()
            .marks_at(2)
            .is_empty()
    );
}

#[test]
fn text_surface_handle_returns_versioned_edit_result() {
    let mut runtime = runtime_with_auxiliary_surfaces();
    let surface_id = SurfaceId::CollectionTitle { block_id: 20 };
    runtime.focus_text_surface_at_offset(surface_id, 0).unwrap();
    let before = runtime.text_surface_snapshot(surface_id).unwrap().identity;

    let result = runtime
        .text_surface(surface_id)
        .unwrap()
        .replace(0..0, RichTextDelta::plain("Projects"))
        .unwrap();

    assert_eq!(result.identity_before, before);
    assert_eq!(result.replaced_range, 0..0);
    assert_eq!(result.inserted_range, 0..8);
    assert!(result.identity_after.content_version > before.content_version);
    assert_eq!(
        runtime
            .text_surface_snapshot(surface_id)
            .unwrap()
            .plain_text(),
        "Projects"
    );
}

#[test]
fn unicode_word_navigation_uses_the_same_contract_for_block_table_and_auxiliary_surfaces() {
    let mut table = sample_table_payload();
    table.block_id = 30;
    if let BlockPayload::Table(table) = &mut table.payload {
        table.rows[0].cells[0].spans =
            vec![cditor_core::rich_text::InlineSpan::plain("cell words")];
    }
    let mut payloads = vec![
        BlockPayloadRecord::rich_text(1, RichBlockKind::Paragraph, "body words"),
        table,
    ];
    payloads.extend(
        runtime_with_auxiliary_surfaces()
            .document
            .payload_window
            .payloads
            .into_values()
            .map(Arc::unwrap_or_clone),
    );
    let mut runtime = DocumentRuntime::from_payloads(1, payloads, 720.0);

    runtime.focus_block_at_offset(1, 0).unwrap();
    assert!(runtime.move_focused_caret_by_word(true, false).unwrap());
    assert_eq!(runtime.caret_offset_for_block(1), Some(4));

    runtime.focus_table_cell_at_offset(30, 0, 0, 0).unwrap();
    assert!(runtime.move_focused_caret_by_word(true, false).unwrap());
    assert_eq!(runtime.focused_table_cell_offset(), Some((30, 0, 0, 4)));

    let caption = SurfaceId::ImageCaption { block_id: 10 };
    runtime.focus_text_surface_at_offset(caption, 0).unwrap();
    assert!(runtime.move_focused_caret_by_word(true, true).unwrap());
    assert_eq!(runtime.text_surface_selection_range(caption), Some(0..7));
}

#[test]
fn text_surface_rich_delta_preserves_inserted_marks() {
    let mut runtime = runtime_with_auxiliary_surfaces();
    let surface_id = SurfaceId::ImageCaption { block_id: 10 };
    runtime.focus_text_surface_at_offset(surface_id, 0).unwrap();

    runtime
        .text_surface(surface_id)
        .unwrap()
        .replace(
            0..7,
            RichTextDelta {
                spans: vec![cditor_core::rich_text::InlineSpan {
                    text: "Caption".to_owned(),
                    marks: vec![InlineMark::Bold],
                }],
            },
        )
        .unwrap();

    let snapshot = runtime.text_surface_snapshot(surface_id).unwrap();
    assert_eq!(snapshot.plain_text(), "Caption");
    assert_eq!(snapshot.marks_at(2), &[InlineMark::Bold]);
    assert_eq!(runtime.input_session_selected_range(), Some(7..7));
}

#[test]
fn auxiliary_select_all_copy_cut_and_rich_clipboard_share_session_selection() {
    let mut runtime = runtime_with_auxiliary_surfaces();
    let surface_id = SurfaceId::ImageCaption { block_id: 10 };
    runtime.focus_text_surface_at_offset(surface_id, 3).unwrap();

    assert!(runtime.select_all_command());
    assert_eq!(runtime.input_session_selected_range(), Some(0..7));
    assert_eq!(runtime.selected_focused_text().as_deref(), Some("caption"));
    let selection = runtime.clipboard_selection_snapshot().unwrap();
    let cditor_core::clipboard::ClipboardSelection::Inline { spans } = selection else {
        panic!("caption selection must use inline clipboard spans");
    };
    assert_eq!(
        cditor_core::rich_text::plain_text_from_spans(&spans),
        "caption"
    );
    assert!(runtime.has_active_selection());
    assert!(runtime.delete_active_selection().unwrap());
    assert!(
        runtime
            .text_surface_snapshot(surface_id)
            .unwrap()
            .is_empty()
    );
    assert_eq!(runtime.document_block_count(), 2);
}

#[test]
fn collection_title_flattens_structural_clipboard_without_inserting_blocks() {
    let mut runtime = runtime_with_auxiliary_surfaces();
    let surface_id = SurfaceId::CollectionTitle { block_id: 20 };
    runtime.focus_text_surface_at_offset(surface_id, 0).unwrap();
    let selection = cditor_core::clipboard::ClipboardSelection::Blocks {
        blocks: vec![
            cditor_core::clipboard::ClipboardBlock {
                source_id: 100,
                parent_source_id: None,
                depth: 0,
                kind: RichBlockKind::Paragraph,
                payload: BlockPayload::RichText {
                    spans: vec![cditor_core::rich_text::InlineSpan::plain("one")],
                },
            },
            cditor_core::clipboard::ClipboardBlock {
                source_id: 101,
                parent_source_id: None,
                depth: 0,
                kind: RichBlockKind::Paragraph,
                payload: BlockPayload::RichText {
                    spans: vec![cditor_core::rich_text::InlineSpan::plain("two")],
                },
            },
        ],
    };

    assert!(runtime.paste_clipboard_selection(&selection).unwrap());
    assert_eq!(runtime.document_block_count(), 2);
    assert_eq!(
        runtime
            .text_surface_snapshot(surface_id)
            .unwrap()
            .plain_text(),
        "one two"
    );
}
