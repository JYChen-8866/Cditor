use cditor_editor_protocol::{
    ProtocolError, ProtocolErrorCode,
    command::{
        AiApplyCommandMode, BlockTransform, CommandCatalog, CommandEnvelope, CommandOutcome,
        EditorCommand, TableAxis,
    },
};

use super::command_errors::{apply_error, missing_table_error, validate_expected_revision};
use super::*;

impl DocumentRuntime {
    /// Applies framework-independent editor commands at the Runtime boundary.
    /// Platform side effects and realtime IME updates remain narrow adapters.
    pub fn dispatch(&mut self, envelope: CommandEnvelope) -> Result<CommandOutcome, ProtocolError> {
        let invocation = envelope.invocation();
        let catalog = CommandCatalog::builtin();
        catalog.validate_invocation(&invocation).map_err(|error| {
            ProtocolError::new(ProtocolErrorCode::InvalidArguments, error.to_string())
        })?;
        validate_expected_revision(envelope.expected_revision, self.revision())?;

        self.break_typing_coalescing();
        let command = envelope.command;
        let mutates_document = catalog
            .definition(&invocation.id)
            .is_some_and(|definition| {
                definition.mutability
                    == cditor_editor_protocol::command::CommandMutability::Document
            });
        let before_revision = self.revision();
        let before_transaction = self.last_committed_transaction_id();
        let before_selection = (
            self.document_selection_snapshot(),
            self.unified_document_selection_snapshot(),
        );
        let focused_before = self.focused_block_id();
        let mut affected_blocks = Vec::new();
        let selection_result = self
            .dispatch_selection_command(&command)
            .map_err(apply_error)?;

        let changed = if let Some(selection_result) = selection_result {
            affected_blocks.extend(selection_result.affected_blocks);
            selection_result.changed
        } else {
            match command {
                EditorCommand::Undo => self.undo_focused_block().map_err(apply_error)?,
                EditorCommand::Redo => self.redo_focused_block().map_err(apply_error)?,
                EditorCommand::SelectAll
                | EditorCommand::SetDocumentSelection { .. }
                | EditorCommand::SetBlockSelectionRange { .. }
                | EditorCommand::FocusBlock { .. }
                | EditorCommand::FocusTableCell { .. }
                | EditorCommand::BlurTableCell
                | EditorCommand::SetTableCellSelection { .. }
                | EditorCommand::NavigateTableCell { .. }
                | EditorCommand::MoveCaret { .. }
                | EditorCommand::SetTextSurfaceSelection { .. } => {
                    unreachable!("selection commands are dispatched before document commands")
                }
                EditorCommand::DeleteSelection => {
                    self.delete_active_selection().map_err(apply_error)?
                }
                EditorCommand::ApplyClipboardData {
                    text,
                    metadata_json,
                } => self
                    .apply_clipboard_data(&text, metadata_json.as_deref())
                    .map_err(apply_error)?,
                EditorCommand::InsertImageAsset { payload } => {
                    let (image, trailing) = self
                        .insert_image_asset_after_focused(payload)
                        .map_err(apply_error)?;
                    affected_blocks.extend([image, trailing]);
                    true
                }
                EditorCommand::DeleteSelectedBlocks => self
                    .delete_selected_block_selection()
                    .map_err(apply_error)?,
                EditorCommand::ToggleBold => self
                    .toggle_inline_mark_on_selection(InlineMark::Bold)
                    .map_err(apply_error)?,
                EditorCommand::ToggleItalic => self
                    .toggle_inline_mark_on_selection(InlineMark::Italic)
                    .map_err(apply_error)?,
                EditorCommand::ToggleUnderline => self
                    .toggle_inline_mark_on_selection(InlineMark::Underline)
                    .map_err(apply_error)?,
                EditorCommand::ToggleStrike => self
                    .toggle_inline_mark_on_selection(InlineMark::Strike)
                    .map_err(apply_error)?,
                EditorCommand::ToggleInlineCode => self
                    .toggle_inline_mark_on_selection(InlineMark::Code)
                    .map_err(apply_error)?,
                EditorCommand::SetInlineColor { target, color } => self
                    .set_inline_color_on_selection(target, color.as_deref())
                    .map_err(apply_error)?,
                EditorCommand::SetBlockColor {
                    block_id,
                    target,
                    color,
                } => {
                    affected_blocks.push(block_id);
                    self.set_block_color(block_id, target, color.as_deref())
                        .map_err(apply_error)?
                }
                EditorCommand::InsertParagraphAfterBlock { block_id } => {
                    let inserted = self
                        .insert_paragraph_after_block(block_id)
                        .map_err(apply_error)?;
                    affected_blocks.extend([block_id, inserted]);
                    true
                }
                EditorCommand::MoveBlockBefore {
                    block_id,
                    before_block_id,
                } => {
                    affected_blocks.push(block_id);
                    self.move_block_subtree_before(block_id, before_block_id)
                        .map_err(apply_error)?
                }
                EditorCommand::MoveBlockToParent {
                    block_id,
                    parent_id,
                    sibling_index,
                } => {
                    affected_blocks.push(block_id);
                    self.move_block_subtree_to_parent(block_id, parent_id, sibling_index)
                        .map_err(apply_error)?
                }
                EditorCommand::InsertParagraphAfterFocused => {
                    let focused = self.focused_block_id().ok_or_else(|| {
                        ProtocolError::new(
                            ProtocolErrorCode::ApplyFailed,
                            "insert paragraph requires a focused block",
                        )
                    })?;
                    let inserted = self.insert_paragraph_after_focused().map_err(apply_error)?;
                    affected_blocks.extend([focused, inserted]);
                    true
                }
                EditorCommand::EnsureTrailingParagraph => {
                    let previous_last = self.visible_index.visible_block_ids.last().copied();
                    let changed = self
                        .focus_or_create_down_placer_paragraph()
                        .map_err(apply_error)?;
                    if let Some(block_id) = previous_last {
                        affected_blocks.push(block_id);
                    }
                    if changed
                        && let Some(block_id) = self.focused_block_id()
                        && Some(block_id) != previous_last
                    {
                        affected_blocks.push(block_id);
                    }
                    changed
                }
                EditorCommand::InsertSoftLineBreak => {
                    self.insert_soft_line_break().map_err(apply_error)?;
                    true
                }
                EditorCommand::HandleEnter => {
                    self.handle_enter().map_err(apply_error)?;
                    true
                }
                EditorCommand::IndentBlock => self.indent_focused_block().map_err(apply_error)?,
                EditorCommand::OutdentBlock => self.outdent_focused_block().map_err(apply_error)?,
                EditorCommand::DeleteBackward => self.delete_backward().map_err(apply_error)?,
                EditorCommand::DeleteForward => self.delete_forward().map_err(apply_error)?,
                EditorCommand::DeleteBlock { block_id } => {
                    affected_blocks.push(block_id);
                    self.delete_block_by_id(block_id).map_err(apply_error)?
                }
                EditorCommand::ToggleTodo { block_id } => {
                    affected_blocks.push(block_id);
                    self.toggle_todo_checked(block_id).map_err(apply_error)?
                }
                EditorCommand::SetCodeLanguage { block_id, language } => {
                    affected_blocks.push(block_id);
                    self.set_code_block_language(block_id, language)
                        .map_err(apply_error)?
                }
                EditorCommand::TransformBlock(BlockTransform::Kind(kind)) => {
                    self.convert_focused_block_kind(kind).map_err(apply_error)?
                }
                EditorCommand::ApplySlashBlock {
                    block_id,
                    trigger_range,
                    kind,
                } => {
                    affected_blocks.push(block_id);
                    self.apply_slash_block_kind(block_id, trigger_range, kind)
                        .map_err(apply_error)?
                }
                EditorCommand::ApplyAiPreview { mode } => {
                    let mode = match mode {
                        AiApplyCommandMode::Replace => AiApplyMode::Replace,
                        AiApplyCommandMode::InsertAfter => AiApplyMode::InsertAfter,
                    };
                    self.apply_ai_preview(mode).map_err(apply_error)?
                }
                EditorCommand::TableToggleHeader { block_id, axis } => {
                    affected_blocks.push(block_id);
                    let Some(record) = self.block_payload_record(block_id) else {
                        return Err(missing_table_error(block_id));
                    };
                    let BlockPayload::Table(table) = &record.payload else {
                        return Err(missing_table_error(block_id));
                    };
                    let enabled = match axis {
                        TableAxis::Row => table.header_rows > 0,
                        TableAxis::Column => table.header_cols > 0,
                    };
                    let count = usize::from(!enabled);
                    match axis {
                        TableAxis::Row => self.set_table_header_rows(block_id, count),
                        TableAxis::Column => self.set_table_header_columns(block_id, count),
                    }
                    .map_err(apply_error)?
                }
                EditorCommand::TableInsertAxis {
                    block_id,
                    axis,
                    index,
                } => {
                    affected_blocks.push(block_id);
                    match axis {
                        TableAxis::Row => self.insert_table_row(block_id, index),
                        TableAxis::Column => self.insert_table_column(block_id, index),
                    }
                    .map_err(apply_error)?
                }
                EditorCommand::TableDeleteAxis {
                    block_id,
                    axis,
                    index,
                } => {
                    affected_blocks.push(block_id);
                    match axis {
                        TableAxis::Row => self.delete_table_row(block_id, index),
                        TableAxis::Column => self.delete_table_column(block_id, index),
                    }
                    .map_err(apply_error)?
                }
                EditorCommand::TableDuplicateAxis {
                    block_id,
                    axis,
                    index,
                } => {
                    affected_blocks.push(block_id);
                    match axis {
                        TableAxis::Row => self.duplicate_table_row(block_id, index),
                        TableAxis::Column => self.duplicate_table_column(block_id, index),
                    }
                    .map_err(apply_error)?
                }
                EditorCommand::TableClearRange { block_id, range } => {
                    affected_blocks.push(block_id);
                    self.clear_table_range(block_id, range)
                        .map_err(apply_error)?
                }
                EditorCommand::TableSetRangeBackground {
                    block_id,
                    range,
                    color,
                } => {
                    affected_blocks.push(block_id);
                    self.set_table_cell_background_color(block_id, range, color)
                        .map_err(apply_error)?
                }
                EditorCommand::SetMediaWidthRatio {
                    block_id,
                    ratio_milli,
                } => {
                    affected_blocks.push(block_id);
                    self.update_image_display_width_ratio(block_id, ratio_milli)
                        .map_err(apply_error)?
                }
                EditorCommand::TableResizeAxis {
                    block_id,
                    axis,
                    index,
                    size_px,
                } => {
                    affected_blocks.push(block_id);
                    let size = TableTrackSize::Px(size_px.max(1));
                    match axis {
                        TableAxis::Row => self.set_table_row_height(block_id, index, size),
                        TableAxis::Column => self.set_table_column_width(block_id, index, size),
                    }
                    .map_err(apply_error)?
                }
                EditorCommand::TableMoveAxis {
                    block_id,
                    axis,
                    from_index,
                    to_index,
                } => {
                    affected_blocks.push(block_id);
                    match axis {
                        TableAxis::Row => self.move_table_row(block_id, from_index, to_index),
                        TableAxis::Column => self.move_table_column(block_id, from_index, to_index),
                    }
                    .map_err(apply_error)?
                }
                command @ (EditorCommand::FoldHeading | EditorCommand::UnfoldHeading) => {
                    let should_fold = matches!(command, EditorCommand::FoldHeading);
                    let block_id = self.focused_block_id().ok_or_else(|| {
                        ProtocolError::new(
                            ProtocolErrorCode::ApplyFailed,
                            "heading fold command requires a focused block",
                        )
                    })?;
                    affected_blocks.push(block_id);
                    if self.is_block_folded(block_id) == should_fold {
                        false
                    } else {
                        self.toggle_block_fold(block_id).map_err(apply_error)?
                    }
                }
                unsupported => {
                    return Err(ProtocolError::new(
                        ProtocolErrorCode::InvalidArguments,
                        format!(
                            "command {} has not migrated to Runtime dispatch",
                            unsupported.stable_id()
                        ),
                    ));
                }
            }
        };

        if changed && mutates_document && self.revision() == before_revision {
            self.note_content_changed();
        }

        let selection_changed = before_selection
            != (
                self.document_selection_snapshot(),
                self.unified_document_selection_snapshot(),
            );
        if affected_blocks.is_empty()
            && (changed || selection_changed)
            && let Some(block_id) = focused_before.or_else(|| self.focused_block_id())
        {
            affected_blocks.push(block_id);
        }
        let transaction_ids = self
            .last_committed_transaction_id()
            .filter(|transaction| Some(*transaction) != before_transaction)
            .into_iter()
            .collect();
        let mut outcome = if changed {
            CommandOutcome::applied(transaction_ids, affected_blocks)
        } else if selection_changed {
            CommandOutcome::applied_side_effect(true)
        } else {
            CommandOutcome::no_op()
        };
        outcome.selection_changed = selection_changed;
        outcome.request_repaint = changed || selection_changed;
        Ok(outcome)
    }
}

