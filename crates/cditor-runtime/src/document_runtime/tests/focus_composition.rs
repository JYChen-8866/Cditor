use super::*;

fn two_paragraph_runtime() -> DocumentRuntime {
    DocumentRuntime::from_payloads(
        1,
        vec![
            BlockPayloadRecord::rich_text(1, RichBlockKind::Paragraph, "ab"),
            BlockPayloadRecord::rich_text(2, RichBlockKind::Paragraph, "cd"),
        ],
        720.0,
    )
}

#[test]
fn same_text_surface_focus_preserves_active_composition_and_session() {
    let mut runtime = two_paragraph_runtime();
    runtime.focus_block_at_offset(1, 1).unwrap();
    runtime.begin_or_update_composition(1, 1..1, "中").unwrap();
    let identity = runtime.input_session_identity().unwrap();
    let selection = runtime.editing.as_ref().unwrap().selected_range.clone();
    let marked = runtime.editing.as_ref().unwrap().marked_range.clone();

    runtime.focus_block_at_offset(1, 0).unwrap();

    assert_eq!(runtime.input_session_identity(), Some(identity));
    assert_eq!(runtime.editing.as_ref().unwrap().selected_range, selection);
    assert_eq!(runtime.editing.as_ref().unwrap().marked_range, marked);
    assert!(runtime.active_composition().is_some());
    assert_eq!(
        runtime.document.payload_window.get(1).unwrap().plain_text(),
        "ab"
    );
}

#[test]
fn cross_block_focus_commits_composition_before_switching_target() {
    let mut runtime = two_paragraph_runtime();
    runtime.focus_block_at_offset(1, 1).unwrap();
    runtime.begin_or_update_composition(1, 1..1, "中").unwrap();
    let previous_identity = runtime.input_session_identity().unwrap();

    runtime.focus_block_at_offset(2, 0).unwrap();

    assert_eq!(
        runtime.document.payload_window.get(1).unwrap().plain_text(),
        "a中b"
    );
    assert!(runtime.active_composition().is_none());
    assert_eq!(
        runtime.input_session_target(),
        Some(InputTarget::BlockText { block_id: 2 })
    );
    assert_ne!(runtime.input_session_identity(), Some(previous_identity));
    assert_eq!(runtime.undo_stacks.get(&1).map(Vec::len), Some(1));
}

#[test]
fn failed_cross_surface_commit_preserves_original_focus_and_composition() {
    let mut runtime = two_paragraph_runtime();
    runtime.focus_block_at_offset(1, 1).unwrap();
    runtime.begin_or_update_composition(1, 1..1, "中").unwrap();
    let selected_range = runtime.editing.as_ref().unwrap().selected_range.clone();
    runtime
        .document
        .payload_window
        .payloads
        .get_mut(&1)
        .unwrap()
        .content_version += 1;

    let error = runtime.focus_block_at_offset(2, 0).unwrap_err();

    assert!(error.contains("stale composition content version"));
    assert_eq!(runtime.focused_block_id(), Some(1));
    assert_eq!(
        runtime.editing.as_ref().unwrap().selected_range,
        selected_range
    );
    assert!(runtime.editing.as_ref().unwrap().composition.is_some());
    assert_eq!(
        runtime.document.payload_window.get(1).unwrap().plain_text(),
        "ab"
    );
    assert!(runtime.undo_stacks.get(&1).is_none_or(Vec::is_empty));
    assert!(runtime.commit_composition_before_external_focus().is_err());
    assert!(runtime.editing.as_ref().unwrap().composition.is_some());
}

#[test]
fn table_cell_composition_commits_before_block_focus_switch() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![
            sample_table_payload(),
            BlockPayloadRecord::rich_text(1, RichBlockKind::Paragraph, "paragraph"),
        ],
        720.0,
    );
    runtime.focus_table_cell_at_offset(10, 0, 1, 1).unwrap();
    runtime.begin_or_update_composition(10, 1..1, "中").unwrap();

    runtime.focus_block_at_offset(1, 0).unwrap();

    let BlockPayload::Table(table) = &runtime.document.payload_window.get(10).unwrap().payload
    else {
        panic!("expected table payload");
    };
    assert_eq!(table.cell_plain_text(0, 1).as_deref(), Some("B中"));
    assert_eq!(
        runtime.input_session_target(),
        Some(InputTarget::BlockText { block_id: 1 })
    );
    assert!(runtime.active_composition().is_none());
}

#[test]
fn same_table_cell_focus_preserves_composition_but_blur_commits_it() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);
    runtime.focus_table_cell_at_offset(10, 0, 1, 1).unwrap();
    runtime.begin_or_update_composition(10, 1..1, "中").unwrap();
    let identity = runtime.input_session_identity();

    runtime.focus_table_cell_at_offset(10, 0, 1, 0).unwrap();

    assert_eq!(runtime.input_session_identity(), identity);
    assert!(runtime.active_composition().is_some());
    assert!(runtime.try_blur_table_cell().unwrap());
    let BlockPayload::Table(table) = &runtime.document.payload_window.get(10).unwrap().payload
    else {
        panic!("expected table payload");
    };
    assert_eq!(table.cell_plain_text(0, 1).as_deref(), Some("B中"));
    assert_eq!(
        runtime.input_session_target(),
        Some(InputTarget::BlockChrome { block_id: 10 })
    );
    assert!(runtime.focused_table_cell_offset().is_none());
}

#[test]
fn external_focus_commit_uses_the_same_preflight_and_single_undo_boundary() {
    let mut runtime = two_paragraph_runtime();
    runtime.focus_block_at_offset(1, 1).unwrap();
    runtime.begin_or_update_composition(1, 1..1, "中").unwrap();

    assert!(runtime.commit_composition_before_external_focus().unwrap());

    assert_eq!(
        runtime.document.payload_window.get(1).unwrap().plain_text(),
        "a中b"
    );
    assert!(runtime.active_composition().is_none());
    assert_eq!(runtime.undo_stacks.get(&1).map(Vec::len), Some(1));
    assert!(!runtime.commit_composition_before_external_focus().unwrap());
}
