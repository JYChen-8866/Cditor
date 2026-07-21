use super::*;

#[test]
fn owned_navigation_commits_or_rolls_back_without_cloning_steps() {
    let mut undo = UndoStack::default();
    undo.record_transaction(EditTransaction::insert_text(1, 0, 42, 0, "a"));

    let step = undo.take_undo_step().unwrap();
    undo.rollback_undo_step(step);
    assert_eq!((undo.undo_len(), undo.redo_len()), (1, 0));

    let step = undo.take_undo_step().unwrap();
    undo.commit_undo_step(step);
    assert_eq!((undo.undo_len(), undo.redo_len()), (0, 1));

    let step = undo.take_redo_step().unwrap();
    undo.rollback_redo_step(step);
    let step = undo.take_redo_step().unwrap();
    undo.commit_redo_step(step);
    assert_eq!((undo.undo_len(), undo.redo_len()), (1, 0));
}

#[test]
fn capacity_truncates_oldest_step_and_mutates_newest_metadata() {
    let mut undo = UndoStack::default();
    for id in 1..=3 {
        undo.record_transaction(command_transaction(id));
    }
    undo.truncate_undo_to(2);
    assert_eq!(undo.undo_len(), 2);
    undo.last_undo_transaction_mut().unwrap().after_anchor = Some(ScrollAnchor {
        block_id: 42,
        offset_in_block: 3.0,
        viewport_y: 4.0,
    });
    assert_eq!(undo.last().unwrap().id, 3);
    assert_eq!(undo.last().unwrap().after_anchor.unwrap().viewport_y, 4.0);
}

#[test]
fn external_blobs_are_reclaimed_when_history_becomes_unreachable() {
    let mut undo = UndoStack::new(UndoGroupingPolicy {
        typing_merge_window_ms: 1_000,
        inline_block_snapshot_limit: 1,
    });
    let large = EditTransaction::paste_blocks(
        1,
        1,
        0,
        vec![
            BlockIndexRecord::new(1, None, 0, 1, 0),
            BlockIndexRecord::new(2, None, 0, 1, 0),
        ],
    );
    undo.record_transaction(large.clone());
    let first = reference(1);
    assert!(undo.mark_externalized(first.clone()));
    let step = undo.take_undo_step().unwrap();
    undo.commit_undo_step(step);
    undo.record_transaction(command_transaction(2));
    assert_eq!(undo.drain_orphaned_external_blobs(), vec![first]);

    undo.record_transaction(large);
    let second = reference(2);
    assert!(undo.mark_externalized(second.clone()));
    undo.truncate_undo_to(0);
    assert_eq!(undo.drain_orphaned_external_blobs(), vec![second.clone()]);
    undo.restore_orphaned_external_blobs([second.clone(), second.clone()]);
    assert_eq!(undo.drain_orphaned_external_blobs(), vec![second]);
}

fn command_transaction(id: u64) -> EditTransaction {
    EditTransaction::new(
        id,
        EditTransactionKind::ExplicitCommand,
        id,
        vec![EditOperation::InsertText {
            block_id: 42,
            offset: 0,
            text: id.to_string(),
        }],
        vec![EditOperation::DeleteText {
            block_id: 42,
            range: 0..1,
        }],
    )
}

fn reference(snapshot_id: u64) -> ExternalUndoBlobRef {
    ExternalUndoBlobRef {
        snapshot_id,
        storage_key: format!("undo:{snapshot_id}"),
        checksum: format!("checksum-{snapshot_id}"),
        encoded_len: snapshot_id as usize * 10,
        block_count: 2,
    }
}
