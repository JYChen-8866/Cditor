use cditor_editor_protocol::command::{
    BlockTransform, CommandCheckState, CommandQueryState, CommandUnavailableReason, EditorCommand,
    TableAxis,
};

use super::*;

impl DocumentRuntime {
    pub fn query_editor_command(&self, command: &EditorCommand) -> CommandQueryState {
        let enabled = match command {
            EditorCommand::Undo => self.can_undo(),
            EditorCommand::Redo => self.can_redo(),
            EditorCommand::SelectAll => true,
            EditorCommand::SetDocumentSelection { selection } => {
                self.has_block(selection.anchor.block_id)
                    && self.has_block(selection.focus.block_id)
            }
            EditorCommand::SetBlockSelectionRange {
                anchor_block_id,
                focus_block_id,
            } => self.has_block(*anchor_block_id) && self.has_block(*focus_block_id),
            EditorCommand::FocusBlock { block_id } => self.has_block(*block_id),
            EditorCommand::FocusTableCell {
                block_id, row, col, ..
            }
            | EditorCommand::SetTableCellSelection {
                block_id, row, col, ..
            } => self
                .table_dimensions(*block_id)
                .is_some_and(|(rows, columns)| *row < rows && *col < columns),
            EditorCommand::BlurTableCell => self.focused_table_cell_offset().is_some(),
            EditorCommand::SetTextSurfaceSelection { surface_id, .. } => {
                self.text_surface_snapshot(*surface_id).is_some()
            }
            EditorCommand::CopySelection
            | EditorCommand::CutSelection
            | EditorCommand::DeleteSelection => self.has_active_selection(),
            EditorCommand::PasteClipboard
            | EditorCommand::ApplyClipboardData { .. }
            | EditorCommand::InsertImageAsset { .. } => self.focused_block_id().is_some(),
            EditorCommand::ToggleBold
            | EditorCommand::ToggleItalic
            | EditorCommand::ToggleUnderline
            | EditorCommand::ToggleStrike
            | EditorCommand::ToggleInlineCode => self.selected_focused_rich_text().is_some(),
            EditorCommand::SetInlineColor { .. } => self.focused_text_selection_range().is_some(),
            EditorCommand::SetBlockColor { block_id, .. }
            | EditorCommand::InsertParagraphAfterBlock { block_id }
            | EditorCommand::CopyBlockText { block_id } => {
                self.block_payload_record(*block_id).is_some()
            }
            EditorCommand::DeleteBlock { block_id } => self.can_delete_block(*block_id),
            EditorCommand::ToggleTodo { block_id } => {
                matches!(self.block_kind(*block_id), Some(RichBlockKind::Todo { .. }))
            }
            EditorCommand::SetCodeLanguage { block_id, .. } => {
                matches!(self.block_kind(*block_id), Some(RichBlockKind::Code { .. }))
            }
            EditorCommand::MoveBlockBefore {
                block_id,
                before_block_id,
            } => {
                self.has_block(*block_id)
                    && before_block_id.is_none_or(|target| self.has_block(target))
            }
            EditorCommand::MoveBlockToParent {
                block_id,
                parent_id,
                ..
            } => self.has_block(*block_id) && parent_id.is_none_or(|parent| self.has_block(parent)),
            EditorCommand::ApplyAiPreview { .. } => {
                self.ai_session_snapshot().is_some_and(|session| {
                    matches!(session.status, AiSessionStatus::Ready) && !session.preview.is_empty()
                })
            }
            EditorCommand::TransformBlock(BlockTransform::Kind(kind)) => self
                .focused_block_id()
                .is_some_and(|block_id| self.can_convert_block_kind(block_id, kind)),
            EditorCommand::DeleteSelectedBlocks => self.has_selected_blocks(),
            EditorCommand::FoldHeading | EditorCommand::UnfoldHeading => {
                self.focused_block_id().is_some_and(|block_id| {
                    matches!(
                        self.block_kind(block_id),
                        Some(RichBlockKind::Heading { .. })
                    )
                })
            }
            EditorCommand::InsertParagraphAfterFocused
            | EditorCommand::DeleteBackward
            | EditorCommand::DeleteForward
            | EditorCommand::MoveCaret { .. } => self.focused_block_id().is_some(),
            EditorCommand::EnsureTrailingParagraph => self.document_block_count() > 0,
            EditorCommand::InsertSoftLineBreak => self.can_insert_soft_line_break(),
            EditorCommand::HandleEnter => self.can_handle_enter(),
            EditorCommand::IndentBlock => self.can_indent_focused_block(),
            EditorCommand::OutdentBlock => self.can_outdent_focused_block(),
            EditorCommand::ApplySlashBlock {
                block_id,
                trigger_range,
                kind,
            } => self.can_apply_slash_block_kind(*block_id, trigger_range.clone(), kind),
            EditorCommand::TableToggleHeader { block_id, .. } => {
                self.table_dimensions(*block_id).is_some()
            }
            EditorCommand::TableInsertAxis {
                block_id,
                axis,
                index,
                count,
            } => *count > 0 && self.table_axis_insert_is_valid(*block_id, *axis, *index),
            EditorCommand::TableDeleteAxis {
                block_id,
                axis,
                index,
            }
            | EditorCommand::TableDuplicateAxis {
                block_id,
                axis,
                index,
            }
            | EditorCommand::TableResizeAxis {
                block_id,
                axis,
                index,
                ..
            } => self.table_axis_index_is_valid(*block_id, *axis, *index),
            EditorCommand::TableClearRange { block_id, range }
            | EditorCommand::TableSetRangeBackground {
                block_id, range, ..
            }
            | EditorCommand::TableMergeCells { block_id, range }
            | EditorCommand::TableSetRangeAlign {
                block_id, range, ..
            } => self.table_range_is_valid(*block_id, *range),
            EditorCommand::TableSplitCell { block_id, row, col } => {
                self.table_range_is_valid(*block_id, TableRange::normalized(*row, *col, *row, *col))
            }
            EditorCommand::SetMediaWidthRatio { block_id, .. } => {
                matches!(self.block_kind(*block_id), Some(RichBlockKind::Image))
            }
            EditorCommand::UpdateWhiteboardScene { block_id, .. } => {
                matches!(self.block_kind(*block_id), Some(RichBlockKind::Whiteboard))
            }
            EditorCommand::TableMoveAxis {
                block_id,
                axis,
                from_index,
                to_index,
            } => {
                self.table_axis_index_is_valid(*block_id, *axis, *from_index)
                    && self.table_axis_index_is_valid(*block_id, *axis, *to_index)
            }
            EditorCommand::InsertBlock(_)
            | EditorCommand::DuplicateSelectedBlocks
            | EditorCommand::InsertTable { .. }
            | EditorCommand::InsertImage
            | EditorCommand::InsertWhiteboard
            | EditorCommand::InsertMermaid => false,
            _ => false,
        };

        if enabled {
            CommandQueryState::ENABLED.with_check(self.command_check_state(command))
        } else {
            CommandQueryState::disabled(CommandUnavailableReason::InvalidSelection)
        }
    }

