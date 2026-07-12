use super::*;

impl DocumentRuntime {
    pub fn focus_block(&mut self, block_id: BlockId) {
        let previous_focus = self.focused_block_id();

        // Get block kind to determine input capability
        let kind = self
            .payload_window
            .get(block_id)
            .map(|payload| payload.kind.clone())
            .unwrap_or(RichBlockKind::Paragraph);

        let input_capability = cditor_core::block::BlockInputCapability::for_kind(&kind);

        let text_len = self
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
        self.focused_text_selection = None;
        self.focused_table_cell = None;

        let mut editing = EditingSession::start(
            block_id,
            self.payload_window
                .get(block_id)
                .map(|payload| payload.content_version)
                .unwrap_or(1),
            CaretAnchor {
                block_id,
                text_offset: text_len as u64,
                caret_rect_y_in_block: 0.0,
                viewport_y: 120.0,
            },
        );

        // Set input target based on block capability
        let input_target = match input_capability {
            cditor_core::block::BlockInputCapability::Text(_) => {
                InputTarget::BlockText { block_id }
            }
            cditor_core::block::BlockInputCapability::TableCell => {
                // For tables, default to block-level focus.
                // Table cell focus is established by focus_table_cell.
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

        editing.set_input_target(input_target);

        // Only set text selection for text-capable blocks
        if input_capability.accepts_text_caret() {
            editing.set_collapsed_selection(text_len);
        } else {
            // No text caret for complex/atomic blocks
            editing.set_collapsed_selection(0);
        }

        self.editing = Some(editing);
    }

    pub fn focused_block_id(&self) -> Option<BlockId> {
        self.editing.as_ref().map(|editing| editing.block_id)
    }

    pub fn first_visible_block_id(&self) -> Option<BlockId> {
        self.visible_index.id_at_visible_index(0)
    }

    pub fn focused_text(&self) -> Option<&str> {
        let block_id = self.focused_block_id()?;
        self.text_models.get(&block_id).map(|model| model.text())
    }

    pub fn focused_text_owned(&self) -> Option<(BlockId, String)> {
        let block_id = self.focused_block_id()?;
        let text = self.text_models.get(&block_id)?.text().to_owned();
        Some((block_id, text))
    }

    pub fn caret_offset_for_block(&self, block_id: BlockId) -> Option<usize> {
        self.editing
            .as_ref()
            .filter(|editing| editing.block_id == block_id)
            .map(|editing| editing.caret_anchor.text_offset as usize)
    }

    pub fn focus_block_at_offset(
        &mut self,
        block_id: BlockId,
        offset: usize,
    ) -> Result<(), String> {
        self.set_caret_offset(block_id, offset)
    }

    pub fn focus_table_cell(
        &mut self,
        block_id: BlockId,
        row: usize,
        col: usize,
    ) -> Result<(), String> {
        let payload_content_version = self
            .payload_window
            .get(block_id)
            .map(|payload| payload.content_version)
            .ok_or_else(|| format!("missing payload for block {block_id}"))?;
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
        let text_len = cditor_core::rich_text::plain_text_from_spans(&cell.spans).len();
        trace_table(
            "focus_table_cell",
            format_args!(
                "block={block_id} row={row} col={col} text_len={text_len} rows={} cols={} content_version={}",
                table.rows.len(),
                table.rows.get(row).map(|row| row.cells.len()).unwrap_or(0),
                payload_content_version
            ),
        );
        self.selected_block_ids.clear();
        self.document_selection = None;
        self.focused_text_selection = None;
        self.focused_table_cell = Some(FocusedTableCell::collapsed(block_id, row, col, text_len));
        let mut editing = EditingSession::start(
            block_id,
            payload_content_version,
            CaretAnchor {
                block_id,
                text_offset: text_len as u64,
                caret_rect_y_in_block: 0.0,
                viewport_y: 120.0,
            },
        );
        editing.set_input_target(InputTarget::TableCell { block_id, row, col });
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

    pub fn focused_table_cell_offset(&self) -> Option<(BlockId, usize, usize, usize)> {
        self.focused_table_cell
            .map(|cell| (cell.block_id, cell.row, cell.col, cell.offset))
    }

    pub fn focused_table_cell_selection_state(
        &self,
    ) -> Option<(
        BlockId,
        usize,
        usize,
        Range<usize>,
        bool,
        Option<Range<usize>>,
    )> {
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

    pub fn blur_table_cell(&mut self) -> bool {
        let Some(focused) = self.focused_table_cell.take() else {
            return false;
        };
        if let Some(editing) = self.editing.as_mut()
            && editing.block_id == focused.block_id
        {
            editing.set_input_target(InputTarget::BlockText {
                block_id: focused.block_id,
            });
            editing.set_collapsed_selection(0);
            editing.clear_composition();
        }
        true
    }

    pub fn focus_table_cell_at_offset(
        &mut self,
        block_id: BlockId,
        row: usize,
        col: usize,
        offset: usize,
    ) -> Result<(), String> {
        self.focus_table_cell(block_id, row, col)?;
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

    pub fn set_caret_offset(&mut self, block_id: BlockId, offset: usize) -> Result<(), String> {
        if self.focused_block_id() != Some(block_id) {
            self.focus_block(block_id);
        }
        let model = self
            .text_models
            .get(&block_id)
            .ok_or_else(|| format!("missing text model for block {block_id}"))?;
        let offset = normalized_grapheme_offset(model.text(), offset);
        let previous_caret = self.caret_offset_for_block(block_id);
        let editing = self.editing.as_mut().expect("editing session exists");
        editing.set_input_target(InputTarget::BlockText { block_id });
        editing.set_collapsed_selection(offset);
        self.document_selection = None;
        self.focused_text_selection = None;
        self.focused_table_cell = None;
        trace_input(
            "set_caret_offset",
            format_args!(
                "block={block_id} requested_offset={} clamped_offset={offset} previous_caret={previous_caret:?} text_len={}",
                offset,
                model.len()
            ),
        );
        Ok(())
    }
}
