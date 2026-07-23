use super::*;

/// (block, row, col, selected range, reversed, marked range)
pub type TableCellSelectionState = (
    BlockId,
    usize,
    usize,
    Range<usize>,
    bool,
    Option<Range<usize>>,
);

impl DocumentRuntime {
    pub(crate) fn focus_block(&mut self, block_id: BlockId) {
        if let Err(error) = self.try_focus_block(block_id) {
            trace_input(
                "focus_block.rejected",
                format_args!("block={block_id} error={error}"),
            );
        }
    }

    pub(crate) fn try_focus_block(&mut self, block_id: BlockId) -> Result<(), String> {
        self.break_typing_coalescing();
        let previous_focus = self.focused_block_id();
        self.hydrate_payload_runtime_state(block_id);

        // Get block kind to determine input capability
        let kind = self
            .document
            .payload_window
            .get(block_id)
            .map(|payload| payload.kind.clone())
            .unwrap_or(RichBlockKind::Paragraph);

        let input_capability = cditor_core::block::BlockInputCapability::for_kind(&kind);
        let input_target = match input_capability {
            cditor_core::block::BlockInputCapability::Text(_) => {
                InputTarget::BlockText { block_id }
            }
            cditor_core::block::BlockInputCapability::TableCell => {
                InputTarget::BlockChrome { block_id }
            }
            cditor_core::block::BlockInputCapability::ComplexBlock => {
                InputTarget::ComplexBlock { block_id }
            }
            cditor_core::block::BlockInputCapability::Atomic
            | cditor_core::block::BlockInputCapability::None => {
                InputTarget::BlockChrome { block_id }
            }
        };
        if self.prepare_input_focus_transition(input_target)?
            == CompositionFocusTransition::PreservedSameSurface
        {
            return Ok(());
        }

        let text_len = self
            .document
            .text_models
            .get(&block_id)
            .map(PieceTableTextModel::len)
            .unwrap_or(0);

        trace_input(
            "focus_block",
            format_args!(
                "previous_focus={previous_focus:?} next_block={block_id} kind={kind:?} capability={input_capability:?} caret_to_text_len={text_len}"
            ),
        );

        self.selected_block_ids.clear();
        self.document_selection = None;
        self.visual_caret_position = None;
        self.focused_text_selection = None;
        self.focused_table_cell = None;
        self.focused_inner_selection = None;

        let session_id = self.allocate_input_session_id();
        let mut editing = EditingSession::start_with_session_id(
            block_id,
            self.document
                .payload_window
                .get(block_id)
                .map(|payload| payload.content_version)
                .unwrap_or(1),
            text_len,
            CaretAnchor {
                block_id,
                caret_rect_y_in_block: 0.0,
                viewport_y: 120.0,
            },
            session_id,
        );

        editing.set_input_target(input_target);

        // Only set text selection for text-capable blocks
        if input_capability.accepts_text_caret() {
            editing.set_collapsed_selection(text_len);
        } else {
            // No text caret for complex/atomic blocks
            editing.set_collapsed_selection(0);
        }

        self.editing = Some(editing);
        Ok(())
    }

    pub fn focused_block_id(&self) -> Option<BlockId> {
        self.editing.as_ref().map(|editing| editing.block_id)
    }

    pub fn first_visible_block_id(&self) -> Option<BlockId> {
        self.document.visible_index.id_at_visible_index(0)
    }

    pub fn focused_text(&self) -> Option<&str> {
        let block_id = self.focused_block_id()?;
        self.document
            .text_models
            .get(&block_id)
            .map(|model| model.text())
    }

    pub fn focused_text_owned(&self) -> Option<(BlockId, String)> {
        let block_id = self.focused_block_id()?;
        let text = self.document.text_models.get(&block_id)?.text().to_owned();
        Some((block_id, text))
    }

    pub fn caret_offset_for_block(&self, block_id: BlockId) -> Option<usize> {
        self.editing
            .as_ref()
            .filter(|editing| editing.block_id == block_id)
            .map(EditingSession::focus_offset)
    }

    pub fn caret_position_for_block(&self, block_id: BlockId) -> Option<TextPosition> {
        let offset = self.caret_offset_for_block(block_id)?;
        let content_version = self.block_content_version(block_id)?;
        Some(
            self.visual_caret_position
                .filter(|state| {
                    state.position.block_id == block_id
                        && state.position.offset == offset
                        && state.content_version == content_version
                })
                .map(|state| state.position)
                .unwrap_or_else(|| TextPosition::downstream(block_id, offset)),
        )
    }

    pub(super) fn remember_visual_caret(&mut self, position: TextPosition) {
        self.visual_caret_position =
            self.block_content_version(position.block_id)
                .map(|content_version| VisualCaretPosition {
                    position,
                    content_version,
                });
    }

