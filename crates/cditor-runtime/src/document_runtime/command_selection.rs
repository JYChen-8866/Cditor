use cditor_core::{edit::TextAffinity, ids::SurfaceId};

use super::*;

pub(super) struct SelectionCommandResult {
    pub changed: bool,
    pub affected_blocks: Vec<BlockId>,
}

impl DocumentRuntime {
    pub(super) fn dispatch_selection_command(
        &mut self,
        command: &cditor_editor_protocol::command::EditorCommand,
    ) -> Result<Option<SelectionCommandResult>, String> {
        use cditor_editor_protocol::command::EditorCommand;

        let (changed, affected_blocks) = match command {
            EditorCommand::SelectAll => (self.select_all_command(), Vec::new()),
            EditorCommand::SetDocumentSelection { selection } => {
                (self.set_document_selection(*selection)?, Vec::new())
            }
            EditorCommand::SetBlockSelectionRange {
                anchor_block_id,
                focus_block_id,
            } => (
                self.select_visible_block_range(*anchor_block_id, *focus_block_id),
                vec![*anchor_block_id, *focus_block_id],
            ),
            EditorCommand::FocusBlock { block_id } => {
                (self.focus_block_command(*block_id)?, vec![*block_id])
            }
            EditorCommand::FocusTableCell {
                block_id,
                row,
                col,
                offset,
                affinity,
            } => (
                self.focus_table_cell_command(*block_id, *row, *col, *offset, *affinity)?,
                vec![*block_id],
            ),
            EditorCommand::BlurTableCell => (self.try_blur_table_cell()?, Vec::new()),
            EditorCommand::SetTableCellSelection {
                block_id,
                row,
                col,
                anchor_offset,
                focus_offset,
                focus_affinity,
            } => (
                self.set_table_cell_selection_command(
                    *block_id,
                    *row,
                    *col,
                    *anchor_offset,
                    *focus_offset,
                    *focus_affinity,
                )?,
                vec![*block_id],
            ),
            EditorCommand::NavigateTableCell {
                direction,
                extend_selection,
            } => {
                let affected_blocks = self.focused_block_id().into_iter().collect();
                (
                    self.navigate_table_cell_command(*direction, *extend_selection)?,
                    affected_blocks,
                )
            }
            EditorCommand::MoveCaret {
                direction,
                extend_selection,
            } => {
                let affected_blocks = self.focused_block_id().into_iter().collect();
                (
                    self.move_caret_command(*direction, *extend_selection)?,
                    affected_blocks,
                )
            }
            EditorCommand::SetTextSurfaceSelection {
                surface_id,
                anchor_offset,
                focus_offset,
                focus_affinity,
            } => (
                self.set_auxiliary_text_surface_selection(
                    *surface_id,
                    *anchor_offset,
                    *focus_offset,
                    *focus_affinity,
                )?,
                surface_id.block_id().into_iter().collect(),
            ),
            _ => return Ok(None),
        };
        Ok(Some(SelectionCommandResult {
            changed,
            affected_blocks,
        }))
    }

    pub(super) fn focus_block_command(&mut self, block_id: BlockId) -> Result<bool, String> {
        let before = self.focused_block_id();
        self.try_focus_block(block_id)?;
        Ok(before != self.focused_block_id())
    }

    pub(super) fn focus_table_cell_command(
        &mut self,
        block_id: BlockId,
        row: usize,
        col: usize,
        offset: Option<usize>,
        affinity: TextAffinity,
    ) -> Result<bool, String> {
        let before = self.focused_table_cell_text_position();
        if let Some(offset) = offset {
            self.focus_table_cell_at_offset(block_id, row, col, offset)?;
            self.move_focused_table_cell_to_text_position(offset, affinity, false)?;
        } else {
            self.focus_table_cell(block_id, row, col)?;
        }
        Ok(before != self.focused_table_cell_text_position())
    }

