use cditor_api::command::{BlockTransform, CditorCommand};
use cditor_api::document::{
    Affinity, DocumentPosition, DocumentSelection, SaveStatus, ScrollAlignment, TextOffset,
};
use cditor_api::{Cditor, CditorError};
use cditor_app::CditorStorageExt;
use cditor_app::wiring::build_component;
use cditor_core::rich_text::RichBlockKind;
use cditor_storage::StorageBackendKind;
use gpui::TestAppContext;
use tempfile::TempDir;

#[gpui::test]
fn component_exposes_ready_state_and_readonly_control(cx: &mut TestAppContext) {
    let component = cx.update(|cx| build_component(Cditor::new().memory(), cx).unwrap());
    let handle = component.handle.clone();

    assert!(cx.read(|cx| handle.is_ready(cx)));
    assert!(!cx.read(|cx| handle.is_readonly(cx)));
    assert_eq!(
        cx.read(|cx| handle.document_info(cx).unwrap().block_count),
        1
    );

    cx.update(|cx| handle.set_readonly(true, cx).unwrap());
    assert!(cx.read(|cx| handle.is_readonly(cx)));
    assert_eq!(cx.read(|cx| handle.save_status(cx)), SaveStatus::Readonly);
    assert_eq!(cx.update(|cx| handle.undo(cx)), Err(CditorError::Readonly));

    cx.update(|cx| handle.set_readonly(false, cx).unwrap());
    assert_eq!(cx.read(|cx| handle.save_status(cx)), SaveStatus::Clean);
    assert_eq!(
        cx.read(|cx| handle.diagnostics(cx).unwrap().document_blocks),
        1
    );
}

#[gpui::test]
fn newer_schema_readonly_cannot_be_disabled_by_the_host(cx: &mut TestAppContext) {
    let component = cx.update(|cx| build_component(Cditor::new().memory(), cx).unwrap());
    let handle = component.handle.clone();
    cx.update(|cx| {
        component.view.update(cx, |view, _cx| {
            view.enforce_newer_schema_readonly(3, 1);
        });
    });

    assert!(cx.read(|cx| handle.is_readonly(cx)));
    assert_eq!(cx.read(|cx| handle.save_status(cx)), SaveStatus::Readonly);
    cx.update(|cx| handle.set_readonly(false, cx).unwrap());
    assert!(cx.read(|cx| handle.is_readonly(cx)));
    assert_eq!(
        cx.update(|cx| handle.execute(CditorCommand::DeleteForward, cx)),
        Err(CditorError::Readonly)
    );
}

#[gpui::test]
fn handle_reports_loading_and_component_drop(cx: &mut TestAppContext) {
    let component = cx.update(|cx| {
        build_component(
            Cditor::new().with_cloud_endpoint("https://example.invalid"),
            cx,
        )
        .unwrap()
    });
    let handle = component.handle.clone();

    assert!(!cx.read(|cx| handle.is_ready(cx)));
    assert_eq!(cx.update(|cx| handle.undo(cx)), Err(CditorError::NotReady));

    drop(component);
    cx.run_until_parked();
    assert!(!cx.read(|cx| handle.is_ready(cx)));
    assert_eq!(
        cx.update(|cx| handle.set_readonly(true, cx)),
        Err(CditorError::ComponentDropped)
    );
    assert_eq!(
        cx.read(|cx| handle.diagnostics(cx)),
        Err(CditorError::ComponentDropped)
    );
}

#[gpui::test]
fn build_rejects_invalid_postgres_configuration(cx: &mut TestAppContext) {
    let result = cx.update(|cx| {
        build_component(
            Cditor::new().with_postgres_url("postgres://localhost/cditor"),
            cx,
        )
    });

    assert!(matches!(result, Err(CditorError::InvalidInput(_))));
}

