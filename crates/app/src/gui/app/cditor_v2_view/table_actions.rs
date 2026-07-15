use cditor_core::ids::BlockId;
use cditor_core::rich_text::TableRange;
use gpui::{Context, Pixels, Point, Window};

use crate::gui::app::cditor_v2_view::{CditorV2View, CditorViewState};
use crate::gui::app::interaction::table_mode::GuiTableInteractionMode;
use crate::gui::block::table::menu::{
    TableBackgroundColor, TableMenuAction, filter_table_menu_items, table_axis_menu_items,
};
use crate::gui::block::table::{
    TableAxis, TableAxisSelection, TableCellRangeSelection, TableCellSelection,
};

impl CditorV2View {
    pub(crate) fn dismiss_table_menu_from_gui(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.table_interaction_mode.is_menu_open() {
            return false;
        }
        self.table_interaction_mode = self
            .table_interaction_mode
            .cell_selection()
            .map(|selection| GuiTableInteractionMode::EditingCell {
                block_id: selection.block_id,
                row: selection.row,
                col: selection.col,
            })
            .unwrap_or(GuiTableInteractionMode::Idle);
        self.table_menu_ui = Default::default();
        cx.notify();
        true
    }

    pub(in crate::gui::app) fn projected_table_axis_selection(&self) -> Option<TableAxisSelection> {
        self.table_interaction_mode.axis_selection()
    }

    pub(in crate::gui::app) fn projected_table_axis_visual_selection(
        &self,
    ) -> Option<TableAxisSelection> {
        self.table_interaction_mode.visual_axis_selection()
    }

    pub(in crate::gui::app) fn projected_table_range_selection(
        &self,
    ) -> Option<TableCellRangeSelection> {
        self.table_interaction_mode.range_selection()
    }

    pub(in crate::gui::app) fn projected_table_cell_selection(&self) -> Option<TableCellSelection> {
        self.table_interaction_mode.cell_selection()
    }