    pub(crate) fn focus_block_at_offset(
        &mut self,
        block_id: BlockId,
        offset: usize,
    ) -> Result<(), String> {
        self.set_caret_offset(block_id, offset)
    }

    pub(crate) fn focus_table_cell(
        &mut self,
        block_id: BlockId,
        row: usize,
        col: usize,
    ) -> Result<(), String> {
        self.break_typing_coalescing();
        self.hydrate_payload_runtime_state(block_id);
        let payload_content_version = self
            .document
            .payload_window
            .get(block_id)
            .map(|payload| payload.content_version)
            .ok_or_else(|| format!("missing payload for block {block_id}"))?;
        let (row, col, text_len, row_count, col_count) = {
            let table = self
                .table_runtime(block_id)
                .ok_or_else(|| format!("missing table runtime for block {block_id}"))?
                .table();
            let (row, col) = table
                .cell_origin(row, col)
                .ok_or_else(|| format!("missing table cell {row}:{col} in block {block_id}"))?;
            let cell = table
                .rows
                .get(row)
                .and_then(|row| row.cells.get(col))
                .ok_or_else(|| format!("missing table cell {row}:{col} in block {block_id}"))?;
            (
                row,
                col,
                cditor_core::rich_text::plain_text_from_spans(&cell.spans).len(),
                table.rows.len(),
                table.rows.get(row).map(|row| row.cells.len()).unwrap_or(0),
            )
        };
        let input_target = InputTarget::TableCell { block_id, row, col };
        if self.prepare_input_focus_transition(input_target)?
            == CompositionFocusTransition::PreservedSameSurface
        {
            return Ok(());
        }
        trace_table(
            "focus_table_cell",
            format_args!(
                "block={block_id} row={row} col={col} text_len={text_len} rows={} cols={} content_version={}",
                row_count, col_count, payload_content_version
            ),
        );
        self.selected_block_ids.clear();
        self.document_selection = None;
        self.visual_caret_position = None;
        self.focused_text_selection = None;
        self.focused_table_cell = Some(FocusedTableCell::collapsed(block_id, row, col, text_len));
        self.focused_inner_selection = None;
        let session_id = self.allocate_input_session_id();
        let mut editing = EditingSession::start_with_session_id(
            block_id,
            payload_content_version,
            text_len,
            CaretAnchor {
                block_id,
                caret_rect_y_in_block: 0.0,
                viewport_y: 120.0,
            },
            session_id,
        );
        editing.set_input_target(input_target);
        editing.set_collapsed_selection(text_len);
        self.editing = Some(editing);
        Ok(())
    }

    pub fn focused_table_cell_for_block(&self, block_id: BlockId) -> Option<TableCellPosition> {
        let focused = self.focused_table_cell?;
        (focused.block_id == block_id).then_some(TableCellPosition {
            row: focused.row,
            col: focused.col,
        })
    }

    pub(super) fn allocate_input_session_id(&mut self) -> u64 {
        let session_id = self.next_input_session_id;
        self.next_input_session_id = self.next_input_session_id.saturating_add(1);
        session_id
    }

    pub fn focused_table_cell_offset(&self) -> Option<(BlockId, usize, usize, usize)> {
        self.focused_table_cell
            .map(|cell| (cell.block_id, cell.row, cell.col, cell.offset))
    }

    pub fn focused_table_cell_text_position(
        &self,
    ) -> Option<(BlockId, usize, usize, usize, TextAffinity)> {
        self.focused_table_cell.map(|cell| {
            (
                cell.block_id,
                cell.row,
                cell.col,
                cell.offset,
                cell.affinity,
            )
        })
    }

    pub fn focused_table_cell_selection_state(&self) -> Option<TableCellSelectionState> {
        self.focused_table_cell.map(|cell| {
            (
                cell.block_id,
                cell.row,
                cell.col,
                cell.selected_range(),
                cell.selection_reversed,
                cell.marked_range(),
            )
        })
    }

    #[cfg(test)]
    pub(crate) fn blur_table_cell(&mut self) -> bool {
        match self.try_blur_table_cell() {
            Ok(blurred) => blurred,
            Err(error) => {
                trace_input("blur_table_cell.rejected", error);
                false
            }
        }
    }

    pub(crate) fn try_blur_table_cell(&mut self) -> Result<bool, String> {
        self.break_typing_coalescing();
        let Some(focused) = self.focused_table_cell else {
            return Ok(false);
        };
        let next_target = InputTarget::BlockChrome {
            block_id: focused.block_id,
        };
        self.prepare_input_focus_transition(next_target)?;
        let Some(focused) = self.focused_table_cell.take() else {
            return Ok(false);
        };
        if let Some(editing) = self.editing.as_mut()
            && editing.block_id == focused.block_id
        {
            editing.set_input_target(next_target);
            editing.set_collapsed_selection(0);
        }
        Ok(true)
    }