#[gpui::test]
fn selection_command_and_virtual_scroll_share_runtime_truth(cx: &mut TestAppContext) {
    let component = cx.update(|cx| build_component(Cditor::new().demo(), cx).unwrap());
    let handle = component.handle;
    let selection = DocumentSelection {
        anchor: DocumentPosition {
            block_id: 1,
            offset: TextOffset::Utf8Bytes(0),
            affinity: Affinity::Downstream,
        },
        head: DocumentPosition {
            block_id: 1,
            offset: TextOffset::Utf8Bytes(6),
            affinity: Affinity::Downstream,
        },
    };

    cx.update(|cx| handle.set_selection(selection, cx).unwrap());
    assert_eq!(
        cx.read(|cx| handle.selected_text(cx)),
        Some("Cditor".to_owned())
    );
    assert!(cx.read(|cx| handle.command_state(&CditorCommand::ToggleBold, cx).enabled));

    let outcome = cx.update(|cx| handle.execute(CditorCommand::ToggleBold, cx).unwrap());
    assert!(outcome.changed());
    assert!(cx.read(|cx| handle.is_dirty(cx)));
    assert!(cx.read(|cx| handle.can_undo(cx)));
    cx.update(|cx| {
        handle
            .scroll_to_block(4, ScrollAlignment::Center, cx)
            .unwrap()
    });
}

#[gpui::test]
fn sqlite_autosaves_and_reopens_through_same_contract(cx: &mut TestAppContext) {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("sdk.cditor.db");
    let component = cx.update(|cx| {
        build_component(
            Cditor::new()
                .with_document_id(1)
                .with_sqlite_path(&path)
                .with_autosave(1),
            cx,
        )
        .unwrap()
    });
    let handle = component.handle.clone();
    cx.run_until_parked();
    assert!(cx.read(|cx| handle.is_ready(cx)));
    assert_eq!(
        cx.read(|cx| handle.diagnostics(cx).unwrap().storage_backend),
        Some(StorageBackendKind::Sqlite)
    );

    let caret = DocumentSelection::caret(DocumentPosition {
        block_id: 1,
        offset: TextOffset::Utf8Bytes(0),
        affinity: Affinity::Downstream,
    });
    cx.update(|cx| handle.set_selection(caret, cx).unwrap());
    let heading =
        CditorCommand::TransformBlock(BlockTransform::Kind(RichBlockKind::Heading { level: 2 }));
    assert!(cx.update(|cx| handle.execute(heading.clone(), cx).unwrap().changed()));

    cx.executor()
        .advance_clock(std::time::Duration::from_secs(2));
    cx.run_until_parked();
    assert_eq!(cx.read(|cx| handle.save_status(cx)), SaveStatus::Clean);
    drop(component);
    cx.run_until_parked();

    let reopened = cx.update(|cx| {
        build_component(
            Cditor::new().with_document_id(1).with_sqlite_path(&path),
            cx,
        )
        .unwrap()
    });
    let reopened_handle = reopened.handle;
    cx.run_until_parked();
    assert!(cx.read(|cx| reopened_handle.is_ready(cx)));
    cx.update(|cx| reopened_handle.set_selection(caret, cx).unwrap());
    assert!(!cx.read(|cx| reopened_handle.command_state(&heading, cx).enabled));
}

#[gpui::test]
fn flush_waits_for_sqlite_commit_and_checkpoint(cx: &mut TestAppContext) {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("explicit-flush.cditor.db");
    let component = cx.update(|cx| {
        build_component(
            Cditor::new()
                .with_document_id(8)
                .with_sqlite_path(&path)
                .without_autosave(),
            cx,
        )
        .unwrap()
    });
    let handle = component.handle.clone();
    cx.run_until_parked();

    let caret = DocumentSelection::caret(DocumentPosition {
        block_id: 1,
        offset: TextOffset::Utf8Bytes(0),
        affinity: Affinity::Downstream,
    });
    cx.update(|cx| handle.set_selection(caret, cx).unwrap());
    let heading =
        CditorCommand::TransformBlock(BlockTransform::Kind(RichBlockKind::Heading { level: 3 }));
    assert!(cx.update(|cx| handle.execute(heading.clone(), cx).unwrap().changed()));

    let task = cx.update(|cx| handle.flush(cx));
    let report = cx.foreground_executor().block_test(task).unwrap();
    assert!(report.revision > 0);
    assert_eq!(report.saved_blocks, 1);
    assert_eq!(cx.read(|cx| handle.save_status(cx)), SaveStatus::Clean);
    assert!(cx.read(|cx| handle.close_guard(cx).can_close_safely));
}

#[gpui::test]
fn save_rejects_non_persistent_backends(cx: &mut TestAppContext) {
    let component = cx.update(|cx| build_component(Cditor::new().memory(), cx).unwrap());
    let task = cx.update(|cx| component.handle.save(cx));
    assert!(matches!(
        cx.foreground_executor().block_test(task),
        Err(CditorError::Unsupported(_))
    ));
}
