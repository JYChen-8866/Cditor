use cditor_editor_protocol::command::{
    CommandEnvelope, CommandId, CommandOutcome, CommandSource, EditorCommand,
    TableCellNavigationDirection, builtin,
};
use cditor_editor_protocol::query::{CommandQuery, QueryResult};

use super::*;

fn dispatch(runtime: &mut DocumentRuntime, command: EditorCommand) -> CommandOutcome {
    runtime
        .dispatch(CommandEnvelope::new(command, CommandSource::Keyboard))
        .expect("keyboard command must dispatch")
}

#[test]
fn text_input_commands_dispatch_with_revision_and_transaction_metadata() {
    let mut runtime =
        runtime_with_kind_depths_and_text(vec![(RichBlockKind::Paragraph, 0, None, "ab")]);
    runtime.focus_block_at_offset(1, 1).unwrap();

    for (command, expected) in [
        (EditorCommand::InsertSoftLineBreak, "a\nb"),
        (EditorCommand::DeleteBackward, "ab"),
        (EditorCommand::DeleteForward, "a"),
    ] {
        let before_revision = runtime.revision();
        let outcome = dispatch(&mut runtime, command);
        assert!(outcome.changed());
        assert_eq!(runtime.revision(), before_revision + 1);
        assert_eq!(outcome.transaction_ids.len(), 1);
        assert_eq!(
            runtime.block_payload_record(1).unwrap().plain_text(),
            expected
        );
    }
}

#[test]
fn block_range_selection_dispatch_reports_selection_without_document_mutation() {
    let mut runtime = DocumentRuntime::demo();
    let projection = runtime.projection_for_window();
    let anchor_block_id = projection.blocks[0].block_id;
    let focus_block_id = projection.blocks[2].block_id;
    let before_revision = runtime.revision();

    let outcome = dispatch(
        &mut runtime,
        EditorCommand::SetBlockSelectionRange {
            anchor_block_id,
            focus_block_id,
        },
    );

    assert!(outcome.selection_changed);
    assert!(outcome.transaction_ids.is_empty());
    assert_eq!(
        outcome.affected_blocks,
        vec![anchor_block_id, focus_block_id]
    );
    assert_eq!(runtime.revision(), before_revision);
    assert_eq!(
        runtime.unified_document_selection_snapshot(),
        Some(cditor_core::edit::UnifiedDocumentSelection {
            anchor: cditor_core::edit::SelectionEndpoint::Block {
                block_id: anchor_block_id,
            },
            focus: cditor_core::edit::SelectionEndpoint::Block {
                block_id: focus_block_id,
            },
        })
    );
}

#[test]
fn block_input_commands_dispatch_and_report_affected_blocks() {
    let mut runtime =
        runtime_with_kind_depths_and_text(vec![(RichBlockKind::Paragraph, 0, None, "ab")]);
    runtime.focus_block_at_offset(1, 1).unwrap();

    let enter = dispatch(&mut runtime, EditorCommand::HandleEnter);
    assert!(enter.changed());
    assert_eq!(runtime.document_block_count(), 2);
    assert_eq!(runtime.block_payload_record(1).unwrap().plain_text(), "a");
    assert_eq!(runtime.block_payload_record(2).unwrap().plain_text(), "b");

    let insert = dispatch(&mut runtime, EditorCommand::InsertParagraphAfterFocused);
    assert!(insert.changed());
    assert_eq!(insert.affected_blocks, vec![2, 3]);
    assert_eq!(runtime.document_block_count(), 3);
}

#[test]
fn down_placer_dispatch_creates_once_then_only_focuses_the_trailing_paragraph() {
    let mut runtime =
        runtime_with_kind_depths_and_text(vec![(RichBlockKind::Paragraph, 0, None, "body")]);
    runtime.focus_block_at_offset(1, 2).unwrap();
    let before_revision = runtime.revision();

    let created = dispatch(&mut runtime, EditorCommand::EnsureTrailingParagraph);

    assert!(created.changed());
    assert_eq!(created.affected_blocks, vec![1, 2]);
    assert_eq!(created.transaction_ids.len(), 1);
    assert_eq!(runtime.revision(), before_revision + 1);
    assert_eq!(runtime.document_block_count(), 2);
    assert_eq!(runtime.focused_block_id(), Some(2));

    runtime.focus_block_at_offset(1, 0).unwrap();
    let after_create_revision = runtime.revision();
    let focused = dispatch(&mut runtime, EditorCommand::EnsureTrailingParagraph);

    assert!(focused.changed());
    assert!(focused.selection_changed);
    assert!(focused.transaction_ids.is_empty());
    assert_eq!(runtime.revision(), after_create_revision);
    assert_eq!(runtime.document_block_count(), 2);
    assert_eq!(runtime.focused_block_id(), Some(2));
}

