use cditor_core::ids::BlockId;

use crate::gui::block::table::{
    TableAxis, TableAxisSelection, TableCellRangeSelection, TableCellSelection,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(in crate::gui::app) enum GuiTableInteractionMode {
    #[default]
    Idle,
    EditingCell {
        block_id: BlockId,
        row: usize,
        col: usize,
    },
    SelectingCellText {
        block_id: BlockId,
        row: usize,
        col: usize,
        anchor_offset: usize,
    },
    #[allow(dead_code)]
    SelectingRange(TableCellRangeSelection),
    #[allow(dead_code)]
    RangeSelected(TableCellRangeSelection),
    AxisSelected(TableAxisSelection),
    CellMenu(TableCellSelection),
    #[allow(dead_code)]
    Resizing {
        block_id: BlockId,
        axis: TableAxis,
        index: usize,
    },
    Reordering {
        block_id: BlockId,
        axis: TableAxis,
        from_index: usize,
        target_index: usize,
        active: bool,
    },
    HScrolling {
        block_id: BlockId,
    },
}

impl GuiTableInteractionMode {
    pub(in crate::gui::app) fn block_id(self) -> Option<BlockId> {
        match self {
            Self::Idle => None,
            Self::EditingCell { block_id, .. }
            | Self::SelectingCellText { block_id, .. }
            | Self::Resizing { block_id, .. }
            | Self::Reordering { block_id, .. }
            | Self::HScrolling { block_id } => Some(block_id),
            Self::SelectingRange(selection) | Self::RangeSelected(selection) => {
                Some(selection.block_id)
            }
            Self::AxisSelected(selection) => Some(selection.block_id),
            Self::CellMenu(selection) => Some(selection.block_id),
        }
    }

    pub(in crate::gui::app) fn is_dragging(self) -> bool {
        matches!(
            self,
            Self::SelectingCellText { .. }
                | Self::SelectingRange(_)
                | Self::Resizing { .. }
                | Self::Reordering { active: true, .. }
                | Self::HScrolling { .. }
        )
    }

    pub(in crate::gui::app) fn axis_selection(self) -> Option<TableAxisSelection> {
        match self {
            Self::AxisSelected(selection) => Some(selection),
            _ => None,
        }
    }

    pub(in crate::gui::app) fn visual_axis_selection(self) -> Option<TableAxisSelection> {
        match self {
            Self::AxisSelected(selection) => Some(selection),
            Self::Reordering {
                block_id,
                axis,
                from_index,
                ..
            } => Some(TableAxisSelection::new(block_id, axis, from_index)),
            _ => None,
        }
    }

    pub(in crate::gui::app) fn range_selection(self) -> Option<TableCellRangeSelection> {
        match self {
            Self::SelectingRange(selection) | Self::RangeSelected(selection)
                if selection.is_multi_cell() =>
            {
                Some(selection)
            }
            _ => None,
        }
    }

    pub(in crate::gui::app) fn cell_selection(self) -> Option<TableCellSelection> {
        match self {
            Self::CellMenu(selection) => Some(selection),
            _ => None,
        }
    }

    pub(in crate::gui::app) fn is_menu_open(self) -> bool {
        matches!(
            self,
            Self::AxisSelected(_) | Self::RangeSelected(_) | Self::CellMenu(_)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_interaction_mode_reports_block_id_for_all_table_modes() {
        assert_eq!(
            GuiTableInteractionMode::EditingCell {
                block_id: 7,
                row: 1,
                col: 2,
            }
            .block_id(),
            Some(7)
        );
        assert_eq!(
            GuiTableInteractionMode::AxisSelected(
                TableAxisSelection::new(9, TableAxis::Column, 1,)
            )
            .block_id(),
            Some(9)
        );
        assert_eq!(GuiTableInteractionMode::Idle.block_id(), None);
    }

    #[test]
    fn table_interaction_mode_marks_dragging_modes_only() {
        assert!(GuiTableInteractionMode::HScrolling { block_id: 7 }.is_dragging());
        assert!(
            GuiTableInteractionMode::SelectingRange(TableCellRangeSelection::new(7, 0, 0, 0, 1,))
                .is_dragging()
        );
        assert!(
            GuiTableInteractionMode::SelectingCellText {
                block_id: 7,
                row: 0,
                col: 1,
                anchor_offset: 2,
            }
            .is_dragging()
        );
        assert!(
            !GuiTableInteractionMode::AxisSelected(TableAxisSelection::new(7, TableAxis::Row, 0,))
                .is_dragging()
        );
    }

    #[test]
    fn table_interaction_mode_projects_render_selection() {
        let range = TableCellRangeSelection::new(7, 0, 0, 0, 1);
        assert_eq!(
            GuiTableInteractionMode::RangeSelected(range).range_selection(),
            Some(range)
        );

        let axis = TableAxisSelection::new(7, TableAxis::Row, 0);
        assert_eq!(
            GuiTableInteractionMode::AxisSelected(axis).axis_selection(),
            Some(axis)
        );
        assert!(GuiTableInteractionMode::AxisSelected(axis).is_menu_open());
        assert!(GuiTableInteractionMode::RangeSelected(range).is_menu_open());
        let cell = TableCellSelection::new(7, 1, 2);
        assert_eq!(
            GuiTableInteractionMode::CellMenu(cell).cell_selection(),
            Some(cell)
        );
        assert!(GuiTableInteractionMode::CellMenu(cell).is_menu_open());
        assert!(!GuiTableInteractionMode::Idle.is_menu_open());

        let pressed_axis = GuiTableInteractionMode::Reordering {
            block_id: 7,
            axis: TableAxis::Column,
            from_index: 2,
            target_index: 2,
            active: false,
        };
        assert_eq!(
            pressed_axis.visual_axis_selection(),
            Some(TableAxisSelection::new(7, TableAxis::Column, 2))
        );
        assert_eq!(pressed_axis.axis_selection(), None);
        assert!(!pressed_axis.is_menu_open());
        assert!(!pressed_axis.is_dragging());
    }
}
