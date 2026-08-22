use super::*;

#[test]
fn readonly_policy_allows_query_and_clipboard_without_allowing_mutation() {
    assert!(!command_mutates_document(&CditorCommand::SelectAll));
    assert!(!command_mutates_document(&CditorCommand::CopySelection));
    assert!(!command_mutates_document(
        &CditorCommand::CopySelectionAsMarkdown
    ));
    assert!(!command_mutates_document(&CditorCommand::MoveCaret {
        direction: CaretDirection::NextVisual,
        extend_selection: false,
    }));
    assert!(command_mutates_document(&CditorCommand::CutSelection));
    assert!(command_mutates_document(&CditorCommand::ToggleBold));
}

#[test]
fn selection_payload_materialization_covers_all_selection_consumers() {
    for command in [
        CditorCommand::CopySelection,
        CditorCommand::CopySelectionAsMarkdown,
        CditorCommand::CutSelection,
        CditorCommand::DeleteSelection,
        CditorCommand::DeleteBackward,
        CditorCommand::DeleteForward,
    ] {
        assert!(command_requires_selection_materialization(&command));
    }
    assert!(!command_requires_selection_materialization(
        &CditorCommand::SelectAll
    ));
    assert!(!command_requires_selection_materialization(
        &CditorCommand::ToggleBold
    ));
}

#[test]
fn inline_mark_query_reports_checked_mixed_and_unchecked() {
    let mut runtime = cditor_runtime::DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord {
            block_id: 1,
            content_version: 1,
            kind: RichBlockKind::Paragraph,
            payload: BlockPayload::RichText {
                spans: vec![
                    InlineSpan {
                        text: "bold".to_owned(),
                        marks: vec![InlineMark::Bold],
                    },
                    InlineSpan::plain(" plain"),
                ],
            },
        }],
        720.0,
    );
    crate::test_support::focus_block_at_offset(&mut runtime, 1, 0);
    crate::test_support::set_document_text_selection(&mut runtime, 1, 0, 1, 4);
    assert_eq!(
        runtime
            .query_editor_command(&CditorCommand::ToggleBold)
            .check,
        CommandCheckState::Checked
    );

    crate::test_support::set_document_text_selection(&mut runtime, 1, 0, 1, 10);
    assert_eq!(
        runtime
            .query_editor_command(&CditorCommand::ToggleBold)
            .check,
        CommandCheckState::Mixed
    );
    assert_eq!(
        runtime
            .query_editor_command(&CditorCommand::ToggleItalic)
            .check,
        CommandCheckState::Unchecked
    );
}
