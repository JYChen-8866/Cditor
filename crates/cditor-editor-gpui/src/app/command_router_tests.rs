//! command_router 的测试（P4-015 query/execute 一致性等）。

use super::*;
use cditor_core::rich_text::{
    BlockPayload, BlockPayloadRecord, InlineColorTarget, InlineMark, InlineSpan, RichBlockKind,
    TableRange,
};
use cditor_editor_protocol::command::{
    CommandCheckState, CommandOutcomeStatus, TableAxis, TableCellNavigationDirection,
};
use gpui::{AppContext, TestAppContext};

use crate::input::routing::BoundInputAction;

fn realtime_replace(runtime: &mut cditor_runtime::DocumentRuntime, text: &str) {
    let expected = runtime.input_session_identity().unwrap();
    runtime
        .apply_realtime_input(cditor_runtime::RealtimeInputRequest {
            expected,
            input: cditor_runtime::RealtimeInput::ReplaceText { range: None, text },
        })
        .unwrap();
}

fn session_realtime_replace(session: &cditor_session::EditorSessionHandle, text: &str) {
    let expected = session.input_context().unwrap().identity.unwrap();
    session
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
        CditorCommand::FocusTableCell {
            block_id: 3,
            row: 0,
            col: 0,
            offset: Some(0),
            affinity: cditor_core::edit::TextAffinity::Downstream,
        },
        CditorCommand::BlurTableCell,
        CditorCommand::SetTableCellSelection {
            block_id: 3,
            row: 0,
            col: 0,
            anchor_offset: 0,
            focus_offset: 0,
            focus_affinity: cditor_core::edit::TextAffinity::Downstream,
        },
        CditorCommand::NavigateTableCell {
            direction: TableCellNavigationDirection::TabForward,
            extend_selection: false,
        },
        CditorCommand::SetTextSurfaceSelection {
            surface_id: cditor_core::ids::SurfaceId::ImageCaption { block_id: 1 },
            anchor_offset: 0,
            focus_offset: 0,
            focus_affinity: cditor_core::edit::TextAffinity::Downstream,
        },
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
            count: 1,
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
        CditorCommand::TableMergeCells {
            block_id: 3,
            range: TableRange::normalized(0, 0, 0, 1),
        },
        CditorCommand::TableSplitCell {
            block_id: 3,
            row: 0,
            col: 0,
        },
        CditorCommand::TableSetRangeAlign {
            block_id: 3,
            range: TableRange::normalized(0, 0, 0, 0),
            align: cditor_core::rich_text::TableCellAlign::Center,
        },
        CditorCommand::SetMediaWidthRatio {
            block_id: 1,
            ratio_milli: 800,
        },
        CditorCommand::UpdateWhiteboardScene {
            block_id: 1,
            scene_json: r#"{"elements":[]}"#.to_owned(),
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
                let before = view
                    .ready_session()
                    .and_then(|session| session.snapshot().ok())
                    .map(|snapshot| snapshot.revision);
                let result = view.dispatch_command(command.clone(), CommandSource::Sdk, cx);
                let after = view
                    .ready_session()
                    .and_then(|session| session.snapshot().ok())
                    .map(|snapshot| snapshot.revision);

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
        crate::test_support::focus_block_at_offset(&mut runtime, 1, 5);
        crate::test_support::set_document_text_selection(&mut runtime, 1, 0, 1, 5);
        CditorV2View::from_runtime_with_options(runtime, false, false, cx)
    });
    assert_query_execute_consistency(&view, cx, "focused");
}