#[cfg(test)]
mod tests {
    use cditor_ai::AiStreamEvent;
    use cditor_core::rich_text::{
        TableCellPayload, TableColumnPayload, TablePayload, TableRowPayload, TableTrackSize,
    };
    use cditor_editor_protocol::{
        command::{CommandId, CommandQueryState, CommandSource, builtin},
        query::{CommandQuery, QueryResult},
    };

    use super::*;

    #[test]
    fn dispatch_applies_format_once_and_reports_transaction() {
        let mut runtime = DocumentRuntime::empty();
        runtime.focus_block_at_offset(1, 0).unwrap();
        for ch in "hello".chars() {
            runtime.insert_char(ch).unwrap();
        }
        runtime.set_document_text_selection(1, 0, 1, 5).unwrap();
        let before_revision = runtime.revision();

        let outcome = runtime
            .dispatch(CommandEnvelope::new(
                EditorCommand::ToggleBold,
                CommandSource::Toolbar,
            ))
            .unwrap();

        assert!(outcome.changed());
        assert_eq!(outcome.transaction_ids.len(), 1);
        assert_eq!(runtime.revision(), before_revision + 1);
    }

    #[test]
    fn dispatch_rejects_stale_revision_without_mutation() {
        let mut runtime = DocumentRuntime::empty();
        let before = runtime.revision();
        let error = runtime
            .dispatch(
                CommandEnvelope::new(EditorCommand::SelectAll, CommandSource::Sdk)
                    .expecting_revision(before + 1),
            )
            .unwrap_err();
        assert_eq!(error.code, ProtocolErrorCode::StalePrecondition);
        assert_eq!(runtime.revision(), before);
    }