#[test]
fn stale_down_placer_dispatch_has_zero_document_or_selection_mutation() {
    let mut runtime =
        runtime_with_kind_depths_and_text(vec![(RichBlockKind::Paragraph, 0, None, "body")]);
    runtime.focus_block_at_offset(1, 2).unwrap();
    let before_revision = runtime.revision();
    let before_selection = runtime.document_selection_snapshot();

    let error = runtime
        .dispatch(
            CommandEnvelope::new(
                EditorCommand::EnsureTrailingParagraph,
                CommandSource::Toolbar,
            )
            .expecting_revision(before_revision + 1),
        )
        .unwrap_err();

    assert_eq!(
        error.code,
        cditor_editor_protocol::ProtocolErrorCode::StalePrecondition
    );
    assert_eq!(runtime.revision(), before_revision);
    assert_eq!(runtime.document_selection_snapshot(), before_selection);
    assert_eq!(runtime.document_block_count(), 1);
}

#[test]
fn document_selection_dispatch_preserves_affinity_without_changing_revision() {
    let mut runtime = runtime_with_kind_depths_and_text(vec![
        (RichBlockKind::Paragraph, 0, None, "alpha"),
        (RichBlockKind::Paragraph, 0, None, "beta"),
    ]);
    let before_revision = runtime.revision();
    let selection = DocumentSelection {
        anchor: TextPosition {
            block_id: 2,
            offset: 3,
            affinity: TextAffinity::Upstream,
        },
        focus: TextPosition::downstream(1, 1),
    };

    let outcome = dispatch(
        &mut runtime,
        EditorCommand::SetDocumentSelection { selection },
    );

    assert!(outcome.selection_changed);
    assert!(outcome.transaction_ids.is_empty());
    assert_eq!(runtime.revision(), before_revision);
    assert_eq!(runtime.document_selection_snapshot(), Some(selection));
    assert_eq!(runtime.focused_block_id(), Some(1));

    let before_selection = runtime.document_selection_snapshot();
    let error = runtime
        .dispatch(
            CommandEnvelope::new(
                EditorCommand::SetDocumentSelection {
                    selection: DocumentSelection {
                        anchor: TextPosition::downstream(1, 0),
                        focus: TextPosition::downstream(2, 4),
                    },
                },
                CommandSource::Sdk,
            )
            .expecting_revision(before_revision + 1),
        )
        .unwrap_err();
    assert_eq!(
        error.code,
        cditor_editor_protocol::ProtocolErrorCode::StalePrecondition
    );
    assert_eq!(runtime.document_selection_snapshot(), before_selection);
    assert_eq!(runtime.revision(), before_revision);
}

#[test]
fn block_focus_dispatch_changes_session_without_changing_document_revision() {
    let mut runtime = runtime_with_kind_depths(vec![
        (RichBlockKind::Paragraph, 0, None),
        (RichBlockKind::Image, 0, None),
    ]);
    runtime.focus_block(1);
    let before_revision = runtime.revision();

    let outcome = dispatch(&mut runtime, EditorCommand::FocusBlock { block_id: 2 });

    assert!(outcome.changed());
    assert_eq!(outcome.affected_blocks, vec![2]);
    assert!(outcome.transaction_ids.is_empty());
    assert_eq!(runtime.focused_block_id(), Some(2));
    assert_eq!(runtime.revision(), before_revision);

    let error = runtime
        .dispatch(
            CommandEnvelope::new(
                EditorCommand::FocusBlock { block_id: 1 },
                CommandSource::Toolbar,
            )
            .expecting_revision(before_revision + 1),
        )
        .unwrap_err();
    assert_eq!(
        error.code,
        cditor_editor_protocol::ProtocolErrorCode::StalePrecondition
    );
    assert_eq!(runtime.focused_block_id(), Some(2));
    assert_eq!(runtime.revision(), before_revision);
}