    pub(crate) fn open_table_cell_menu_from_gui(
        &mut self,
        block_id: BlockId,
        row: usize,
        col: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus, cx);
        let already_focused = self
            .ready_runtime_ref()
            .and_then(|runtime| runtime.focused_table_cell_offset())
            .is_some_and(|(focused_block_id, focused_row, focused_col, _)| {
                (focused_block_id, focused_row, focused_col) == (block_id, row, col)
            });
        if !already_focused && let Some(runtime) = self.ready_runtime() {
            let _ = runtime.focus_table_cell(block_id, row, col);
        }
        self.table_interaction_mode =
            GuiTableInteractionMode::CellMenu(TableCellSelection::new(block_id, row, col));
        self.table_menu_ui = Default::default();
        cx.notify();
    }

    pub(crate) fn focus_table_cell_from_gui(
        &mut self,
        block_id: BlockId,
        row: usize,
        col: usize,
        position: Option<Point<Pixels>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus, cx);
        self.text_drag_selection = None;
        self.table_interaction_mode = GuiTableInteractionMode::EditingCell { block_id, row, col };
        let offset = position.and_then(|position| {
            self.text_offset_for_table_cell_at_position(block_id, row, col, position)
        });
        super::trace_table(
            "focus_cell.gui.begin",
            format_args!(
                "block={block_id} row={row} col={col} position={position:?} resolved_offset={offset:?}"
            ),
        );
        if let CditorViewState::Ready(runtime) = &mut self.state {
            if let Some(offset) = offset {
                let _ = runtime.focus_table_cell_at_offset(block_id, row, col, offset);
            } else {
                let _ = runtime.focus_table_cell(block_id, row, col);
            }
            let payload_state = runtime
                .block_payload_record(block_id)
                .map(|payload| match &payload.payload {
                    cditor_core::rich_text::BlockPayload::Table(table) => format!(
                        "table rows={} cols={} content_version={}",
                        table.rows.len(),
                        table.rows.first().map(|row| row.cells.len()).unwrap_or(0),
                        payload.content_version
                    ),
                    other => format!("non_table payload={other:?}"),
                })
                .unwrap_or_else(|| "missing_payload".to_owned());
            super::trace_table(
                "focus_cell.gui.end",
                format_args!(
                    "block={block_id} row={row} col={col} focused_block={:?} focused_cell={:?} focused_cell_offset={:?} payload={payload_state}",
                    runtime.focused_block_id(),
                    runtime.focused_table_cell_for_block(block_id),
                    runtime.focused_table_cell_offset()
                ),
            );
        }
        cx.notify();
    }

    pub(crate) fn begin_table_cell_text_selection_from_gui(
        &mut self,
        block_id: BlockId,
        row: usize,
        col: usize,
        position: Option<Point<Pixels>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_table_cell_from_gui(block_id, row, col, position, window, cx);
        let anchor_offset = self
            .ready_runtime_ref()
            .and_then(|runtime| runtime.focused_table_cell_offset())
            .filter(|(focused_block_id, focused_row, focused_col, _)| {
                (*focused_block_id, *focused_row, *focused_col) == (block_id, row, col)
            })
            .map(|(_, _, _, offset)| offset)
            .unwrap_or(0);
        self.table_interaction_mode = GuiTableInteractionMode::SelectingCellText {
            block_id,
            row,
            col,
            anchor_offset,
        };
    }

    pub(crate) fn update_table_cell_text_selection_from_gui(
        &mut self,
        block_id: BlockId,
        row: usize,
        col: usize,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let GuiTableInteractionMode::SelectingCellText {
            block_id: anchor_block_id,
            row: anchor_row,
            col: anchor_col,
            anchor_offset,
        } = self.table_interaction_mode
        else {
            return;
        };
        if (anchor_block_id, anchor_row, anchor_col) != (block_id, row, col) {
            return;
        }
        let Some(focus_offset) =
            self.text_offset_for_table_cell_at_position(block_id, row, col, position)
        else {
            return;
        };
        let changed = self
            .ready_runtime()
            .and_then(|runtime| {
                runtime
                    .set_focused_table_cell_text_selection(anchor_offset, focus_offset)
                    .ok()
            })
            .unwrap_or(false);
        if changed {
            cx.notify();
        }
    }

    pub(in crate::gui::app) fn finish_table_cell_text_selection_drag(&mut self) {
        if let GuiTableInteractionMode::SelectingCellText {
            block_id, row, col, ..
        } = self.table_interaction_mode
        {
            self.table_interaction_mode =
                GuiTableInteractionMode::EditingCell { block_id, row, col };
        }
    }

    pub(crate) fn confirm_table_menu_from_gui(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(selection) = self.table_interaction_mode.axis_selection() else {
            return false;
        };
        let Some(action) =
            filter_table_menu_items(&table_axis_menu_items(selection), &self.table_menu_ui.query)
                .first()
                .map(|item| item.action)
        else {
            return true;
        };
        let _ = self.apply_selected_table_menu_action_from_gui(action, cx);
        true
    }

    pub(crate) fn duplicate_selected_table_axis_from_gui(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(selection) = self.table_interaction_mode.axis_selection() else {
            return false;
        };
        let action = match selection.axis {
            TableAxis::Row => TableMenuAction::DuplicateRow,
            TableAxis::Column => TableMenuAction::DuplicateColumn,
        };
        let _ = self.apply_selected_table_menu_action_from_gui(action, cx);
        true
    }

    pub(crate) fn delete_table_menu_query_backward_from_gui(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.table_interaction_mode.axis_selection().is_none() {
            return false;
        }
        self.table_menu_ui.delete_backward();
        cx.notify();
        true
    }

    pub(crate) fn set_table_background_submenu_open_from_gui(
        &mut self,
        open: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.selected_table_range().is_none() {
            return false;
        }
        if self.table_menu_ui.color_submenu_open == open {
            return false;
        }
        self.table_menu_ui.color_submenu_open = open;
        cx.notify();
        true
    }

    pub(crate) fn set_selected_table_background_from_gui(
        &mut self,
        color: TableBackgroundColor,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.readonly {
            return false;
        }
        let Some((block_id, range)) = self.selected_table_range() else {
            return false;
        };
        let changed = self
            .ready_runtime()
            .and_then(|runtime| {
                runtime
                    .set_table_cell_background_color(
                        block_id,
                        range,
                        color.value().map(str::to_owned),
                    )
                    .ok()
            })
            .unwrap_or(false);
        if changed {
            self.dismiss_table_menu_from_gui(cx);
            self.mark_dirty(cx);
            cx.notify();
        }
        changed
    }

    pub(crate) fn apply_selected_table_menu_action_from_gui(
        &mut self,
        action: TableMenuAction,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.readonly {
            return false;
        }
        let axis_selection = self.table_interaction_mode.axis_selection();
        let changed = match action {
            TableMenuAction::ToggleHeader => {
                let Some(selection) = axis_selection else {
                    return false;
                };
                let currently_enabled = self
                    .ready_runtime_ref()
                    .and_then(|runtime| runtime.block_payload_record(selection.block_id))
                    .and_then(|record| match &record.payload {
                        cditor_core::rich_text::BlockPayload::Table(table) => {
                            Some(match selection.axis {
                                TableAxis::Row => table.header_rows > 0,
                                TableAxis::Column => table.header_cols > 0,
                            })
                        }
                        _ => None,
                    })
                    .unwrap_or(false);
                let count = usize::from(!currently_enabled);
                self.ready_runtime()
                    .and_then(|runtime| match selection.axis {
                        TableAxis::Row => runtime
                            .set_table_header_rows(selection.block_id, count)
                            .ok(),
                        TableAxis::Column => runtime
                            .set_table_header_columns(selection.block_id, count)
                            .ok(),
                    })
                    .unwrap_or(false)
            }
            TableMenuAction::BackgroundColor => {
                return self.set_table_background_submenu_open_from_gui(true, cx);
            }
            TableMenuAction::InsertRowAbove
                if axis_selection.is_some_and(|selection| selection.axis == TableAxis::Row) =>
            {
                let selection = axis_selection.expect("checked row selection");
                self.ready_runtime()
                    .and_then(|runtime| {
                        runtime
                            .insert_table_row(selection.block_id, selection.index)
                            .ok()
                    })
                    .unwrap_or(false)
            }
            TableMenuAction::InsertRowBelow
                if axis_selection.is_some_and(|selection| selection.axis == TableAxis::Row) =>
            {
                let selection = axis_selection.expect("checked row selection");
                self.ready_runtime()
                    .and_then(|runtime| {
                        runtime
                            .insert_table_row(selection.block_id, selection.index.saturating_add(1))
                            .ok()
                    })
                    .unwrap_or(false)
            }
            TableMenuAction::DeleteRow
                if axis_selection.is_some_and(|selection| selection.axis == TableAxis::Row) =>
            {
                let selection = axis_selection.expect("checked row selection");
                self.ready_runtime()
                    .and_then(|runtime| {
                        runtime
                            .delete_table_row(selection.block_id, selection.index)
                            .ok()
                    })
                    .unwrap_or(false)
            }
            TableMenuAction::DuplicateRow
                if axis_selection.is_some_and(|selection| selection.axis == TableAxis::Row) =>
            {
                let selection = axis_selection.expect("checked row selection");
                self.ready_runtime()
                    .and_then(|runtime| {
                        runtime
                            .duplicate_table_row(selection.block_id, selection.index)
                            .ok()
                    })
                    .unwrap_or(false)
            }
            TableMenuAction::InsertColumnLeft
                if axis_selection.is_some_and(|selection| selection.axis == TableAxis::Column) =>
            {
                let selection = axis_selection.expect("checked column selection");
                self.ready_runtime()
                    .and_then(|runtime| {
                        runtime
                            .insert_table_column(selection.block_id, selection.index)
                            .ok()
                    })
                    .unwrap_or(false)
            }
            TableMenuAction::InsertColumnRight
                if axis_selection.is_some_and(|selection| selection.axis == TableAxis::Column) =>
            {
                let selection = axis_selection.expect("checked column selection");
                self.ready_runtime()
                    .and_then(|runtime| {
                        runtime
                            .insert_table_column(
                                selection.block_id,
                                selection.index.saturating_add(1),
                            )
                            .ok()
                    })
                    .unwrap_or(false)
            }
            TableMenuAction::DeleteColumn
                if axis_selection.is_some_and(|selection| selection.axis == TableAxis::Column) =>
            {
                let selection = axis_selection.expect("checked column selection");
                self.ready_runtime()
                    .and_then(|runtime| {
                        runtime
                            .delete_table_column(selection.block_id, selection.index)
                            .ok()
                    })
                    .unwrap_or(false)
            }
            TableMenuAction::DuplicateColumn
                if axis_selection.is_some_and(|selection| selection.axis == TableAxis::Column) =>
            {
                let selection = axis_selection.expect("checked column selection");
                self.ready_runtime()
                    .and_then(|runtime| {
                        runtime
                            .duplicate_table_column(selection.block_id, selection.index)
                            .ok()
                    })
                    .unwrap_or(false)
            }
            TableMenuAction::ClearContents => {
                let Some((block_id, range)) = self.selected_table_range() else {
                    return false;
                };
                self.ready_runtime()
                    .and_then(|runtime| runtime.clear_table_range(block_id, range).ok())
                    .unwrap_or(false)
            }
            TableMenuAction::DuplicateRow
            | TableMenuAction::DuplicateColumn
            | TableMenuAction::InsertRowAbove
            | TableMenuAction::InsertRowBelow
            | TableMenuAction::DeleteRow
            | TableMenuAction::InsertColumnLeft
            | TableMenuAction::InsertColumnRight
            | TableMenuAction::DeleteColumn => false,
        };
        if changed {
            self.dismiss_table_menu_from_gui(cx);
            self.mark_dirty(cx);
            cx.notify();
        }
        changed
    }

    pub(in crate::gui::app) fn selected_table_range(&self) -> Option<(BlockId, TableRange)> {
        if let Some(selection) = self.projected_table_cell_selection() {
            let range = self.ready_runtime_ref()?.table_cell_selection_range(
                selection.block_id,
                selection.row,
                selection.col,
            )?;
            return Some((selection.block_id, range));
        }
        if let Some(selection) = self.projected_table_range_selection() {
            let runtime = self.ready_runtime_ref()?;
            return runtime
                .table_range_selection_range(selection.block_id, selection.range)
                .map(|range| (selection.block_id, range));
        }
        let selection = self.projected_table_axis_selection()?;
        let runtime = self.ready_runtime_ref()?;
        let range = match selection.axis {
            TableAxis::Row => {
                runtime.table_row_selection_range(selection.block_id, selection.index)
            }
            TableAxis::Column => {
                runtime.table_column_selection_range(selection.block_id, selection.index)
            }
        };
        range.map(|range| (selection.block_id, range))
    }
}
