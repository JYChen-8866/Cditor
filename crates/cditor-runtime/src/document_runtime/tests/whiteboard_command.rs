use cditor_editor_protocol::command::{
    CommandEnvelope, CommandOutcomeStatus, CommandSource, EditorCommand,
};

use super::*;

fn whiteboard_runtime() -> DocumentRuntime {
    runtime_with_single_payload(
        RichBlockKind::Whiteboard,
        BlockPayload::Whiteboard(cditor_core::rich_text::WhiteboardPayload {
            scene_json: "{}".to_owned(),
        }),
    )
}

fn update_scene(
    runtime: &mut DocumentRuntime,
    block_id: BlockId,
    scene_json: &str,
) -> Result<cditor_editor_protocol::command::CommandOutcome, cditor_editor_protocol::ProtocolError>
{
    runtime.dispatch(CommandEnvelope::new(
        EditorCommand::UpdateWhiteboardScene {
            block_id,
            scene_json: scene_json.to_owned(),
        },
        CommandSource::Toolbar,
    ))
}

#[test]
fn whiteboard_scene_dispatch_is_one_undoable_document_command() {
    let mut runtime = whiteboard_runtime();
    let revision_before = runtime.revision();
    let outcome = update_scene(&mut runtime, 1, r#"{"elements":[{"id":"a"}]}"#).unwrap();

    assert_eq!(outcome.status, CommandOutcomeStatus::Applied);
    assert_eq!(outcome.affected_blocks, vec![1]);
    assert_eq!(outcome.transaction_ids.len(), 1);
    assert_eq!(runtime.revision(), revision_before + 1);
    let BlockPayload::Whiteboard(payload) = runtime.block_payload_record(1).unwrap().payload else {
        panic!("expected whiteboard payload");
    };
    assert_eq!(payload.scene_json, r#"{"elements":[{"id":"a"}]}"#);

    assert!(runtime.undo_focused_block().unwrap());
    let BlockPayload::Whiteboard(payload) = runtime.block_payload_record(1).unwrap().payload else {
        panic!("expected whiteboard payload");
    };
    assert_eq!(payload.scene_json, "{}");
}

#[test]
fn whiteboard_scene_dispatch_reports_no_op_and_rejects_wrong_targets() {
    let mut runtime = whiteboard_runtime();
    let revision_before = runtime.revision();
    let no_op = update_scene(&mut runtime, 1, "{}").unwrap();
    assert_eq!(no_op.status, CommandOutcomeStatus::NoOp);
    assert_eq!(runtime.revision(), revision_before);

    let mut paragraph = runtime_with_single_payload(
        RichBlockKind::Paragraph,
        BlockPayload::RichText {
            spans: vec![InlineSpan::plain("text")],
        },
    );
    let paragraph_revision = paragraph.revision();
    let error = update_scene(&mut paragraph, 1, "{}").unwrap_err();
    assert_eq!(
        error.code,
        cditor_editor_protocol::ProtocolErrorCode::ApplyFailed
    );
    assert_eq!(paragraph.revision(), paragraph_revision);
}