#[test]
fn table_cell_focus_and_blur_dispatch_only_change_session_state() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);
    let before_revision = runtime.revision();

    let focused = dispatch(
        &mut runtime,
        EditorCommand::FocusTableCell {
            block_id: 10,
            row: 0,
            col: 1,
            offset: Some(0),
            affinity: TextAffinity::Upstream,
        },
    );

    assert!(focused.changed());
    assert_eq!(focused.affected_blocks, vec![10]);
    assert!(focused.transaction_ids.is_empty());
    assert_eq!(
        runtime.focused_table_cell_text_position(),
        Some((10, 0, 1, 0, TextAffinity::Upstream))
    );
    assert_eq!(runtime.revision(), before_revision);

    let blurred = dispatch(&mut runtime, EditorCommand::BlurTableCell);
    assert!(blurred.changed());
    assert_eq!(runtime.focused_table_cell_offset(), None);
    assert_eq!(runtime.focused_block_id(), Some(10));
    assert_eq!(runtime.revision(), before_revision);

    let error = runtime
        .dispatch(
            CommandEnvelope::new(
                EditorCommand::FocusTableCell {
                    block_id: 10,
                    row: 0,
                    col: 0,
                    offset: None,
                    affinity: TextAffinity::Downstream,
                },
                CommandSource::Toolbar,
            )
            .expecting_revision(before_revision + 1),
        )
        .unwrap_err();
    assert_eq!(
        error.code,
        cditor_editor_protocol::ProtocolErrorCode::StalePrecondition
    );
    assert_eq!(runtime.focused_table_cell_offset(), None);
    assert_eq!(runtime.revision(), before_revision);
}

#[test]
fn table_cell_selection_dispatch_preserves_direction_without_document_change() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);
    runtime.focus_table_cell_at_offset(10, 0, 1, 1).unwrap();
    let before_revision = runtime.revision();

    let outcome = dispatch(
        &mut runtime,
        EditorCommand::SetTableCellSelection {
            block_id: 10,
            row: 0,
            col: 1,
            anchor_offset: 1,
            focus_offset: 0,
            focus_affinity: TextAffinity::Upstream,
        },
    );

    assert!(outcome.changed());
    assert_eq!(outcome.affected_blocks, vec![10]);
    assert!(outcome.transaction_ids.is_empty());
    assert_eq!(runtime.revision(), before_revision);
    assert_eq!(
        runtime.focused_table_cell_selection_state(),
        Some((10, 0, 1, 0..1, true, None))
    );
    assert_eq!(
        runtime.focused_table_cell_text_position(),
        Some((10, 0, 1, 0, TextAffinity::Upstream))
    );

    let before_selection = runtime.focused_table_cell_selection_state();
    let error = runtime
        .dispatch(
            CommandEnvelope::new(
                EditorCommand::SetTableCellSelection {
                    block_id: 10,
                    row: 0,
                    col: 1,
                    anchor_offset: 0,
                    focus_offset: 1,
                    focus_affinity: TextAffinity::Downstream,
                },
                CommandSource::Keyboard,
            )
            .expecting_revision(before_revision + 1),
        )
        .unwrap_err();
    assert_eq!(
        error.code,
        cditor_editor_protocol::ProtocolErrorCode::StalePrecondition
    );
    assert_eq!(
        runtime.focused_table_cell_selection_state(),
        before_selection
    );
    assert_eq!(runtime.revision(), before_revision);
}

