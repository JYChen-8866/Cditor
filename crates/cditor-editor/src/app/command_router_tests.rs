//! command_router 的测试（P4-015 query/execute 一致性等）。

use super::*;
use cditor_core::rich_text::{
    BlockPayload, BlockPayloadRecord, InlineColorTarget, InlineMark, InlineSpan, RichBlockKind,
    TableRange,
};
use cditor_editor_protocol::command::CommandOutcomeStatus;
use gpui::{AppContext, TestAppContext};

fn realtime_replace(runtime: &mut cditor_runtime::DocumentRuntime, text: &str) {
    let expected = runtime.input_session_identity().unwrap();
    runtime
        .apply_realtime_input(cditor_runtime::RealtimeInputRequest {
            expected,
            input: cditor_runtime::RealtimeInput::ReplaceText { range: None, text },
        })
        .unwrap();
}

/// P4-015：每个 command variant 一个代表性实例（参数只需类型合法）。
fn representative_commands() -> Vec<CditorCommand> {
    use cditor_editor_protocol::command::BlockTransform;
    use cditor_editor_protocol::command::{AiApplyCommandMode, BlockInput};
    vec![
        CditorCommand::Undo,
        CditorCommand::Redo,
        CditorCommand::SelectAll,
        CditorCommand::CopySelection,
        CditorCommand::CutSelection,
        CditorCommand::PasteClipboard,
        CditorCommand::ApplyClipboardData {
            text: "plain".to_owned(),
            metadata_json: None,
        },
        CditorCommand::InsertImageAsset {
            payload: cditor_core::rich_text::ImagePayload::default(),
        },
        CditorCommand::DeleteSelection,
        CditorCommand::ToggleBold,
        CditorCommand::ToggleItalic,
        CditorCommand::ToggleUnderline,
        CditorCommand::ToggleStrike,
        CditorCommand::ToggleInlineCode,
        CditorCommand::SetInlineColor {
            target: InlineColorTarget::Text,
            color: Some("#cf222e".to_owned()),
        },
        CditorCommand::SetBlockColor {
            block_id: 1,
            target: InlineColorTarget::Background,
            color: None,
        },
        CditorCommand::InsertParagraphAfterBlock { block_id: 1 },
        CditorCommand::MoveBlockBefore {
            block_id: 1,
            before_block_id: Some(2),
        },
        CditorCommand::MoveBlockToParent {
            block_id: 2,
            parent_id: None,
            sibling_index: 0,
        },
        CditorCommand::DeleteBlock { block_id: 1 },
        CditorCommand::ToggleTodo { block_id: 1 },
        CditorCommand::SetCodeLanguage {
            block_id: 1,
            language: Some("rust".to_owned()),
        },
        CditorCommand::CopyBlockText { block_id: 1 },
        CditorCommand::ApplyAiPreview {
            mode: AiApplyCommandMode::Replace,
        },
        CditorCommand::InsertBlock(BlockInput {
            kind: RichBlockKind::Paragraph,
            attrs: Default::default(),
            payload: BlockPayload::RichText { spans: Vec::new() },
        }),
        CditorCommand::TransformBlock(BlockTransform::Kind(RichBlockKind::Quote)),
        CditorCommand::DeleteSelectedBlocks,
        CditorCommand::DuplicateSelectedBlocks,
        CditorCommand::InsertTable {
            rows: 2,
            columns: 2,
        },
        CditorCommand::InsertImage,
        CditorCommand::InsertWhiteboard,
        CditorCommand::InsertMermaid,
        CditorCommand::FoldHeading,
        CditorCommand::UnfoldHeading,
        CditorCommand::InsertParagraphAfterFocused,
        CditorCommand::EnsureTrailingParagraph,
        CditorCommand::InsertSoftLineBreak,
        CditorCommand::HandleEnter,
        CditorCommand::IndentBlock,
        CditorCommand::OutdentBlock,
        CditorCommand::DeleteBackward,
        CditorCommand::DeleteForward,
        CditorCommand::SetDocumentSelection {
            selection: cditor_core::edit::DocumentSelection {
                anchor: cditor_core::edit::TextPosition::downstream(1, 0),
                focus: cditor_core::edit::TextPosition::downstream(1, 1),
            },
        },
        CditorCommand::FocusBlock { block_id: 1 },
        CditorCommand::MoveCaret {
            direction: CaretDirection::NextVisual,
            extend_selection: false,
        },
        CditorCommand::ApplySlashBlock {
            block_id: 1,
            trigger_range: 0..1,
            kind: RichBlockKind::Heading { level: 2 },
        },
        CditorCommand::TableToggleHeader {
            block_id: 3,
            axis: TableAxis::Row,
        },
        CditorCommand::TableInsertAxis {
            block_id: 3,
            axis: TableAxis::Row,
            index: 0,
        },
        CditorCommand::TableDeleteAxis {
            block_id: 3,
            axis: TableAxis::Column,
            index: 0,
        },
        CditorCommand::TableDuplicateAxis {
            block_id: 3,
            axis: TableAxis::Row,
            index: 0,
        },
        CditorCommand::TableClearRange {
            block_id: 3,
            range: TableRange::normalized(0, 0, 0, 0),
        },
        CditorCommand::TableSetRangeBackground {
            block_id: 3,
            range: TableRange::normalized(0, 0, 0, 0),
            color: Some("#fff8c5".to_owned()),
        },
        CditorCommand::SetMediaWidthRatio {
            block_id: 1,
            ratio_milli: 800,
        },
        CditorCommand::TableResizeAxis {
            block_id: 3,
            axis: TableAxis::Column,
            index: 0,
            size_px: 180,
        },
        CditorCommand::TableMoveAxis {
            block_id: 3,
            axis: TableAxis::Row,
            from_index: 0,
            to_index: 1,
        },
    ]
}

