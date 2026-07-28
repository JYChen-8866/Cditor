use cditor_desktop::wiring::build_component;
use cditor_sdk::command::CditorCommand;
use cditor_sdk::document::{
    Affinity, DocumentPosition, DocumentSelection, SaveStatus, ScrollAlignment, TextOffset,
};
use cditor_sdk::{Cditor, CditorError};
use gpui::TestAppContext;

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
    assert_eq!(
        cx.read(|cx| handle.save_status(cx)),
        SaveStatus::LocallySaved
    );
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
fn handle_reports_component_drop(cx: &mut TestAppContext) {
    let component = cx.update(|cx| build_component(Cditor::new().memory(), cx).unwrap());
    let handle = component.handle.clone();

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
fn dirty_document_exports_a_valid_non_mutating_recovery_bundle(cx: &mut TestAppContext) {
    let component =
        cx.update(|cx| build_component(Cditor::new().memory().without_autosave(), cx).unwrap());
    let handle = component.handle;
    let caret = DocumentSelection::caret(DocumentPosition {
        block_id: 1,
        offset: TextOffset::Utf8Bytes(0),
        affinity: Affinity::Downstream,
    });
    cx.update(|cx| handle.set_selection(caret, cx).unwrap());
    assert!(
        cx.update(|cx| handle
            .execute(CditorCommand::InsertParagraphAfterFocused, cx)
            .unwrap())
            .changed()
    );

    let dirty_before = cx.read(|cx| handle.is_dirty(cx));
    let export = cx.read(|cx| handle.export_recovery(cx)).unwrap();
    let plan = cditor_session::decode_emergency_export(&export.bytes).unwrap();

    assert!(dirty_before);
    assert!(cx.read(|cx| handle.is_dirty(cx)));
    assert_eq!(export.transaction_count, plan.transactions.len());
    assert_eq!(export.media_type, "application/vnd.cditor.recovery+json");
    assert!(export.suggested_file_name.ends_with(".json"));
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
