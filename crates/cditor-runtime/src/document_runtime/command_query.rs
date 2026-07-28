use cditor_editor_protocol::{
    command::{CommandCatalog, CommandId, CommandQueryState, CommandUnavailableReason, builtin},
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
                QueryResult::BlockExists(self.document.index.index_of(block_id).is_some())
            }
            CommandQuery::Diagnostics { include_expensive } => {
                QueryResult::Diagnostics(DiagnosticsSnapshot {
                    resident_blocks: self.loaded_payload_count(),
                    projected_blocks: self.projection_for_window().blocks.len(),
                    pending_tasks: self.pending_layout_task_count(),
                    undo_bytes: if include_expensive {
                        self.estimated_text_undo_memory_bytes()
                    } else {
                        0
                    },
                    stale_results_discarded: 0,
                })
            }
        }
    }

    fn query_command_state(&self, command_id: &CommandId) -> CommandQueryState {
        if CommandCatalog::builtin().definition(command_id).is_none() {
            return CommandQueryState::disabled(CommandUnavailableReason::UnknownCommand);
        }
        let enabled = match command_id.as_str() {
            builtin::EDIT_UNDO => self.can_undo(),
            builtin::EDIT_REDO => self.can_redo(),
            builtin::EDIT_SELECT_ALL => self.focused_block_id().is_some(),
            builtin::SELECTION_SET_DOCUMENT => self.document_block_count() > 0,
            builtin::SELECTION_FOCUS_BLOCK => self.document_block_count() > 0,
            builtin::SELECTION_FOCUS_TABLE_CELL => self.document_block_count() > 0,
            builtin::SELECTION_BLUR_TABLE_CELL => self.focused_table_cell_offset().is_some(),
            builtin::SELECTION_SET_TEXT_SURFACE => self.document_block_count() > 0,
            builtin::SELECTION_SET_TABLE_CELL => self.document_block_count() > 0,
            builtin::SELECTION_NAVIGATE_TABLE_CELL => self.focused_table_cell_offset().is_some(),
            builtin::EDIT_DELETE_SELECTION => self.has_active_selection(),
            builtin::EDIT_APPLY_CLIPBOARD_DATA | builtin::ASSET_INSERT_IMAGE_PAYLOAD => {
                self.focused_block_id().is_some()
            }
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
            builtin::BLOCK_INSERT_AFTER_FOCUSED
            | builtin::TEXT_DELETE_BACKWARD
            | builtin::TEXT_DELETE_FORWARD => self.focused_block_id().is_some(),
            builtin::BLOCK_ENSURE_TRAILING_PARAGRAPH => self.document_block_count() > 0,
            builtin::BLOCK_MOVE_BEFORE | builtin::BLOCK_MOVE_TO_PARENT => {
                self.focused_block_id().is_some()
            }
            builtin::TEXT_INSERT_SOFT_BREAK => self.can_insert_soft_line_break(),
            builtin::BLOCK_ENTER => self.can_handle_enter(),
            builtin::BLOCK_INDENT => self.can_indent_focused_block(),
            builtin::BLOCK_OUTDENT => self.can_outdent_focused_block(),
            builtin::MEDIA_SET_WIDTH_RATIO => self.focused_block_id().is_some_and(|block_id| {
                matches!(self.block_kind(block_id), Some(RichBlockKind::Image))
            }),
            builtin::TABLE_RESIZE_AXIS | builtin::TABLE_MOVE_AXIS => {
                self.focused_block_id().is_some_and(|block_id| {
                    matches!(self.block_kind(block_id), Some(RichBlockKind::Table))
                })
            }
            builtin::BLOCK_DELETE_SELECTED => self.has_selected_blocks(),
            builtin::BLOCK_APPLY_SLASH => self.focused_block_id().is_some(),
            builtin::HEADING_FOLD | builtin::HEADING_UNFOLD => {
                self.focused_block_id().is_some_and(|block_id| {
                    matches!(
                        self.block_kind(block_id),
                        Some(RichBlockKind::Heading { .. })
                    )
                })
            }
            _ => false,
        };
        if enabled {
            CommandQueryState::ENABLED
        } else {
            CommandQueryState::disabled(CommandUnavailableReason::InvalidSelection)
        }
    }
}
