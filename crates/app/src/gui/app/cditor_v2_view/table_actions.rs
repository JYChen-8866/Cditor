use cditor_core::ids::BlockId;
use cditor_core::rich_text::{TableCellAlign, TableRange};
use gpui::{Context, Pixels, Point, Window};

use crate::gui::app::cditor_v2_view::{CditorV2View, CditorViewState};
use crate::gui::app::interaction::table_mode::GuiTableInteractionMode;
use crate::gui::block::table::menu::TableMenuAction;
use crate::gui::block::table::{TableAxis, TableAxisSelection, TableCellRangeSelection};

impl CditorV2View {
    pub(crate) fn dismiss_table_menu_from_gui(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.table_interaction_mode.is_menu_open() {
            return false;
        }
        self.table_interaction_mode = GuiTableInteractionMode::Idle;
        cx.notify();
        true
    }

    pub(in crate::gui::app) fn projected_table_axis_selection(&self) -> Option<TableAxisSelection> {
        self.table_interaction_mode.axis_selection()
    }

    pub(in crate::gui::app) fn projected_table_range_selection(
        &self,
    ) -> Option<TableCellRangeSelection> {
        self.table_interaction_mode.range_selection()
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

    pub(crate) fn begin_table_cell_range_selection_from_gui(
        &mut self,
        block_id: BlockId,
        row: usize,
        col: usize,
        position: Option<Point<Pixels>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_table_cell_from_gui(block_id, row, col, position, window, cx);
        let selection = TableCellRangeSelection::new(block_id, row, col, row, col);
        self.table_interaction_mode = GuiTableInteractionMode::SelectingRange(selection);
    }

    pub(crate) fn update_table_cell_range_selection_from_gui(
        &mut self,
        block_id: BlockId,
        row: usize,
        col: usize,
        cx: &mut Context<Self>,
    ) {
        let (GuiTableInteractionMode::SelectingRange(anchor)
        | GuiTableInteractionMode::RangeSelected(anchor)) = self.table_interaction_mode
        else {
            return;
        };
        if anchor.block_id != block_id {
            return;
        }
        let selection =
            TableCellRangeSelection::new(block_id, anchor.anchor_row, anchor.anchor_col, row, col);
        self.table_interaction_mode = if selection.is_multi_cell() {
            GuiTableInteractionMode::RangeSelected(selection)
        } else {
            GuiTableInteractionMode::SelectingRange(selection)
        };
        cx.notify();
    }

    pub(in crate::gui::app) fn finish_table_cell_range_selection_drag(&mut self) {
        if let GuiTableInteractionMode::SelectingRange(selection) = self.table_interaction_mode {
            self.table_interaction_mode = if selection.is_multi_cell() {
                GuiTableInteractionMode::RangeSelected(selection)
            } else {
                GuiTableInteractionMode::Idle
            };
        }
    }

    pub(crate) fn select_table_axis_from_gui(
        &mut self,
        block_id: BlockId,
        axis: TableAxis,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus, cx);
        self.clear_gutter_action();
        self.text_drag_selection = None;
        if let CditorViewState::Ready(runtime) = &mut self.state {
            runtime.focus_block(block_id);
        }
        let selection = TableAxisSelection::new(block_id, axis, index);
        self.table_interaction_mode = GuiTableInteractionMode::AxisSelected(selection);
        cx.notify();
    }

    pub(crate) fn set_selected_table_axis_align_from_gui(
        &mut self,
        align: TableCellAlign,
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
            .and_then(|runtime| runtime.set_table_cell_align(block_id, range, align).ok())
            .unwrap_or(false);
        if changed {
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
            TableMenuAction::Align(align) => self.set_selected_table_axis_align_from_gui(align, cx),
            TableMenuAction::MergeCells => self.merge_selected_table_axis_from_gui(cx),
            TableMenuAction::SplitCell => self.split_selected_table_axis_from_gui(cx),
            TableMenuAction::BackgroundColor => {
                let Some((block_id, range)) = self.selected_table_range() else {
                    return false;
                };
                self.ready_runtime()
                    .and_then(|runtime| {
                        runtime
                            .set_table_cell_background_color(
                                block_id,
                                range,
                                Some("action_background".to_owned()),
                            )
                            .ok()
                    })
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

    pub(crate) fn merge_selected_table_axis_from_gui(&mut self, cx: &mut Context<Self>) -> bool {
        if self.readonly {
            return false;
        }
        let Some((block_id, range)) = self.selected_table_range() else {
            return false;
        };
        let changed = self
            .ready_runtime()
            .and_then(|runtime| runtime.merge_table_cells(block_id, range).ok())
            .unwrap_or(false);
        if changed {
            self.mark_dirty(cx);
            cx.notify();
        }
        changed
    }

    pub(crate) fn split_selected_table_axis_from_gui(&mut self, cx: &mut Context<Self>) -> bool {
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
                    .split_table_cell(block_id, range.start_row, range.start_col)
                    .ok()
            })
            .unwrap_or(false);
        if changed {
            self.mark_dirty(cx);
            cx.notify();
        }
        changed
    }

    pub(in crate::gui::app) fn selected_table_range(&self) -> Option<(BlockId, TableRange)> {
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
