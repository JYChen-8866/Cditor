use cditor_editor_protocol::command::{
    CommandEnvelope, CommandId, CommandOutcome, CommandSource, EditorCommand, builtin,
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
fn structure_input_commands_dispatch_without_false_document_changes() {
    let mut runtime = runtime_with_kind_depths(vec![
        (RichBlockKind::BulletedList, 0, None),
        (RichBlockKind::BulletedList, 0, None),
    ]);
    runtime.focus_block(2);

    let indent = dispatch(&mut runtime, EditorCommand::IndentBlock);
    assert!(indent.changed());
    assert_eq!(runtime.index.parent_ids[1], Some(1));
    assert_eq!(runtime.index.depths[1], 1);

    let unavailable_indent = dispatch(&mut runtime, EditorCommand::IndentBlock);
    assert!(!unavailable_indent.changed());

    let outdent = dispatch(&mut runtime, EditorCommand::OutdentBlock);
    assert!(outdent.changed());
    assert_eq!(runtime.index.parent_ids[1], None);
    assert_eq!(runtime.index.depths[1], 0);
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

    runtime.focus_block(2);
    for command_id in [
        builtin::BLOCK_INSERT_AFTER_FOCUSED,
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
fn clipboard_and_image_payload_commands_own_import_mutations() {
    let mut runtime =
        runtime_with_kind_depths_and_text(vec![(RichBlockKind::Paragraph, 0, None, "start ")]);
    runtime.focus_block_at_offset(1, 6).unwrap();

    let paste = dispatch(
        &mut runtime,
        EditorCommand::ApplyClipboardData {
            text: "a\r\nb".to_owned(),
            metadata_json: Some("{invalid".to_owned()),
        },
    );
    assert!(paste.changed());
    assert_eq!(
        runtime.block_payload_record(1).unwrap().plain_text(),
        "start a\nb"
    );
    assert_eq!(paste.transaction_ids.len(), 1);

    let image = dispatch(
        &mut runtime,
        EditorCommand::InsertImageAsset {
            payload: cditor_core::rich_text::ImagePayload::default(),
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
