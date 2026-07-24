use cditor_core::ids::{BlockId, SurfaceId};
use cditor_core::rich_text::TableRange;
use gpui::{Context, Pixels, Point, Window};

use crate::app::cditor_v2_view::{CditorV2View, CditorViewState};
use crate::block::table::menu::{
    TableBackgroundColor, TableMenuAction, filter_table_menu_items, table_axis_menu_items,
};
use crate::block::table::{
    TableAxis, TableAxisSelection, TableCellRangeSelection, TableCellSelection,
};
use crate::interaction::table_mode::GuiTableInteractionMode;
use cditor_editor_protocol::command::{
    CditorCommand, CommandEnvelope, CommandOutcomeStatus, CommandSource,
    TableAxis as CommandTableAxis,
};
use cditor_session::TableRangeRequest;

impl CditorV2View {
    pub(crate) fn dismiss_table_menu_from_gui(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.interaction.table_interaction_mode.is_menu_open() {
            return false;
        }
        self.interaction.table_interaction_mode = self
            .interaction
            .table_interaction_mode
            .cell_selection()
            .map(|selection| GuiTableInteractionMode::EditingCell {
                block_id: selection.block_id,
                row: selection.row,
                col: selection.col,
            })
            .unwrap_or(GuiTableInteractionMode::Idle);
        self.overlay.table_menu_ui = Default::default();
        cx.notify();
        true
    }

    pub(in crate::app) fn projected_table_axis_selection(&self) -> Option<TableAxisSelection> {
        self.interaction.table_interaction_mode.axis_selection()
    }

    pub(in crate::app) fn projected_table_axis_visual_selection(
        &self,
    ) -> Option<TableAxisSelection> {
        self.interaction
            .table_interaction_mode
            .visual_axis_selection()
    }

    pub(in crate::app) fn projected_table_range_selection(
        &self,
    ) -> Option<TableCellRangeSelection> {
        self.interaction.table_interaction_mode.range_selection()
    }

    pub(in crate::app) fn projected_table_cell_selection(&self) -> Option<TableCellSelection> {
        self.interaction.table_interaction_mode.cell_selection()
    }