    #[test]
    fn select_all_changes_selection_without_changing_revision() {
        let mut runtime = DocumentRuntime::empty();
        runtime.focus_block_at_offset(1, 0).unwrap();
        for ch in "hello".chars() {
            runtime.insert_char(ch).unwrap();
        }
        let before_revision = runtime.revision();

        let outcome = runtime
            .dispatch(CommandEnvelope::new(
                EditorCommand::SelectAll,
                CommandSource::Keyboard,
            ))
            .unwrap();

        assert!(outcome.selection_changed);
        assert!(outcome.transaction_ids.is_empty());
        assert_eq!(runtime.revision(), before_revision);
        assert_eq!(runtime.selected_focused_text().as_deref(), Some("hello"));
    }

    #[test]
    fn slash_command_mutates_the_target_once_through_dispatch() {
        let mut runtime = DocumentRuntime::empty();
        runtime.focus_block_at_offset(1, 0).unwrap();
        for ch in "/h2".chars() {
            runtime.insert_char(ch).unwrap();
        }
        let before_revision = runtime.revision();

        let outcome = runtime
            .dispatch(CommandEnvelope::new(
                EditorCommand::ApplySlashBlock {
                    block_id: 1,
                    trigger_range: 0..3,
                    kind: RichBlockKind::Heading { level: 2 },
                },
                CommandSource::SlashMenu,
            ))
            .unwrap();

        assert!(outcome.changed());
        assert_eq!(outcome.affected_blocks, vec![1]);
        assert_eq!(outcome.transaction_ids.len(), 1);
        assert_eq!(runtime.revision(), before_revision + 1);
        assert_eq!(
            runtime.block_kind(1),
            Some(RichBlockKind::Heading { level: 2 })
        );
        assert_eq!(runtime.block_payload_record(1).unwrap().plain_text(), "");
    }

