use cditor_editor_protocol::{
    ProtocolError, ProtocolErrorCode,
    command::{
        BlockTransform, CommandCatalog, CommandEnvelope, CommandOutcome, CommandQueryState,
        CommandUnavailableReason, EditorCommand, builtin,
    },
    query::{CommandQuery, DiagnosticsSnapshot, DocumentSummary, QueryResult},
};

use super::*;

impl DocumentRuntime {
    pub fn query(&self, query: CommandQuery) -> QueryResult {
        match query {
            CommandQuery::State { command_id } => {
                QueryResult::CommandState(self.query_command_state(&command_id))
            }
            CommandQuery::DocumentSummary => QueryResult::DocumentSummary(DocumentSummary {
                document_id: self.document_id,
                revision: self.revision(),
                block_count: self.document_block_count(),
                readonly: false,
                dirty: self.dirty_payload_count() > 0,
            }),
            CommandQuery::BlockExists { block_id } => {
                QueryResult::BlockExists(self.index.index_of(block_id).is_some())
            }
            CommandQuery::Diagnostics { include_expensive } => {
                QueryResult::Diagnostics(DiagnosticsSnapshot {
                    resident_blocks: self.loaded_payload_count(),
                    projected_blocks: self.projection_for_window().blocks.len(),
                    pending_tasks: self.pending_layout_task_count(),
                    undo_bytes: include_expensive
                        .then(|| self.estimated_text_undo_memory_bytes())
                        .unwrap_or_default(),
                    stale_results_discarded: 0,
                })
            }
        }
    }

    fn query_command_state(
        &self,
        command_id: &cditor_editor_protocol::command::CommandId,
    ) -> CommandQueryState {
        if CommandCatalog::builtin().definition(command_id).is_none() {
            return CommandQueryState::disabled(CommandUnavailableReason::UnknownCommand);
        }
        let enabled = match command_id.as_str() {
            builtin::EDIT_UNDO => self.can_undo(),
            builtin::EDIT_REDO => self.can_redo(),
            builtin::EDIT_SELECT_ALL => self.focused_block_id().is_some(),
            builtin::EDIT_DELETE_SELECTION => self.has_active_selection(),
            builtin::FORMAT_TOGGLE_MARK
            | builtin::FORMAT_TOGGLE_BOLD
            | builtin::FORMAT_TOGGLE_ITALIC
            | builtin::FORMAT_TOGGLE_UNDERLINE
            | builtin::FORMAT_TOGGLE_STRIKE
            | builtin::FORMAT_TOGGLE_INLINE_CODE
            | builtin::FORMAT_SET_COLOR => self.focused_text_selection_range().is_some(),
            builtin::BLOCK_SET_COLOR
            | builtin::BLOCK_INSERT_AFTER
            | builtin::BLOCK_DELETE
            | builtin::BLOCK_TOGGLE_TODO
            | builtin::CODE_SET_LANGUAGE
            | builtin::BLOCK_TRANSFORM => self.focused_block_id().is_some(),
            _ => false,
        };
        if enabled {
            CommandQueryState::ENABLED
        } else {
            CommandQueryState::disabled(CommandUnavailableReason::InvalidSelection)
        }
    }

    /// Applies framework-independent editor commands at the Runtime boundary.
    /// Platform side effects and realtime IME updates remain narrow adapters.
    pub fn dispatch(&mut self, envelope: CommandEnvelope) -> Result<CommandOutcome, ProtocolError> {
        let invocation = envelope.invocation();
        CommandCatalog::builtin()
            .validate_invocation(&invocation)
            .map_err(|error| {
                ProtocolError::new(ProtocolErrorCode::InvalidArguments, error.to_string())
            })?;
        if let Some(expected) = envelope.expected_revision
            && expected != self.revision()
        {
            return Err(ProtocolError::new(
                ProtocolErrorCode::StalePrecondition,
                format!(
                    "command expected revision {expected}, current revision is {}",
                    self.revision()
                ),
            ));
        }

        self.break_typing_coalescing();
        let before_transaction = self.last_committed_transaction_id();
        let before_selection = self.document_selection_snapshot();
        let focused_before = self.focused_block_id();
        let mut affected_blocks = Vec::new();

        let changed = match envelope.command {
            EditorCommand::Undo => self.undo_focused_block().map_err(apply_error)?,
            EditorCommand::Redo => self.redo_focused_block().map_err(apply_error)?,
            EditorCommand::SelectAll => self.select_all_command(),
            EditorCommand::DeleteSelection => {
                self.delete_active_selection().map_err(apply_error)?
            }
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
            unsupported => {
                return Err(ProtocolError::new(
                    ProtocolErrorCode::InvalidArguments,
                    format!(
                        "command {} has not migrated to Runtime dispatch",
                        unsupported.stable_id()
                    ),
                ));
            }
        };

        let selection_changed = before_selection != self.document_selection_snapshot();
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

fn apply_error(message: String) -> ProtocolError {
    ProtocolError::new(ProtocolErrorCode::ApplyFailed, message)
}

#[cfg(test)]
mod tests {
    use cditor_editor_protocol::command::{CommandId, CommandSource};

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
}
