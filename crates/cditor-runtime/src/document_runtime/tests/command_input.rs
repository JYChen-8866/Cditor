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
