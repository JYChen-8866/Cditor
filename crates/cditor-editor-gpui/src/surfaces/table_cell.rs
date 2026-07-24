use cditor_core::ids::{BlockId, SurfaceId};
use cditor_runtime::TableCellPosition;
use cditor_session::SurfaceVersionSnapshot;
use gpui::{Pixels, Point};

use crate::editor_view::CditorV2View;
use crate::text::{
    ParleyTextPosition, RichTextPlatformLayout, TextLayoutSurfaceId,
    platform_text_position_for_point, record_unavailable_geometry,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct TableCellLayoutKey {
    pub(crate) block_id: BlockId,
    pub(crate) row: usize,
    pub(crate) col: usize,
}

pub(crate) const fn surface_id(block_id: BlockId, position: TableCellPosition) -> SurfaceId {
    SurfaceId::TableCell {
        block_id,
        row: position.row,
        column: position.col,
    }
}

pub(crate) const fn layout_surface_id(
    block_id: BlockId,
    position: TableCellPosition,
) -> TextLayoutSurfaceId {
    TextLayoutSurfaceId::TableCell {
        block_id,
        row: position.row,
        column: position.col,
    }
}

impl CditorV2View {
    pub(crate) fn current_table_cell_layout_cache(
        &self,
        current: SurfaceVersionSnapshot,
        block_id: BlockId,
        row: usize,
        col: usize,
    ) -> Option<&RichTextPlatformLayout> {
        let expected = surface_id(block_id, TableCellPosition { row, col });
        if current.surface_id != expected {
            return None;
        }
        let cache =
            self.cache
                .table_cell_layouts
                .get(&TableCellLayoutKey { block_id, row, col })?;
        super::text::layout_cache_is_current(cache, current).then_some(cache)
    }

    pub(crate) fn text_position_for_table_cell_at_position(
        &self,
        block_id: BlockId,
        row: usize,
        col: usize,
        position: Point<Pixels>,
    ) -> Option<ParleyTextPosition> {
        let session = self.ready_session()?;
        let surface_id = surface_id(block_id, TableCellPosition { row, col });
        let current = session.surface_version(surface_id).ok().flatten()?;
        let Some(cache) = self.current_table_cell_layout_cache(current, block_id, row, col) else {
            record_unavailable_geometry();
            return None;
        };
        Some(platform_text_position_for_point(cache, position))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_cell_surface_id_preserves_row_and_column() {
        let position = TableCellPosition { row: 2, col: 3 };
        assert_eq!(
            surface_id(7, position),
            SurfaceId::TableCell {
                block_id: 7,
                row: 2,
                column: 3,
            }
        );
        assert_eq!(
            layout_surface_id(7, position),
            TextLayoutSurfaceId::TableCell {
                block_id: 7,
                row: 2,
                column: 3,
            }
        );
    }
}