    #[test]
    fn heading_fold_is_idempotent_and_advances_revision_once() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![
                BlockPayloadRecord::rich_text(1, RichBlockKind::Heading { level: 1 }, "Heading"),
                BlockPayloadRecord::rich_text(2, RichBlockKind::Paragraph, "Body"),
            ],
            720.0,
        );
        runtime.focus_block_at_offset(1, 0).unwrap();
        let before_revision = runtime.revision();

        let first = runtime
            .dispatch(CommandEnvelope::new(
                EditorCommand::FoldHeading,
                CommandSource::Toolbar,
            ))
            .unwrap();
        let second = runtime
            .dispatch(CommandEnvelope::new(
                EditorCommand::FoldHeading,
                CommandSource::Toolbar,
            ))
            .unwrap();

        assert!(first.changed());
        assert!(!second.changed());
        assert!(runtime.is_block_folded(1));
        assert_eq!(runtime.revision(), before_revision + 1);
        assert!(first.transaction_ids.is_empty());
    }

    #[test]
    fn history_commands_execute_through_dispatch() {
        let mut runtime = DocumentRuntime::empty();
        runtime.focus_block_at_offset(1, 0).unwrap();
        runtime.insert_char('x').unwrap();
        let edited_revision = runtime.revision();

        let undo = runtime
            .dispatch(CommandEnvelope::new(
                EditorCommand::Undo,
                CommandSource::Sdk,
            ))
            .unwrap();
        let undo_revision = runtime.revision();
        let redo = runtime
            .dispatch(CommandEnvelope::new(
                EditorCommand::Redo,
                CommandSource::Sdk,
            ))
            .unwrap();
        let redo_revision = runtime.revision();

        assert!(undo.changed());
        assert!(redo.changed());
        assert!(undo_revision > edited_revision);
        assert!(redo_revision > undo_revision);
        assert_eq!(runtime.block_payload_record(1).unwrap().plain_text(), "x");
    }

    #[test]
    fn table_command_dispatches_one_transaction_and_revision() {
        let table = TablePayload {
            rows: vec![TableRowPayload {
                cells: vec![TableCellPayload::plain("A")],
                height: TableTrackSize::Auto,
            }],
            columns: vec![TableColumnPayload::default()],
            ..TablePayload::default()
        };
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord {
                block_id: 1,
                content_version: 1,
                kind: RichBlockKind::Table,
                payload: BlockPayload::Table(table),
            }],
            720.0,
        );
        let before_revision = runtime.revision();

        let outcome = runtime
            .dispatch(CommandEnvelope::new(
                EditorCommand::TableInsertAxis {
                    block_id: 1,
                    axis: TableAxis::Row,
                    index: 1,
                },
                CommandSource::Toolbar,
            ))
            .unwrap();

        let BlockPayload::Table(table) = runtime.block_payload_record(1).unwrap().payload else {
            panic!("table payload");
        };
        assert_eq!(table.row_count(), 2);
        assert_eq!(runtime.revision(), before_revision + 1);
        assert_eq!(outcome.transaction_ids.len(), 1);
        assert_eq!(outcome.affected_blocks, vec![1]);
    }

    #[test]
    fn ai_preview_apply_keeps_stale_validation_inside_runtime_dispatch() {
        let mut runtime = DocumentRuntime::empty();
        runtime.focus_block_at_offset(1, 0).unwrap();
        let request = runtime.begin_ai_request("continue").unwrap();
        let request_id = request.request.request_id;
        assert_eq!(
            runtime.apply_ai_stream_event(AiStreamEvent::Delta {
                request_id,
                text: "generated".to_owned(),
            }),
            AiStreamApplyResult::Applied
        );
        assert_eq!(
            runtime.apply_ai_stream_event(AiStreamEvent::Done { request_id }),
            AiStreamApplyResult::Applied
        );
        let before_revision = runtime.revision();

        let outcome = runtime
            .dispatch(CommandEnvelope::new(
                EditorCommand::ApplyAiPreview {
                    mode: AiApplyCommandMode::InsertAfter,
                },
                CommandSource::Ai,
            ))
            .unwrap();

        assert!(outcome.changed());
        assert_eq!(runtime.revision(), before_revision + 1);
        assert_eq!(
            runtime.block_payload_record(1).unwrap().plain_text(),
            "generated"
        );
        assert!(runtime.ai_session_snapshot().is_none());
    }

    #[test]
    fn unsupported_platform_command_fails_before_mutation() {
        let mut runtime = DocumentRuntime::empty();
        let before = runtime.revision();
        let error = runtime
            .dispatch(CommandEnvelope::new(
                EditorCommand::CopySelection,
                CommandSource::Keyboard,
            ))
            .unwrap_err();
        assert_eq!(error.code, ProtocolErrorCode::InvalidArguments);
        assert_eq!(runtime.revision(), before);
    }

    #[test]
    fn query_reports_command_block_and_bounded_diagnostics() {
        let mut runtime = DocumentRuntime::empty();
        let undo = runtime.query(CommandQuery::State {
            command_id: CommandId::builtin(builtin::EDIT_UNDO),
        });
        assert!(matches!(
            undo,
            QueryResult::CommandState(CommandQueryState { enabled: false, .. })
        ));

        runtime.focus_block_at_offset(1, 0).unwrap();
        runtime.insert_char('x').unwrap();
        let undo = runtime.query(CommandQuery::State {
            command_id: CommandId::builtin(builtin::EDIT_UNDO),
        });
        assert!(matches!(
            undo,
            QueryResult::CommandState(CommandQueryState { enabled: true, .. })
        ));
        assert_eq!(
            runtime.query(CommandQuery::BlockExists { block_id: 1 }),
            QueryResult::BlockExists(true)
        );
        let QueryResult::Diagnostics(diagnostics) = runtime.query(CommandQuery::Diagnostics {
            include_expensive: false,
        }) else {
            panic!("expected diagnostics");
        };
        assert_eq!(diagnostics.undo_bytes, 0);
        assert!(diagnostics.projected_blocks <= runtime.document_block_count());
    }

    #[test]
    fn projection_facade_preserves_request_identity_and_bounds_the_window() {
        let mut runtime = DocumentRuntime::empty();
        let projection =
            runtime.projection(cditor_editor_protocol::projection::ProjectionRequest {
                viewport_revision: 42,
                include_diagnostics: false,
            });

        assert_eq!(projection.viewport_revision, 42);
        assert_eq!(projection.document_id, runtime.document_id);
        assert!(projection.blocks.len() <= 320);
        assert!(projection.debug.page_boundaries.is_empty());
    }
}