    pub(crate) fn focus_table_cell_at_offset(
        &mut self,
        block_id: BlockId,
        row: usize,
        col: usize,
        offset: usize,
    ) -> Result<(), String> {
        self.focus_table_cell(block_id, row, col)?;
        if self.active_composition().is_some() {
            return Ok(());
        }
        let Some(focused) = self.focused_table_cell else {
            return Ok(());
        };
        let row = focused.row;
        let col = focused.col;
        let Some(text) = self.table_cell_plain_text(block_id, row, col) else {
            return Ok(());
        };
        let offset = normalized_grapheme_offset(&text, offset);
        if let Some(cell) = self.focused_table_cell.as_mut()
            && cell.block_id == block_id
            && cell.row == row
            && cell.col == col
        {
            *cell = cell
                .with_selected_range(offset..offset, false)
                .with_marked_range(None);
        }
        if let Some(editing) = self.editing.as_mut() {
            editing.set_input_target(InputTarget::TableCell { block_id, row, col });
            editing.set_collapsed_selection(offset);
        }
        trace_table(
            "focus_table_cell_at_offset",
            format_args!("block={block_id} row={row} col={col} offset={offset}"),
        );
        Ok(())
    }

    pub(crate) fn set_focused_table_cell_text_selection(
        &mut self,
        anchor_offset: usize,
        focus_offset: usize,
    ) -> Result<bool, String> {
        self.break_typing_coalescing();
        let Some(focused) = self.focused_table_cell else {
            return Ok(false);
        };
        let text = self
            .table_cell_plain_text(focused.block_id, focused.row, focused.col)
            .ok_or_else(|| {
                format!(
                    "missing table cell {}:{} in block {}",
                    focused.row, focused.col, focused.block_id
                )
            })?;
        let anchor_offset = normalized_grapheme_offset(&text, anchor_offset);
        let focus_offset = normalized_grapheme_offset(&text, focus_offset);
        let selected_range = anchor_offset.min(focus_offset)..anchor_offset.max(focus_offset);
        let selection_reversed = focus_offset < anchor_offset;
        let changed = focused.selected_range() != selected_range
            || focused.selection_reversed != selection_reversed
            || focused.offset != focus_offset;

        if let Some(cell) = self.focused_table_cell.as_mut() {
            *cell = cell
                .with_selected_range(selected_range.clone(), selection_reversed)
                .with_marked_range(None);
        }
        if let Some(editing) = self.editing.as_mut() {
            editing.set_input_target(InputTarget::TableCell {
                block_id: focused.block_id,
                row: focused.row,
                col: focused.col,
            });
            if selected_range.is_empty() {
                editing.set_collapsed_selection(focus_offset);
            } else {
                editing.set_selected_range(selected_range, selection_reversed);
            }
            editing.clear_composition();
        }
        Ok(changed)
    }

    pub(crate) fn set_focused_table_cell_text_selection_position(
        &mut self,
        anchor_offset: usize,
        focus_offset: usize,
        focus_affinity: TextAffinity,
    ) -> Result<bool, String> {
        let changed = self.set_focused_table_cell_text_selection(anchor_offset, focus_offset)?;
        if let Some(cell) = self.focused_table_cell.as_mut() {
            *cell = cell.with_affinity(focus_affinity);
        }
        Ok(changed)
    }

    pub(crate) fn set_caret_offset(
        &mut self,
        block_id: BlockId,
        offset: usize,
    ) -> Result<(), String> {
        let input_target = InputTarget::BlockText { block_id };
        if self
            .editing
            .as_ref()
            .is_some_and(|editing| editing.input_target == input_target)
            && self.prepare_input_focus_transition(input_target)?
                == CompositionFocusTransition::PreservedSameSurface
        {
            return Ok(());
        }
        self.break_typing_coalescing();
        if self
            .editing
            .as_ref()
            .is_none_or(|editing| editing.input_target != input_target)
        {
            self.try_focus_block(block_id)?;
        }
        let (offset, text_len) = {
            let model = self
                .document
                .text_models
                .get(&block_id)
                .ok_or_else(|| format!("missing text model for block {block_id}"))?;
            (
                normalized_grapheme_offset(model.text(), offset),
                model.len(),
            )
        };
        let previous_caret = self.caret_offset_for_block(block_id);
        let editing = self.editing.as_mut().expect("editing session exists");
        editing.set_input_target(InputTarget::BlockText { block_id });
        editing.set_collapsed_selection(offset);
        self.document_selection = None;
        self.remember_visual_caret(TextPosition::downstream(block_id, offset));
        self.focused_text_selection = None;
        self.focused_table_cell = None;
        self.focused_inner_selection = None;
        trace_input(
            "set_caret_offset",
            format_args!(
                "block={block_id} requested_offset={} clamped_offset={offset} previous_caret={previous_caret:?} text_len={}",
                offset, text_len
            ),
        );
        Ok(())
    }
}