#[test]
fn table_cell_navigation_dispatch_preserves_unicode_tabs_and_stale_safety() {
    let mut payload = sample_table_payload();
    let BlockPayload::Table(table) = &mut payload.payload else {
        unreachable!();
    };
    table.rows[0].cells[1] = cditor_core::rich_text::TableCellPayload::plain("中文");
    let mut runtime = DocumentRuntime::from_payloads(1, vec![payload], 720.0);
    let before_revision = runtime.revision();

    runtime.focus_table_cell_at_offset(10, 0, 1, 0).unwrap();
    let extended = dispatch(
        &mut runtime,
        EditorCommand::NavigateTableCell {
            direction: TableCellNavigationDirection::Right,
            extend_selection: true,
        },
    );
    assert!(extended.changed());
    assert_eq!(extended.affected_blocks, vec![10]);
    assert!(extended.transaction_ids.is_empty());
    assert_eq!(runtime.revision(), before_revision);
    assert_eq!(
        runtime.focused_table_cell_selection_state(),
        Some((10, 0, 1, 0..3, false, None))
    );

    runtime.focus_table_cell_at_offset(10, 0, 0, 1).unwrap();
    assert!(
        dispatch(
            &mut runtime,
            EditorCommand::NavigateTableCell {
                direction: TableCellNavigationDirection::Right,
                extend_selection: false,
            },
        )
        .changed()
    );
    assert_eq!(runtime.focused_table_cell_offset(), Some((10, 0, 1, 0)));

    assert!(
        dispatch(
            &mut runtime,
            EditorCommand::NavigateTableCell {
                direction: TableCellNavigationDirection::TabForward,
                extend_selection: false,
            },
        )
        .changed()
    );
    assert_eq!(runtime.focused_table_cell_offset(), Some((10, 1, 0, 0)));
    assert!(
        dispatch(
            &mut runtime,
            EditorCommand::NavigateTableCell {
                direction: TableCellNavigationDirection::TabBackward,
                extend_selection: false,
            },
        )
        .changed()
    );
    assert_eq!(runtime.focused_table_cell_offset(), Some((10, 0, 1, 6)));

    let before_stale = runtime.focused_table_cell_selection_state();
    let error = runtime
        .dispatch(
            CommandEnvelope::new(
                EditorCommand::NavigateTableCell {
                    direction: TableCellNavigationDirection::Down,
                    extend_selection: false,
                },
                CommandSource::Keyboard,
            )
            .expecting_revision(before_revision + 1),
        )
        .unwrap_err();
    assert_eq!(
        error.code,
        cditor_editor_protocol::ProtocolErrorCode::StalePrecondition
    );
    assert_eq!(runtime.focused_table_cell_selection_state(), before_stale);
    assert_eq!(runtime.revision(), before_revision);
}

#[test]
fn caret_navigation_dispatch_owns_semantic_fallbacks_without_document_changes() {
    let mut runtime = runtime_with_kind_depths_and_text(vec![
        (RichBlockKind::Paragraph, 0, None, "ab\ncd"),
        (RichBlockKind::Paragraph, 0, None, "xy"),
    ]);
    let before_revision = runtime.revision();

    runtime.focus_block_at_offset(1, 5).unwrap();
    let line_start = dispatch(
        &mut runtime,
        EditorCommand::MoveCaret {
            direction: cditor_editor_protocol::command::CaretDirection::LineStart,
            extend_selection: false,
        },
    );
    assert!(line_start.changed());
    assert_eq!(line_start.affected_blocks, vec![1]);
    assert!(line_start.transaction_ids.is_empty());
    assert_eq!(runtime.caret_offset_for_block(1), Some(3));

    let extended = dispatch(
        &mut runtime,
        EditorCommand::MoveCaret {
            direction: cditor_editor_protocol::command::CaretDirection::PreviousVisual,
            extend_selection: true,
        },
    );
    assert!(extended.changed());
    assert_eq!(
        runtime.document_selection_snapshot(),
        Some(DocumentSelection {
            anchor: TextPosition::downstream(1, 3),
            focus: TextPosition::downstream(1, 2),
        })
    );

    let document_end = dispatch(
        &mut runtime,
        EditorCommand::MoveCaret {
            direction: cditor_editor_protocol::command::CaretDirection::DocumentEnd,
            extend_selection: false,
        },
    );
    assert!(document_end.changed());
    assert_eq!(runtime.focused_block_id(), Some(2));
    assert_eq!(runtime.caret_offset_for_block(2), Some(2));
    assert_eq!(runtime.revision(), before_revision);

    let before_stale = runtime.document_selection_snapshot();
    let error = runtime
        .dispatch(
            CommandEnvelope::new(
                EditorCommand::MoveCaret {
                    direction: cditor_editor_protocol::command::CaretDirection::PreviousLine,
                    extend_selection: false,
                },
                CommandSource::Keyboard,
            )
            .expecting_revision(before_revision + 1),
        )
        .unwrap_err();
    assert_eq!(
        error.code,
        cditor_editor_protocol::ProtocolErrorCode::StalePrecondition
    );
    assert_eq!(runtime.focused_block_id(), Some(2));
    assert_eq!(runtime.caret_offset_for_block(2), Some(2));
    assert_eq!(runtime.document_selection_snapshot(), before_stale);
    assert_eq!(runtime.revision(), before_revision);
}