    pub(crate) fn open_table_cell_menu_from_gui(
        &mut self,
        block_id: BlockId,
        row: usize,
        col: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus.editor, cx);
        if !self.commit_document_composition_before_external_focus(cx) {
            return;
        }
        let already_focused = self
            .ready_session()
            .and_then(|session| session.table_interaction(None).ok()?.focused_cell)
            .is_some_and(|focused| {
                (focused.block_id, focused.row, focused.col) == (block_id, row, col)
            });
        if !already_focused && let Some(session) = self.ready_session() {
            let _ = session.dispatch_with_snapshot(
                cditor_editor_protocol::command::CommandEnvelope::new(
                    CditorCommand::FocusTableCell {
                        block_id,
                        row,
                        col,
                        offset: None,
                        affinity: cditor_core::edit::TextAffinity::Downstream,
                    },
                    CommandSource::Toolbar,
                ),
            );
        }
        self.interaction.table_interaction_mode =
            GuiTableInteractionMode::CellMenu(TableCellSelection::new(block_id, row, col));
        self.overlay.table_menu_ui = Default::default();
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
        window.focus(&self.focus.editor, cx);
        self.interaction.text_drag_selection = None;
        self.interaction.table_interaction_mode =
            GuiTableInteractionMode::EditingCell { block_id, row, col };
        let text_position = position.and_then(|position| {
            self.text_position_for_table_cell_at_position(block_id, row, col, position)
        });
        super::trace_table(
            "focus_cell.gui.begin",
            format_args!(
                "block={block_id} row={row} col={col} position={position:?} resolved_position={text_position:?}"
            ),
        );
        if let CditorViewState::Ready(session) = &self.state {
            let command = if let Some(text_position) = text_position {
                CditorCommand::FocusTableCell {
                    block_id,
                    row,
                    col,
                    offset: Some(text_position.offset),
                    affinity: text_position.affinity,
                }
            } else {
                CditorCommand::FocusTableCell {
                    block_id,
                    row,
                    col,
                    offset: None,
                    affinity: cditor_core::edit::TextAffinity::Downstream,
                }
            };
            let _ = session.dispatch_with_snapshot(
                cditor_editor_protocol::command::CommandEnvelope::new(
                    command,
                    CommandSource::Toolbar,
                ),
            );
            let Ok(snapshot) = session.table_interaction(Some(block_id)) else {
                return;
            };
            let payload_state = snapshot
                .requested_table
                .map(|table| {
                    format!(
                        "table rows={} cols={} content_version={}",
                        table.rows, table.cols, table.content_version
                    )
                })
                .unwrap_or_else(|| "missing_or_non_table_payload".to_owned());
            super::trace_table(
                "focus_cell.gui.end",
                format_args!(
                    "block={block_id} row={row} col={col} focused_block={:?} focused_cell={:?} payload={payload_state}",
                    snapshot.focused_block_id, snapshot.focused_cell
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
        pointer: Option<(Point<Pixels>, usize)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let position = pointer.map(|(position, _)| position);
        let click_count = pointer.map(|(_, click_count)| click_count).unwrap_or(1);
        self.focus_table_cell_from_gui(block_id, row, col, position, window, cx);
        let surface_version = self.ready_session().and_then(|session| {
            session
                .surface_version(SurfaceId::TableCell {
                    block_id,
                    row,
                    column: col,
                })
                .ok()
                .flatten()
        });
        if let Some(kind) = crate::app::text_hit::selection_kind_for_click_count(click_count)
            && let Some(position) = position
            && let Some(current) = surface_version
            && let Some(cache) = self.current_table_cell_layout_cache(current, block_id, row, col)
        {
            let local_x = f32::from(position.x - cache.bounds.left());
            let local_y = f32::from(position.y - cache.bounds.top());
            let selection = cache.snapshot.selection_at_point(local_x, local_y, kind);
            if let Some(session) = self.ready_session() {
                let _ = session.dispatch_with_snapshot(CommandEnvelope::new(
                    CditorCommand::SetTableCellSelection {
                        block_id,
                        row,
                        col,
                        anchor_offset: selection.anchor.offset,
                        focus_offset: selection.focus.offset,
                        focus_affinity: selection.focus.affinity,
                    },
                    CommandSource::Toolbar,
                ));
            }
        }
        let anchor_position = self
            .ready_session()
            .and_then(|session| session.table_interaction(None).ok()?.focused_cell)
            .filter(|focused| (focused.block_id, focused.row, focused.col) == (block_id, row, col))
            .map(|focused| (focused.offset, focused.affinity))
            .unwrap_or((0, cditor_core::edit::TextAffinity::Downstream));
        self.interaction.table_interaction_mode = GuiTableInteractionMode::SelectingCellText {
            block_id,
            row,
            col,
            anchor_offset: anchor_position.0,
            anchor_affinity: anchor_position.1,
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
            anchor_affinity: _,
        } = self.interaction.table_interaction_mode
        else {
            return;
        };
        if (anchor_block_id, anchor_row, anchor_col) != (block_id, row, col) {
            return;
        }
        let Some(focus_position) =
            self.text_position_for_table_cell_at_position(block_id, row, col, position)
        else {
            return;
        };
        let changed = self.ready_session().is_some_and(|session| {
            session
                .dispatch_with_snapshot(CommandEnvelope::new(
                    CditorCommand::SetTableCellSelection {
                        block_id,
                        row,
                        col,
                        anchor_offset,
                        focus_offset: focus_position.offset,
                        focus_affinity: focus_position.affinity,
                    },
                    CommandSource::Toolbar,
                ))
                .is_ok_and(|snapshot| snapshot.outcome.changed())
        });
        if changed {
            cx.notify();
        }
    }

    pub(in crate::app) fn finish_table_cell_text_selection_drag(&mut self) {
        if let GuiTableInteractionMode::SelectingCellText {
            block_id, row, col, ..
        } = self.interaction.table_interaction_mode
        {
            self.interaction.table_interaction_mode =
                GuiTableInteractionMode::EditingCell { block_id, row, col };
        }
    }

    pub(crate) fn confirm_table_menu_from_gui(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(selection) = self.interaction.table_interaction_mode.axis_selection() else {
            return false;
        };
        let Some(action) = filter_table_menu_items(
            &table_axis_menu_items(selection),
            &self.overlay.table_menu_ui.query,
        )
        .first()
        .map(|item| item.action) else {
            return true;
        };
        let _ = self.apply_selected_table_menu_action_from_gui(action, cx);
        true
    }

    pub(crate) fn duplicate_selected_table_axis_from_gui(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(selection) = self.interaction.table_interaction_mode.axis_selection() else {
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
        if self
            .interaction
            .table_interaction_mode
            .axis_selection()
            .is_none()
        {
            return false;
        }
        self.overlay.table_menu_ui.delete_backward();
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
        if self.overlay.table_menu_ui.color_submenu_open == open {
            return false;
        }
        self.overlay.table_menu_ui.color_submenu_open = open;
        cx.notify();
        true
    }

    pub(crate) fn set_selected_table_background_from_gui(
        &mut self,
        color: TableBackgroundColor,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.status.readonly {
            return false;
        }
        let Some((block_id, range)) = self.selected_table_range() else {
            return false;
        };
        let changed = matches!(
            self.dispatch_command(
                CditorCommand::TableSetRangeBackground {
                    block_id,
                    range,
                    color: color.value().map(str::to_owned),
                },
                CommandSource::ContextMenu,
                cx,
            ),
            Ok(outcome) if outcome.status == CommandOutcomeStatus::Applied
        );
        if changed {
            self.dismiss_table_menu_from_gui(cx);
        }
        changed
    }

    pub(crate) fn apply_selected_table_menu_action_from_gui(
        &mut self,
        action: TableMenuAction,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.status.readonly {
            return false;
        }
        let axis_selection = self.interaction.table_interaction_mode.axis_selection();
        let command = match action {
            TableMenuAction::ToggleHeader => {
                axis_selection.map(|selection| CditorCommand::TableToggleHeader {
                    block_id: selection.block_id,
                    axis: command_table_axis(selection.axis),
                })
            }
            TableMenuAction::BackgroundColor => {
                return self.set_table_background_submenu_open_from_gui(true, cx);
            }
            TableMenuAction::InsertRowAbove
                if axis_selection.is_some_and(|selection| selection.axis == TableAxis::Row) =>
            {
                let selection = axis_selection.expect("checked row selection");
                Some(CditorCommand::TableInsertAxis {
                    block_id: selection.block_id,
                    axis: CommandTableAxis::Row,
                    index: selection.index,
                })
            }
            TableMenuAction::InsertRowBelow
                if axis_selection.is_some_and(|selection| selection.axis == TableAxis::Row) =>
            {
                let selection = axis_selection.expect("checked row selection");
                Some(CditorCommand::TableInsertAxis {
                    block_id: selection.block_id,
                    axis: CommandTableAxis::Row,
                    index: selection.index.saturating_add(1),
                })
            }
            TableMenuAction::DeleteRow
                if axis_selection.is_some_and(|selection| selection.axis == TableAxis::Row) =>
            {
                let selection = axis_selection.expect("checked row selection");
                Some(CditorCommand::TableDeleteAxis {
                    block_id: selection.block_id,
                    axis: CommandTableAxis::Row,
                    index: selection.index,
                })
            }
            TableMenuAction::DuplicateRow
                if axis_selection.is_some_and(|selection| selection.axis == TableAxis::Row) =>
            {
                let selection = axis_selection.expect("checked row selection");
                Some(CditorCommand::TableDuplicateAxis {
                    block_id: selection.block_id,
                    axis: CommandTableAxis::Row,
                    index: selection.index,
                })
            }
            TableMenuAction::InsertColumnLeft
                if axis_selection.is_some_and(|selection| selection.axis == TableAxis::Column) =>
            {
                let selection = axis_selection.expect("checked column selection");
                Some(CditorCommand::TableInsertAxis {
                    block_id: selection.block_id,
                    axis: CommandTableAxis::Column,
                    index: selection.index,
                })
            }
            TableMenuAction::InsertColumnRight
                if axis_selection.is_some_and(|selection| selection.axis == TableAxis::Column) =>
            {
                let selection = axis_selection.expect("checked column selection");
                Some(CditorCommand::TableInsertAxis {
                    block_id: selection.block_id,
                    axis: CommandTableAxis::Column,
                    index: selection.index.saturating_add(1),
                })
            }
            TableMenuAction::DeleteColumn
                if axis_selection.is_some_and(|selection| selection.axis == TableAxis::Column) =>
            {
                let selection = axis_selection.expect("checked column selection");
                Some(CditorCommand::TableDeleteAxis {
                    block_id: selection.block_id,
                    axis: CommandTableAxis::Column,
                    index: selection.index,
                })
            }
            TableMenuAction::DuplicateColumn
                if axis_selection.is_some_and(|selection| selection.axis == TableAxis::Column) =>
            {
                let selection = axis_selection.expect("checked column selection");
                Some(CditorCommand::TableDuplicateAxis {
                    block_id: selection.block_id,
                    axis: CommandTableAxis::Column,
                    index: selection.index,
                })
            }
            TableMenuAction::ClearContents => {
                let Some((block_id, range)) = self.selected_table_range() else {
                    return false;
                };
                Some(CditorCommand::TableClearRange { block_id, range })
            }
            TableMenuAction::DuplicateRow
            | TableMenuAction::DuplicateColumn
            | TableMenuAction::InsertRowAbove
            | TableMenuAction::InsertRowBelow
            | TableMenuAction::DeleteRow
            | TableMenuAction::InsertColumnLeft
            | TableMenuAction::InsertColumnRight
            | TableMenuAction::DeleteColumn => None,
        };
        let Some(command) = command else {
            return false;
        };
        let changed = matches!(
            self.dispatch_command(command, CommandSource::ContextMenu, cx),
            Ok(outcome) if outcome.status == CommandOutcomeStatus::Applied
        );
        if changed {
            self.dismiss_table_menu_from_gui(cx);
        }
        changed
    }

    pub(in crate::app) fn selected_table_range(&self) -> Option<(BlockId, TableRange)> {
        if let Some(selection) = self.projected_table_cell_selection() {
            let range = self
                .ready_session()?
                .table_range(
                    selection.block_id,
                    TableRangeRequest::Cell {
                        row: selection.row,
                        col: selection.col,
                    },
                )
                .ok()
                .flatten()?;
            return Some((selection.block_id, range));
        }
        if let Some(selection) = self.projected_table_range_selection() {
            let session = self.ready_session()?;
            return session
                .table_range(
                    selection.block_id,
                    TableRangeRequest::Range(selection.range),
                )
                .ok()
                .flatten()
                .map(|range| (selection.block_id, range));
        }
        let selection = self.projected_table_axis_selection()?;
        let session = self.ready_session()?;
        let range = match selection.axis {
            TableAxis::Row => {
                session.table_range(selection.block_id, TableRangeRequest::Row(selection.index))
            }
            TableAxis::Column => session.table_range(
                selection.block_id,
                TableRangeRequest::Column(selection.index),
            ),
        };
        range
            .ok()
            .flatten()
            .map(|range| (selection.block_id, range))
    }
}

const fn command_table_axis(axis: TableAxis) -> CommandTableAxis {
    match axis {
        TableAxis::Row => CommandTableAxis::Row,
        TableAxis::Column => CommandTableAxis::Column,
    }
}
