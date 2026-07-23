use cditor_editor_protocol::command::{
    CommandEnvelope, CommandOutcome, CommandOutcomeStatus, CommandSource, EditorCommand,
};

use super::*;

fn dispatch(runtime: &mut DocumentRuntime, command: EditorCommand) -> CommandOutcome {
    runtime
        .dispatch(CommandEnvelope::new(command, CommandSource::Sdk))
        .unwrap()
}

#[test]
fn merge_split_and_align_execute_through_document_dispatch() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);
    let merged = dispatch(
        &mut runtime,
        EditorCommand::TableMergeCells {
            block_id: 10,
            range: TableRange::normalized(0, 0, 1, 1),
        },
    );
    assert_eq!(merged.status, CommandOutcomeStatus::Applied);
    assert_eq!(merged.affected_blocks, vec![10]);
    assert_eq!(merged.transaction_ids.len(), 1);
    let BlockPayload::Table(table) = runtime.block_payload_record(10).unwrap().payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.visible_cells().count(), 1);

    let split = dispatch(
        &mut runtime,
        EditorCommand::TableSplitCell {
            block_id: 10,
            row: 1,
            col: 1,
        },
    );
    assert_eq!(split.status, CommandOutcomeStatus::Applied);
    let BlockPayload::Table(table) = runtime.block_payload_record(10).unwrap().payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.visible_cells().count(), 4);

    let aligned = dispatch(
        &mut runtime,
        EditorCommand::TableSetRangeAlign {
            block_id: 10,
            range: TableRange::normalized(0, 0, 1, 0),
            align: TableCellAlign::Center,
        },
    );
    assert_eq!(aligned.status, CommandOutcomeStatus::Applied);
    let BlockPayload::Table(table) = runtime.block_payload_record(10).unwrap().payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.rows[0].cells[0].align, TableCellAlign::Center);
    assert_eq!(table.rows[1].cells[0].align, TableCellAlign::Center);
    assert_eq!(table.rows[0].cells[1].align, TableCellAlign::Left);
}

#[test]
fn table_cell_commands_reject_invalid_targets_without_revision_changes() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);
    let revision = runtime.revision();
    let error = runtime
        .dispatch(CommandEnvelope::new(
            EditorCommand::TableMergeCells {
                block_id: 10,
                range: TableRange::normalized(0, 0, 5, 5),
            },
            CommandSource::Sdk,
        ))
        .unwrap_err();

    assert_eq!(
        error.code,
        cditor_editor_protocol::ProtocolErrorCode::ApplyFailed
    );
    assert_eq!(runtime.revision(), revision);
    assert!(runtime.drain_pending_structure_transactions().is_empty());
}