#[gpui::test]
fn automation_and_sdk_execute_the_same_command_path(cx: &mut TestAppContext) {
    fn view(cx: &mut TestAppContext) -> gpui::Entity<CditorV2View> {
        cx.new(|cx| {
            let mut runtime = rich_document_runtime();
            crate::test_support::focus_block_at_offset(&mut runtime, 1, 0);
            crate::test_support::set_document_text_selection(&mut runtime, 1, 0, 1, 5);
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
            view.ready_session()
                .unwrap()
                .loaded_payload_record(1)
                .unwrap()
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
        crate::test_support::focus_block_at_offset(&mut runtime, 1, 0);
        crate::test_support::set_document_text_selection(&mut runtime, 1, 0, 1, 5);
        CditorV2View::from_runtime_with_options(runtime, false, false, cx)
    });

    view.update(cx, |view, cx| {
        let before = view.ready_session().unwrap().snapshot().unwrap().revision;
        let outcome = view
            .dispatch_command(CditorCommand::ToggleBold, CommandSource::Keyboard, cx)
            .expect("format command");
        let runtime = view.ready_session().unwrap();
        assert_eq!(runtime.snapshot().unwrap().revision, before + 1);
        assert_eq!(
            outcome.transaction_ids.first().copied(),
            runtime
                .persistence_runtime_snapshot()
                .unwrap()
                .last_committed_transaction_id
        );
        assert!(!outcome.transaction_ids.is_empty());
        assert_eq!(
            runtime
                .persistence_runtime_snapshot()
                .unwrap()
                .pending_structure_transactions,
            1
        );
    });
}

#[gpui::test]
fn table_command_reports_transaction_and_advances_revision_once(cx: &mut TestAppContext) {
    let view = cx.new(|cx| {
        CditorV2View::from_runtime_with_options(rich_document_runtime(), false, false, cx)
    });

    view.update(cx, |view, cx| {
        let before = view.ready_session().unwrap().snapshot().unwrap().revision;
        let outcome = view
            .dispatch_command(
                CditorCommand::TableInsertAxis {
                    block_id: 3,
                    axis: TableAxis::Row,
                    index: 1,
                    count: 1,
                },
                CommandSource::Toolbar,
                cx,
            )
            .expect("table command");
        let runtime = view.ready_session().unwrap();
        assert_eq!(runtime.snapshot().unwrap().revision, before + 1);
        assert_eq!(
            outcome.transaction_ids.first().copied(),
            runtime
                .persistence_runtime_snapshot()
                .unwrap()
                .last_committed_transaction_id
        );
        assert!(!outcome.transaction_ids.is_empty());
        assert_eq!(
            runtime
                .persistence_runtime_snapshot()
                .unwrap()
                .pending_structure_transactions,
            1
        );
        let BlockPayload::Table(table) = runtime.loaded_payload_record(3).unwrap().unwrap().payload
        else {
            panic!("table payload");
        };
        assert_eq!(table.row_count(), 3);
    });
}

#[gpui::test]
fn dispatched_command_breaks_runtime_typing_coalescing(cx: &mut TestAppContext) {
    let view = cx.new(|cx| {
        let mut runtime = rich_document_runtime();
        crate::test_support::focus_block_at_offset(&mut runtime, 1, "hello world".len());
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
        session_realtime_replace(view.ready_session().unwrap(), "c");

        view.dispatch_command(CditorCommand::Undo, CommandSource::Keyboard, cx)
            .unwrap();
        assert_eq!(
            view.ready_session()
                .unwrap()
                .loaded_payload_record(1)
                .unwrap()
                .unwrap()
                .plain_text(),
            "hello worldab"
        );
        view.dispatch_command(CditorCommand::Undo, CommandSource::Keyboard, cx)
            .unwrap();
        assert_eq!(
            view.ready_session()
                .unwrap()
                .loaded_payload_record(1)
                .unwrap()
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
fn table_tab_action_moves_after_realtime_text_input(cx: &mut TestAppContext) {
    let view = cx.new(|cx| CditorV2View::from_runtime(rich_document_runtime(), false, cx));

    view.update(cx, |view, cx| {
        view.dispatch_command(
            CditorCommand::FocusTableCell {
                block_id: 3,
                row: 0,
                col: 0,
                offset: Some(0),
                affinity: cditor_core::edit::TextAffinity::Downstream,
            },
            CommandSource::Keyboard,
            cx,
        )
        .unwrap();
        let session = view.ready_session().unwrap().clone();
        session_realtime_replace(&session, "aa");
        let command = CditorCommand::NavigateTableCell {
            direction: TableCellNavigationDirection::TabForward,
            extend_selection: false,
        };

        assert!(view.query_command(&command).enabled);
        view.handle_bound_input_action(BoundInputAction::Tab { backwards: false }, cx);
        let focused = view
            .ready_session()
            .unwrap()
            .table_interaction(None)
            .unwrap()
            .focused_cell
            .unwrap();
        assert_eq!(
            (focused.block_id, focused.row, focused.col, focused.offset),
            (3, 0, 1, 0)
        );
    });
}

#[gpui::test]
fn keyboard_enter_reveals_the_new_focused_block_in_the_document_viewport(cx: &mut TestAppContext) {
    let payloads = (1..=10)
        .map(|block_id| BlockPayloadRecord::rich_text(block_id, RichBlockKind::Paragraph, "line"))
        .collect();
    let mut runtime = cditor_runtime::DocumentRuntime::from_payloads(1, payloads, 100.0);
    crate::test_support::focus_block_at_offset(&mut runtime, 10, "line".len());
    let view = cx.new(|cx| CditorV2View::from_runtime(runtime, false, cx));

    view.update(cx, |view, cx| {
        let before = view
            .ready_session()
            .unwrap()
            .layout_viewport()
            .unwrap()
            .global_scroll_top;

        let outcome = view
            .dispatch_command(CditorCommand::HandleEnter, CommandSource::Keyboard, cx)
            .unwrap();
        let after = view
            .ready_session()
            .unwrap()
            .layout_viewport()
            .unwrap()
            .global_scroll_top;

        assert!(outcome.changed());
        assert!(after > before);
    });
}

#[test]
fn only_keyboard_editing_commands_request_document_caret_reveal() {
    assert!(keyboard_command_reveals_focused_block(
        &CditorCommand::HandleEnter,
        CommandSource::Keyboard,
    ));
    assert!(keyboard_command_reveals_focused_block(
        &CditorCommand::DeleteBackward,
        CommandSource::Keyboard,
    ));
    assert!(!keyboard_command_reveals_focused_block(
        &CditorCommand::ToggleBold,
        CommandSource::Keyboard,
    ));
    assert!(!keyboard_command_reveals_focused_block(
        &CditorCommand::HandleEnter,
        CommandSource::Sdk,
    ));
}

#[gpui::test]
fn code_caret_reveal_is_requested_once_after_a_line_break_not_ordinary_input(
    cx: &mut TestAppContext,
) {
    let block_id = 1;
    let runtime = cditor_runtime::DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            block_id,
            RichBlockKind::Code { language: None },
            "let value = 1;",
        )],
        720.0,
    );
    let view = cx.new(|cx| CditorV2View::from_runtime(runtime, false, cx));

    view.update(cx, |view, cx| {
        view.dispatch_command(
            CditorCommand::FocusBlock { block_id },
            CommandSource::Keyboard,
            cx,
        )
        .unwrap();
        let session = view.ready_session().unwrap().clone();
        session_realtime_replace(&session, "x");
        assert!(
            view.interaction
                .code_caret_reveal_after_line_break
                .is_empty()
        );

        let outcome = view
            .dispatch_command(CditorCommand::HandleEnter, CommandSource::Keyboard, cx)
            .unwrap();
        assert!(outcome.changed());
        assert!(view.take_code_caret_reveal_after_line_break(block_id));
        assert!(!view.take_code_caret_reveal_after_line_break(block_id));
    });
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
        let before_revision = view.ready_session().unwrap().snapshot().unwrap().revision;
        let outcome = view
            .dispatch_command(
                CditorCommand::EnsureTrailingParagraph,
                CommandSource::Toolbar,
                cx,
            )
            .unwrap();

        assert!(outcome.selection_changed);
        assert_eq!(
            view.ready_session().unwrap().snapshot().unwrap().revision,
            before_revision
        );
        assert_eq!(
            view.save_status(),
            &crate::persistence::EditorSaveStatus::LocallySaved
        );
    });
}

#[path = "command_router_tests/policy.rs"]
mod policy;