fn rich_document_runtime() -> cditor_runtime::DocumentRuntime {
    let mut table = cditor_core::rich_text::TablePayload {
        rows: vec![
            cditor_core::rich_text::TableRowPayload {
                cells: vec![
                    cditor_core::rich_text::TableCellPayload::plain("A"),
                    cditor_core::rich_text::TableCellPayload::plain("B"),
                ],
                height: Default::default(),
            },
            cditor_core::rich_text::TableRowPayload {
                cells: vec![
                    cditor_core::rich_text::TableCellPayload::plain("C"),
                    cditor_core::rich_text::TableCellPayload::plain("D"),
                ],
                height: Default::default(),
            },
        ],
        columns: Vec::new(),
        header_rows: 1,
        header_cols: 0,
        header_style: Default::default(),
    };
    table.normalize();
    cditor_runtime::DocumentRuntime::from_payloads(
        1,
        vec![
            BlockPayloadRecord::rich_text(1, RichBlockKind::Paragraph, "hello world"),
            BlockPayloadRecord::rich_text(2, RichBlockKind::Heading { level: 2 }, "title"),
            BlockPayloadRecord {
                block_id: 3,
                content_version: 1,
                kind: RichBlockKind::Table,
                payload: BlockPayload::Table(table),
            },
        ],
        720.0,
    )
}

/// query 说禁用 -> dispatch 必须拒绝且不改文档；query 说启用 ->
/// dispatch 不得返回前置条件类错误（NotReady/Readonly/InvalidSelection）。
fn assert_query_execute_consistency(
    view: &gpui::Entity<CditorV2View>,
    cx: &mut TestAppContext,
    context_label: &str,
) {
    for command in representative_commands() {
        view.update(cx, |view, cx| {
                let state = view.query_command(&command);
                let before = view.ready_runtime_ref().map(|runtime| runtime.revision());
                let result = view.dispatch_command(command.clone(), CommandSource::Sdk, cx);
                let after = view.ready_runtime_ref().map(|runtime| runtime.revision());

                if state.enabled {
                    if let Err(error) = &result {
                        assert!(
                            !matches!(
                                error,
                                CditorError::NotReady
                                    | CditorError::Readonly
                                    | CditorError::InvalidSelection
                            ),
                            "[{context_label}] {command:?} was query-enabled but execute failed a precondition: {error:?}"
                        );
                    }
                } else {
                    assert!(
                        result.is_err(),
                        "[{context_label}] {command:?} was query-disabled but execute succeeded"
                    );
                    assert_eq!(
                        before, after,
                        "[{context_label}] disabled {command:?} must not mutate the document"
                    );
                }
            });
    }
}

#[gpui::test]
fn readonly_view_disables_all_mutating_commands_and_refuses_execution(cx: &mut TestAppContext) {
    let view = cx.new(|cx| {
        CditorV2View::from_runtime_with_options(rich_document_runtime(), false, true, cx)
    });
    for command in representative_commands() {
        view.update(cx, |view, _| {
            let state = view.query_command(&command);
            if command_mutates_document(&command) {
                assert!(
                    !state.enabled,
                    "readonly must disable mutating command {command:?}"
                );
                assert_eq!(state.reason, Some(CommandUnavailableReason::Readonly));
            }
        });
    }
    assert_query_execute_consistency(&view, cx, "readonly");
}