    pub(super) fn set_auxiliary_text_surface_selection(
        &mut self,
        surface_id: SurfaceId,
        anchor_offset: usize,
        focus_offset: usize,
        focus_affinity: TextAffinity,
    ) -> Result<bool, String> {
        if !matches!(
            surface_id,
            SurfaceId::ImageCaption { .. } | SurfaceId::CollectionTitle { .. }
        ) {
            return Err(format!(
                "surface {surface_id:?} is not an auxiliary text surface"
            ));
        }
        let before_surface = self.focused_text_surface_id();
        let before_range = self.text_surface_selection_range(surface_id);
        let before_caret = self.text_surface_caret_offset(surface_id);

        self.focus_text_surface_at_offset(surface_id, anchor_offset)?;
        let extend_selection = anchor_offset != focus_offset;
        if extend_selection || focus_affinity != TextAffinity::Downstream {
            self.move_focused_text_surface_to_offset(
                surface_id,
                focus_offset,
                focus_affinity,
                extend_selection,
            )?;
        }

        Ok(before_surface != self.focused_text_surface_id()
            || before_range != self.text_surface_selection_range(surface_id)
            || before_caret != self.text_surface_caret_offset(surface_id))
    }

    pub(super) fn set_table_cell_selection_command(
        &mut self,
        block_id: BlockId,
        row: usize,
        col: usize,
        anchor_offset: usize,
        focus_offset: usize,
        focus_affinity: TextAffinity,
    ) -> Result<bool, String> {
        let focused = self.focused_table_cell_offset();
        if !focused.is_some_and(|(focused_block, focused_row, focused_col, _)| {
            (focused_block, focused_row, focused_col) == (block_id, row, col)
        }) {
            self.focus_table_cell_at_offset(block_id, row, col, anchor_offset)?;
        }
        self.set_focused_table_cell_text_selection_position(
            anchor_offset,
            focus_offset,
            focus_affinity,
        )
    }

    fn navigate_table_cell_command(
        &mut self,
        direction: cditor_editor_protocol::command::TableCellNavigationDirection,
        extend_selection: bool,
    ) -> Result<bool, String> {
        use cditor_editor_protocol::command::TableCellNavigationDirection as Direction;

        match (direction, extend_selection) {
            (Direction::Left, false) => self.move_focused_table_cell_left(),
            (Direction::Left, true) => self.extend_focused_table_cell_selection_left(),
            (Direction::Right, false) => self.move_focused_table_cell_right(),
            (Direction::Right, true) => self.extend_focused_table_cell_selection_right(),
            (Direction::Up, false) => self.move_focused_table_cell_up(),
            (Direction::Down, false) => self.move_focused_table_cell_down(),
            (Direction::TabForward, false) => self.move_focused_table_cell_tab(false),
            (Direction::TabBackward, false) => self.move_focused_table_cell_tab(true),
            (Direction::Up | Direction::Down, true)
            | (Direction::TabForward | Direction::TabBackward, true) => Ok(false),
        }
    }

    fn move_caret_command(
        &mut self,
        direction: cditor_editor_protocol::command::CaretDirection,
        extend_selection: bool,
    ) -> Result<bool, String> {
        use cditor_editor_protocol::command::CaretDirection as Direction;

        match direction {
            Direction::PreviousVisual => self.move_caret_left(extend_selection),
            Direction::NextVisual => self.move_caret_right(extend_selection),
            Direction::PreviousWord => self.move_focused_caret_by_word(false, extend_selection),
            Direction::NextWord => self.move_focused_caret_by_word(true, extend_selection),
            Direction::PreviousLine => self.move_caret_up(extend_selection),
            Direction::NextLine => self.move_caret_down(extend_selection),
            Direction::LineStart => {
                self.move_focused_caret_to_line_boundary(false, extend_selection)
            }
            Direction::LineEnd => self.move_focused_caret_to_line_boundary(true, extend_selection),
            Direction::DocumentStart => {
                self.move_caret_to_document_boundary(false, extend_selection)
            }
            Direction::DocumentEnd => self.move_caret_to_document_boundary(true, extend_selection),
        }
    }
}