#[test]
fn structure_input_commands_dispatch_without_false_document_changes() {
    let mut runtime = runtime_with_kind_depths(vec![
        (RichBlockKind::BulletedList, 0, None),
        (RichBlockKind::BulletedList, 0, None),
    ]);
    runtime.focus_block(2);

    let indent = dispatch(&mut runtime, EditorCommand::IndentBlock);
    assert!(indent.changed());
    assert_eq!(runtime.document.index.parent_ids[1], Some(1));
    assert_eq!(runtime.document.index.depths[1], 1);

    let unavailable_indent = dispatch(&mut runtime, EditorCommand::IndentBlock);
    assert!(!unavailable_indent.changed());

    let outdent = dispatch(&mut runtime, EditorCommand::OutdentBlock);
    assert!(outdent.changed());
    assert_eq!(runtime.document.index.parent_ids[1], None);
    assert_eq!(runtime.document.index.depths[1], 0);
}

#[test]
fn runtime_query_matches_keyboard_input_preconditions() {
    let mut runtime = runtime_with_kind_depths(vec![
        (RichBlockKind::BulletedList, 0, None),
        (RichBlockKind::BulletedList, 0, None),
    ]);
    for command_id in [
        builtin::BLOCK_INSERT_AFTER_FOCUSED,
        builtin::TEXT_INSERT_SOFT_BREAK,
        builtin::BLOCK_ENTER,
        builtin::BLOCK_INDENT,
        builtin::BLOCK_OUTDENT,
        builtin::TEXT_DELETE_BACKWARD,
        builtin::TEXT_DELETE_FORWARD,
    ] {
        assert!(matches!(
            runtime.query(CommandQuery::State {
                command_id: CommandId::builtin(command_id),
            }),
            QueryResult::CommandState(state) if !state.enabled
        ));
    }
    assert!(matches!(
        runtime.query(CommandQuery::State {
            command_id: CommandId::builtin(builtin::BLOCK_ENSURE_TRAILING_PARAGRAPH),
        }),
        QueryResult::CommandState(state) if state.enabled
    ));

    runtime.focus_block(2);
    for command_id in [
        builtin::BLOCK_INSERT_AFTER_FOCUSED,
        builtin::BLOCK_ENSURE_TRAILING_PARAGRAPH,
        builtin::TEXT_INSERT_SOFT_BREAK,
        builtin::BLOCK_ENTER,
        builtin::BLOCK_INDENT,
        builtin::TEXT_DELETE_BACKWARD,
        builtin::TEXT_DELETE_FORWARD,
    ] {
        assert!(matches!(
            runtime.query(CommandQuery::State {
                command_id: CommandId::builtin(command_id),
            }),
            QueryResult::CommandState(state) if state.enabled
        ));
    }
    assert!(!runtime.can_outdent_focused_block());
}

#[test]
fn drag_commit_commands_dispatch_once_with_typed_payloads() {
    let image = BlockPayloadRecord {
        block_id: 1,
        content_version: 1,
        kind: RichBlockKind::Image,
        payload: BlockPayload::Image(cditor_core::rich_text::ImagePayload::default()),
    };
    let mut runtime = DocumentRuntime::from_payloads(1, vec![image, sample_table_payload()], 720.0);

    for command in [
        EditorCommand::SetMediaWidthRatio {
            block_id: 1,
            ratio_milli: 750,
        },
        EditorCommand::TableResizeAxis {
            block_id: 10,
            axis: cditor_editor_protocol::command::TableAxis::Column,
            index: 0,
            size_px: 180,
        },
        EditorCommand::TableMoveAxis {
            block_id: 10,
            axis: cditor_editor_protocol::command::TableAxis::Row,
            from_index: 0,
            to_index: 1,
        },
    ] {
        let before_revision = runtime.revision();
        let outcome = dispatch(&mut runtime, command);
        assert!(outcome.changed());
        assert_eq!(runtime.revision(), before_revision + 1);
        assert_eq!(outcome.transaction_ids.len(), 1);
    }

    let BlockPayload::Image(image) = runtime.block_payload_record(1).unwrap().payload else {
        panic!("image payload");
    };
    assert_eq!(image.display_width_ratio_milli, Some(750));
    let BlockPayload::Table(table) = runtime.block_payload_record(10).unwrap().payload else {
        panic!("table payload");
    };
    assert_eq!(table.columns[0].width, TableTrackSize::Px(180));
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("C"));
}

