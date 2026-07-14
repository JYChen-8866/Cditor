use cditor_app::api::{
    Affinity, DocumentPosition, DocumentSelection, SaveStatus, ScrollAlignment, TextOffset,
};
use cditor_app::{CditorBuilder, CditorCommand, CditorError};
use gpui::TestAppContext;

#[gpui::test]
fn sdk_component_exposes_ready_state_and_readonly_control(cx: &mut TestAppContext) {
    let component = cx.update(|cx| CditorBuilder::new().memory().build(cx).unwrap());
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
fn sdk_handle_reports_loading_and_component_drop(cx: &mut TestAppContext) {
    let component = cx.update(|cx| {
        CditorBuilder::new()
            .with_cloud_endpoint("https://example.invalid")
            .build(cx)
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
fn sdk_build_rejects_invalid_postgres_configuration(cx: &mut TestAppContext) {
    let result = cx.update(|cx| {
        CditorBuilder::new()
            .with_postgres_url("postgres://localhost/cditor")
            .build(cx)
    });

    assert!(matches!(result, Err(CditorError::InvalidInput(_))));
}

#[gpui::test]
fn sdk_selection_command_and_virtual_scroll_share_runtime_truth(cx: &mut TestAppContext) {
    let component = cx.update(|cx| CditorBuilder::new().demo().build(cx).unwrap());
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
    assert!(cx.read(|cx| { handle.command_state(&CditorCommand::ToggleBold, cx).enabled }));

    let outcome = cx.update(|cx| handle.execute(CditorCommand::ToggleBold, cx).unwrap());
    assert!(outcome.changed);
    assert!(cx.read(|cx| handle.is_dirty(cx)));
    assert!(cx.read(|cx| handle.can_undo(cx)));
    cx.update(|cx| handle.set_readonly(true, cx).unwrap());
    assert_eq!(cx.read(|cx| handle.save_status(cx)), SaveStatus::Readonly);
    cx.update(|cx| handle.set_readonly(false, cx).unwrap());
    assert_eq!(cx.read(|cx| handle.save_status(cx)), SaveStatus::Dirty);

    cx.update(|cx| {
        handle
            .scroll_to_block(4, ScrollAlignment::Center, cx)
            .unwrap()
    });
}
