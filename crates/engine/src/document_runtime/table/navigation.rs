use super::*;

impl DocumentRuntime {
    pub fn move_focused_table_cell_left(&mut self) -> Result<bool, String> {
        self.move_focused_table_cell_horizontally(false)
    }

    pub fn move_focused_table_cell_right(&mut self) -> Result<bool, String> {
        self.move_focused_table_cell_horizontally(true)
    }

    pub fn move_focused_table_cell_up(&mut self) -> Result<bool, String> {
        self.move_focused_table_cell_vertically(-1)
    }

    pub fn move_focused_table_cell_down(&mut self) -> Result<bool, String> {
        self.move_focused_table_cell_vertically(1)
    }

    pub fn move_focused_table_cell_tab(&mut self, backwards: bool) -> Result<bool, String> {
        let Some(focused) = self.focused_table_cell else {
            return Ok(false);
        };
        let Some((row, col)) = self.adjacent_table_cell_position(
            focused.block_id,
            focused.row,
            focused.col,
            if backwards { -1 } else { 1 },
        ) else {
            return Ok(false);
        };
        let offset = if backwards {
            self.table_cell_plain_text(focused.block_id, row, col)
                .map(|text| text.len())
                .unwrap_or(0)
        } else {
            0
        };
        self.focus_table_cell_at_offset(focused.block_id, row, col, offset)?;
        Ok(true)
    }

    fn move_focused_table_cell_horizontally(&mut self, forward: bool) -> Result<bool, String> {
        let Some(focused) = self.focused_table_cell else {
            return Ok(false);
        };
        let Some(text) = self.table_cell_plain_text(focused.block_id, focused.row, focused.col)
        else {
            return Ok(false);
        };
        let caret = previous_char_boundary(&text, focused.offset.min(text.len()));
        let next = if forward {
            next_grapheme_boundary(&text, caret)
        } else {
            previous_grapheme_boundary(&text, caret)
        };
        if next != caret {
            self.focus_table_cell_at_offset(focused.block_id, focused.row, focused.col, next)?;
            return Ok(true);
        }
        let Some((row, col)) = self.adjacent_table_cell_position(
            focused.block_id,
            focused.row,
            focused.col,
            if forward { 1 } else { -1 },
        ) else {
            return Ok(false);
        };
        let offset = if forward {
            0
        } else {
            self.table_cell_plain_text(focused.block_id, row, col)
                .map(|text| text.len())
                .unwrap_or(0)
        };
        self.focus_table_cell_at_offset(focused.block_id, row, col, offset)?;
        Ok(true)
    }

    fn move_focused_table_cell_vertically(&mut self, delta_row: isize) -> Result<bool, String> {
        let Some(focused) = self.focused_table_cell else {
            return Ok(false);
        };
        let Some(row) = focused.row.checked_add_signed(delta_row) else {
            return Ok(false);
        };
        let Some(text) = self.table_cell_plain_text(focused.block_id, row, focused.col) else {
            return Ok(false);
        };
        self.focus_table_cell_at_offset(
            focused.block_id,
            row,
            focused.col,
            focused.offset.min(text.len()),
        )?;
        Ok(true)
    }

    fn adjacent_table_cell_position(
        &self,
        block_id: BlockId,
        row: usize,
        col: usize,
        delta: isize,
    ) -> Option<(usize, usize)> {
        let table = self.table_runtime(block_id)?.table();
        let mut flat_index = 0usize;
        let mut current = None;
        let mut positions = Vec::new();
        for (row_index, table_row) in table.rows.iter().enumerate() {
            for col_index in 0..table_row.cells.len() {
                if row_index == row && col_index == col {
                    current = Some(flat_index);
                }
                positions.push((row_index, col_index));
                flat_index += 1;
            }
        }
        let current = current?;
        let next = current.checked_add_signed(delta)?;
        positions.get(next).copied()
    }
}