#[gpui::test]
fn unfocused_document_query_and_execute_stay_consistent(cx: &mut TestAppContext) {
    let view = cx.new(|cx| {
        CditorV2View::from_runtime_with_options(rich_document_runtime(), false, false, cx)
    });
    assert_query_execute_consistency(&view, cx, "no-focus");
}

#[gpui::test]
fn focused_document_query_and_execute_stay_consistent(cx: &mut TestAppContext) {
    let view = cx.new(|cx| {
        let mut runtime = rich_document_runtime();
        runtime.focus_block_at_offset(1, 5).unwrap();
        runtime.set_document_text_selection(1, 0, 1, 5).unwrap();
        CditorV2View::from_runtime_with_options(runtime, false, false, cx)
    });
    assert_query_execute_consistency(&view, cx, "focused");
}

#[gpui::test]
fn automation_and_sdk_execute_the_same_command_path(cx: &mut TestAppContext) {
    fn view(cx: &mut TestAppContext) -> gpui::Entity<CditorV2View> {
        cx.new(|cx| {
            let mut runtime = rich_document_runtime();
            runtime.focus_block_at_offset(1, 0).unwrap();
            runtime.set_document_text_selection(1, 0, 1, 5).unwrap();
            CditorV2View::from_runtime_with_options(runtime, false, false, cx)
        })
    }

    let sdk = view(cx);
    let automation = view(cx);
    for (view, source) in [
        (&sdk, CommandSource::Sdk),
        (&automation, CommandSource::Automation),
    ] {
        view.update(cx, |view, cx| {
            let outcome = view
                .dispatch_command(CditorCommand::ToggleBold, source, cx)
                .expect("command applies");
            assert_eq!(outcome.status, CommandOutcomeStatus::Applied);
        });
    }

    let payload = |view: &gpui::Entity<CditorV2View>, cx: &mut TestAppContext| {
        view.read_with(cx, |view, _| {
            view.ready_runtime_ref()
                .unwrap()
                .block_payload_record(1)
                .unwrap()
                .payload
        })
    };
    assert_eq!(payload(&sdk, cx), payload(&automation, cx));
}

#[gpui::test]
fn format_command_reports_transaction_and_advances_revision_once(cx: &mut TestAppContext) {
    let view = cx.new(|cx| {
        let mut runtime = rich_document_runtime();
        runtime.focus_block_at_offset(1, 0).unwrap();
        runtime.set_document_text_selection(1, 0, 1, 5).unwrap();
        CditorV2View::from_runtime_with_options(runtime, false, false, cx)
    });

    view.update(cx, |view, cx| {
        let before = view.ready_runtime_ref().unwrap().revision();
        let outcome = view
            .dispatch_command(CditorCommand::ToggleBold, CommandSource::Keyboard, cx)
            .expect("format command");
        let runtime = view.ready_runtime_ref().unwrap();
        assert_eq!(runtime.revision(), before + 1);
        assert_eq!(
            outcome.transaction_ids.first().copied(),
            runtime.last_committed_transaction_id()
        );
        assert!(!outcome.transaction_ids.is_empty());
        assert_eq!(runtime.pending_structure_transaction_count(), 1);
    });
}

#[gpui::test]
fn table_command_reports_transaction_and_advances_revision_once(cx: &mut TestAppContext) {
    let view = cx.new(|cx| {
        CditorV2View::from_runtime_with_options(rich_document_runtime(), false, false, cx)
    });

    view.update(cx, |view, cx| {
        let before = view.ready_runtime_ref().unwrap().revision();
        let outcome = view
            .dispatch_command(
                CditorCommand::TableInsertAxis {
                    block_id: 3,
                    axis: TableAxis::Row,
                    index: 1,
                },
                CommandSource::Toolbar,
                cx,
            )
            .expect("table command");
        let runtime = view.ready_runtime_ref().unwrap();
        assert_eq!(runtime.revision(), before + 1);
        assert_eq!(
            outcome.transaction_ids.first().copied(),
            runtime.last_committed_transaction_id()
        );
        assert!(!outcome.transaction_ids.is_empty());
        assert_eq!(runtime.pending_structure_transaction_count(), 1);
        let BlockPayload::Table(table) = runtime.block_payload_record(3).unwrap().payload else {
            panic!("table payload");
        };
        assert_eq!(table.row_count(), 3);
    });
}