    fn has_block(&self, block_id: BlockId) -> bool {
        self.block_kind(block_id).is_some()
    }

    fn table_dimensions(&self, block_id: BlockId) -> Option<(usize, usize)> {
        let record = self.block_payload_record(block_id)?;
        let BlockPayload::Table(table) = &record.payload else {
            return None;
        };
        Some((table.rows.len(), table.columns.len()))
    }

    fn table_axis_index_is_valid(&self, block_id: BlockId, axis: TableAxis, index: usize) -> bool {
        let Some((rows, columns)) = self.table_dimensions(block_id) else {
            return false;
        };
        index
            < match axis {
                TableAxis::Row => rows,
                TableAxis::Column => columns,
            }
    }

    fn table_axis_insert_is_valid(&self, block_id: BlockId, axis: TableAxis, index: usize) -> bool {
        let Some((rows, columns)) = self.table_dimensions(block_id) else {
            return false;
        };
        index
            <= match axis {
                TableAxis::Row => rows,
                TableAxis::Column => columns,
            }
    }

    fn table_range_is_valid(&self, block_id: BlockId, range: TableRange) -> bool {
        let Some((rows, columns)) = self.table_dimensions(block_id) else {
            return false;
        };
        range.start_row <= range.end_row
            && range.start_col <= range.end_col
            && range.end_row < rows
            && range.end_col < columns
    }

    fn command_check_state(&self, command: &EditorCommand) -> CommandCheckState {
        match command {
            EditorCommand::FoldHeading => self.fold_check_state(true),
            EditorCommand::UnfoldHeading => self.fold_check_state(false),
            EditorCommand::ToggleBold => self.inline_mark_check_state(&InlineMark::Bold),
            EditorCommand::ToggleItalic => self.inline_mark_check_state(&InlineMark::Italic),
            EditorCommand::ToggleUnderline => self.inline_mark_check_state(&InlineMark::Underline),
            EditorCommand::ToggleStrike => self.inline_mark_check_state(&InlineMark::Strike),
            EditorCommand::ToggleInlineCode => self.inline_mark_check_state(&InlineMark::Code),
            _ => CommandCheckState::NotCheckable,
        }
    }

    fn fold_check_state(&self, folded: bool) -> CommandCheckState {
        if self
            .focused_block_id()
            .is_some_and(|block_id| self.is_block_folded(block_id) == folded)
        {
            CommandCheckState::Checked
        } else {
            CommandCheckState::Unchecked
        }
    }

    fn inline_mark_check_state(&self, mark: &InlineMark) -> CommandCheckState {
        let Some(selection) = self.selected_focused_rich_text() else {
            return CommandCheckState::Unchecked;
        };
        let mut saw_text = false;
        let mut any = false;
        let mut all = true;
        for span in selection.spans.iter().filter(|span| !span.text.is_empty()) {
            saw_text = true;
            let contains = span.marks.contains(mark);
            any |= contains;
            all &= contains;
        }
        match (saw_text, any, all) {
            (true, true, true) => CommandCheckState::Checked,
            (true, true, false) => CommandCheckState::Mixed,
            _ => CommandCheckState::Unchecked,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_table_query_validates_the_command_index() {
        let runtime = DocumentRuntime::large_mixed_demo();
        let table_id = runtime
            .visible_block_ids()
            .iter()
            .copied()
            .find(|block_id| matches!(runtime.block_kind(*block_id), Some(RichBlockKind::Table)))
            .unwrap();

        assert!(
            runtime
                .query_editor_command(&EditorCommand::TableDeleteAxis {
                    block_id: table_id,
                    axis: TableAxis::Row,
                    index: 0,
                })
                .enabled
        );
        assert!(
            !runtime
                .query_editor_command(&EditorCommand::TableDeleteAxis {
                    block_id: table_id,
                    axis: TableAxis::Row,
                    index: usize::MAX,
                })
                .enabled
        );
    }

    #[test]
    fn typed_target_query_rejects_missing_blocks() {
        let runtime = DocumentRuntime::demo();
        let state = runtime.query_editor_command(&EditorCommand::FocusBlock { block_id: u64::MAX });

        assert!(!state.enabled);
        assert_eq!(
            state.reason,
            Some(CommandUnavailableReason::InvalidSelection)
        );
    }
}