#[test]
fn raw_clipboard_is_rejected_at_runtime_and_typed_image_payload_still_applies() {
    let mut runtime =
        runtime_with_kind_depths_and_text(vec![(RichBlockKind::Paragraph, 0, None, "start ")]);
    runtime.focus_block_at_offset(1, 6).unwrap();

    let error = runtime
        .dispatch(CommandEnvelope::new(
            EditorCommand::ApplyClipboardData {
                text: "a\r\nb".to_owned(),
                metadata_json: Some("{invalid".to_owned()),
            },
            CommandSource::Import,
        ))
        .unwrap_err();
    assert_eq!(
        error.code,
        cditor_editor_protocol::ProtocolErrorCode::InvalidArguments
    );
    assert_eq!(
        runtime.block_payload_record(1).unwrap().plain_text(),
        "start "
    );

    let image = dispatch(
        &mut runtime,
        EditorCommand::InsertImageAsset {
            payload: cditor_core::rich_text::ImagePayload::default(),
            asset: None,
            after_block_id: None,
        },
    );
    assert!(image.changed());
    assert_eq!(image.affected_blocks.len(), 2);
    assert!(matches!(
        runtime.block_kind(image.affected_blocks[0]),
        Some(RichBlockKind::Image)
    ));
    assert_eq!(image.transaction_ids.len(), 1);
}

#[test]
fn imported_image_and_asset_attachment_commit_in_one_transaction() {
    let mut runtime =
        runtime_with_kind_depths_and_text(vec![(RichBlockKind::Paragraph, 0, None, "start")]);
    runtime.focus_block_at_offset(1, 5).unwrap();
    let asset = cditor_core::edit::AssetSnapshot {
        asset_id: 77,
        file_name: "image.png".into(),
        media_type: "image/png".into(),
        size_bytes: 12,
        source: "assets/abc.png".into(),
        checksum: Some("a".repeat(64)),
        state: cditor_core::edit::AssetState::Ready,
    };

    let outcome = dispatch(
        &mut runtime,
        EditorCommand::InsertImageAsset {
            payload: cditor_core::rich_text::ImagePayload {
                source: asset.source.clone(),
                ..Default::default()
            },
            asset: Some(asset.clone()),
            after_block_id: Some(1),
        },
    );

    let image_block = outcome.affected_blocks[0];
    assert_eq!(outcome.transaction_ids.len(), 1);
    assert_eq!(
        runtime.attached_asset_ids(image_block),
        vec![asset.asset_id]
    );
    assert_eq!(runtime.asset_snapshot(asset.asset_id), Some(&asset));
}