#[gpui::test]
fn dispatched_command_breaks_runtime_typing_coalescing(cx: &mut TestAppContext) {
    let view = cx.new(|cx| {
        let mut runtime = rich_document_runtime();
        runtime
            .focus_block_at_offset(1, "hello world".len())
            .unwrap();
        realtime_replace(&mut runtime, "a");
        realtime_replace(&mut runtime, "b");
        CditorV2View::from_runtime_with_options(runtime, false, false, cx)
    });

    view.update(cx, |view, cx| {
        view.dispatch_command(
            CditorCommand::CopyBlockText { block_id: 1 },
            CommandSource::Keyboard,
            cx,
        )
        .unwrap();
        realtime_replace(view.ready_runtime().unwrap(), "c");

        view.dispatch_command(CditorCommand::Undo, CommandSource::Keyboard, cx)
            .unwrap();
        assert_eq!(
            view.ready_runtime_ref()
                .unwrap()
                .block_payload_record(1)
                .unwrap()
                .plain_text(),
            "hello worldab"
        );
        view.dispatch_command(CditorCommand::Undo, CommandSource::Keyboard, cx)
            .unwrap();
        assert_eq!(
            view.ready_runtime_ref()
                .unwrap()
                .block_payload_record(1)
                .unwrap()
                .plain_text(),
            "hello world"
        );
    });
}

#[test]
fn every_keyboard_document_command_has_a_router_handler() {
    let commands = [
        CditorCommand::Undo,
        CditorCommand::Redo,
        CditorCommand::SelectAll,
        CditorCommand::CopySelection,
        CditorCommand::CutSelection,
        CditorCommand::PasteClipboard,
        CditorCommand::DeleteBackward,
        CditorCommand::DeleteForward,
        CditorCommand::ToggleBold,
        CditorCommand::ToggleItalic,
        CditorCommand::ToggleUnderline,
        CditorCommand::ToggleInlineCode,
    ];
    assert!(
        commands
            .iter()
            .all(|command| gui_handler_for_command(command).is_some())
    );
}

#[test]
fn keyboard_document_mutations_are_owned_by_runtime_dispatch() {
    let commands = [
        CditorCommand::InsertParagraphAfterFocused,
        CditorCommand::InsertSoftLineBreak,
        CditorCommand::HandleEnter,
        CditorCommand::IndentBlock,
        CditorCommand::OutdentBlock,
        CditorCommand::DeleteBackward,
        CditorCommand::DeleteForward,
    ];
    assert!(commands.iter().all(runtime_dispatches));
}

#[gpui::test]
fn down_placer_focus_without_document_change_does_not_mark_the_editor_dirty(
    cx: &mut TestAppContext,
) {
    let runtime = cditor_runtime::DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "",
        )],
        720.0,
    );
    let view = cx.new(|cx| CditorV2View::from_runtime(runtime, false, cx));

    view.update(cx, |view, cx| {
        let before_revision = view.ready_runtime_ref().unwrap().revision();
        let outcome = view
            .dispatch_command(
                CditorCommand::EnsureTrailingParagraph,
                CommandSource::Toolbar,
                cx,
            )
            .unwrap();

        assert!(outcome.selection_changed);
        assert_eq!(
            view.ready_runtime_ref().unwrap().revision(),
            before_revision
        );
        assert_eq!(
            view.save_status(),
            &crate::persistence::EditorSaveStatus::Clean
        );
    });
}

#[test]
fn readonly_policy_allows_query_and_clipboard_without_allowing_mutation() {
    assert!(!command_mutates_document(&CditorCommand::SelectAll));
    assert!(!command_mutates_document(&CditorCommand::CopySelection));
    assert!(!command_mutates_document(&CditorCommand::MoveCaret {
        direction: CaretDirection::NextVisual,
        extend_selection: false,
    }));
    assert!(command_mutates_document(&CditorCommand::CutSelection));
    assert!(command_mutates_document(&CditorCommand::ToggleBold));
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
    runtime.focus_block_at_offset(1, 0).unwrap();
    runtime.set_document_text_selection(1, 0, 1, 4).unwrap();
    assert_eq!(
        inline_mark_check_state(&runtime, &InlineMark::Bold),
        CommandCheckState::Checked
    );

    runtime.set_document_text_selection(1, 0, 1, 10).unwrap();
    assert_eq!(
        inline_mark_check_state(&runtime, &InlineMark::Bold),
        CommandCheckState::Mixed
    );
    assert_eq!(
        inline_mark_check_state(&runtime, &InlineMark::Italic),
        CommandCheckState::Unchecked
    );
}
