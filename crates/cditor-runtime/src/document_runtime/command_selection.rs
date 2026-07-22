use cditor_core::{edit::TextAffinity, ids::SurfaceId};

use super::*;

impl DocumentRuntime {
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
}