#[test]
fn dropped_video_inserts_video_and_trailing_paragraph_in_one_transaction() {
    let mut runtime =
        runtime_with_kind_depths_and_text(vec![(RichBlockKind::Paragraph, 0, None, "start")]);
    runtime.focus_block_at_offset(1, 5).unwrap();
    let asset = cditor_core::edit::AssetSnapshot {
        asset_id: 88,
        file_name: "clip.mp4".into(),
        media_type: "video/mp4".into(),
        size_bytes: 24,
        source: "assets/clip.mp4".into(),
        checksum: Some("b".repeat(64)),
        state: cditor_core::edit::AssetState::Ready,
    };
    let outcome = dispatch(
        &mut runtime,
        EditorCommand::InsertVideoAsset {
            payload: cditor_core::rich_text::VideoPayload {
                source: asset.source.clone(),
                title: asset.file_name.clone(),
                media_type: Some(asset.media_type.clone()),
                intrinsic_width: Some(1920),
                intrinsic_height: Some(1080),
                ..Default::default()
            },
            asset: Some(asset.clone()),
            after_block_id: Some(1),
        },
    );
    let video_block = outcome.affected_blocks[0];
    assert_eq!(outcome.transaction_ids.len(), 1);
    assert_eq!(runtime.block_kind(video_block), Some(RichBlockKind::Video));
    let video_height = runtime
        .projection(cditor_editor_protocol::projection::ProjectionRequest {
            viewport_revision: runtime.revision(),
            include_diagnostics: false,
        })
        .blocks
        .into_iter()
        .find(|block| block.block_id == video_block)
        .expect("inserted video projection")
        .layout
        .effective_height();
    assert_eq!(
        video_height,
        cditor_core::layout::VIDEO_BLOCK_ESTIMATED_HEIGHT_PX
    );
    assert_eq!(
        runtime.attached_asset_ids(video_block),
        vec![asset.asset_id]
    );
    assert_eq!(runtime.asset_snapshot(asset.asset_id), Some(&asset));
    assert_eq!(
        runtime.block_kind(outcome.affected_blocks[1]),
        Some(RichBlockKind::Paragraph)
    );

    let undo = dispatch(&mut runtime, EditorCommand::Undo);
    assert!(undo.changed());
    assert_eq!(runtime.block_kind(video_block), None);
    assert!(runtime.attached_asset_ids(video_block).is_empty());

    let redo = dispatch(&mut runtime, EditorCommand::Redo);
    assert!(redo.changed());
    assert_eq!(runtime.block_kind(video_block), Some(RichBlockKind::Video));
    assert_eq!(
        runtime.attached_asset_ids(video_block),
        vec![asset.asset_id]
    );
}

#[test]
fn dropped_video_with_an_explicit_anchor_does_not_require_text_focus() {
    let runtime =
        runtime_with_kind_depths_and_text(vec![(RichBlockKind::Paragraph, 0, None, "start")]);
    assert_eq!(runtime.focused_block_id(), None);

    let command = |after_block_id| EditorCommand::InsertVideoAsset {
        payload: cditor_core::rich_text::VideoPayload::default(),
        asset: None,
        after_block_id,
    };
    assert!(runtime.query_editor_command(&command(Some(1))).enabled);
    assert!(!runtime.query_editor_command(&command(Some(999))).enabled);
    assert!(runtime.query_editor_command(&command(None)).enabled);
}

#[test]
fn imported_image_uses_the_paste_time_block_anchor_after_focus_moves() {
    let mut runtime = runtime_with_kind_depths_and_text(vec![
        (RichBlockKind::Paragraph, 0, None, "first"),
        (RichBlockKind::Paragraph, 0, None, "second"),
    ]);
    runtime.focus_block_at_offset(2, 6).unwrap();

    let outcome = dispatch(
        &mut runtime,
        EditorCommand::InsertImageAsset {
            payload: cditor_core::rich_text::ImagePayload {
                source: "assets/image.png".into(),
                ..Default::default()
            },
            asset: None,
            after_block_id: Some(1),
        },
    );

    assert_eq!(
        runtime.full_projection_for_tests().blocks[1].block_id,
        outcome.affected_blocks[0]
    );
    assert_eq!(runtime.full_projection_for_tests().blocks[3].block_id, 2);
}

#[test]
fn block_drag_commands_dispatch_one_structure_transaction() {
    let mut runtime = runtime_with_kind_depths(vec![
        (RichBlockKind::BulletedList, 0, None),
        (RichBlockKind::BulletedList, 0, None),
        (RichBlockKind::BulletedList, 0, None),
    ]);

    let before = dispatch(
        &mut runtime,
        EditorCommand::MoveBlockBefore {
            block_id: 1,
            before_block_id: Some(3),
        },
    );
    assert!(before.changed());
    assert_eq!(before.transaction_ids.len(), 1);
    assert_eq!(runtime.full_projection_for_tests().blocks[1].block_id, 1);

    let nested = dispatch(
        &mut runtime,
        EditorCommand::MoveBlockToParent {
            block_id: 3,
            parent_id: Some(1),
            sibling_index: 0,
        },
    );
    assert!(nested.changed());
    assert_eq!(nested.transaction_ids.len(), 1);
    let index = runtime.document.index.index_of(3).unwrap();
    assert_eq!(runtime.document.index.parent_ids[index], Some(1));
}
