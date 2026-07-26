use cditor_core::rich_text::{BlockPayloadRecord, RichBlockKind};
use cditor_editor_protocol::command::{CommandEnvelope, CommandSource, EditorCommand};
use cditor_test_support::fail_first_persistence_fixture;
use gpui::TestAppContext;

use super::*;

#[gpui::test]
fn failed_save_restores_transactions_and_explicit_retry_cleans_document(cx: &mut TestAppContext) {
    let (persistence, storage) = fail_first_persistence_fixture(1, Some(std::time::Duration::ZERO));
    let mut runtime = cditor_runtime::DocumentRuntime::from_payloads(
        1,
        (1..=3)
            .map(|block_id| {
                BlockPayloadRecord::rich_text(
                    block_id,
                    RichBlockKind::Paragraph,
                    block_id.to_string(),
                )
            })
            .collect(),
        720.0,
    );
    assert!(
        runtime
            .dispatch(CommandEnvelope::new(
                EditorCommand::MoveBlockBefore {
                    block_id: 1,
                    before_block_id: Some(3),
                },
                CommandSource::Sdk,
            ))
            .unwrap()
            .changed()
    );
    let (view, cx) = cx.add_window_view(|_window, cx| {
        CditorV2View::from_runtime_with_persistence_options(
            runtime,
            false,
            false,
            Some(persistence),
            cx,
        )
    });

    view.update(cx, |view, cx| {
        view.mark_dirty(cx);
        view.flush_storage_persistence(cx);
    });
    cx.run_until_parked();
    assert!(matches!(
        view.read_with(cx, |view, _| view.sdk_save_status()),
        cditor_sdk::document::SaveStatus::FailedLocal(failure)
            if failure.kind == cditor_sdk::document::SaveFailureKind::Other
                && failure.message.contains("injected")
    ));
    let close_guard = view.read_with(cx, |view, _| view.sdk_close_guard());
    assert!(!close_guard.can_close_safely);
    assert!(close_guard.requires_recovery_export);
    assert!(close_guard.local_failure.is_some());
    assert_eq!(
        view.read_with(cx, |view, _| {
            view.ready_session()
                .unwrap()
                .persistence_runtime_snapshot()
                .unwrap()
                .pending_structure_transactions
        }),
        1
    );

    let retry = view.update(cx, |view, cx| view.sdk_save(cx));
    let report = cx.foreground_executor().block_test(retry).unwrap();
    assert_eq!(report.saved_blocks, 3);
    assert_eq!(
        view.read_with(cx, |view, _| view.sdk_save_status()),
        cditor_sdk::document::SaveStatus::LocallySaved
    );
    assert_eq!(storage.transaction_counts(), vec![1, 1]);
}
