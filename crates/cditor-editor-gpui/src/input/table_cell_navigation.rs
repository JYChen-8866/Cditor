use std::collections::HashMap;

use cditor_editor_protocol::command::{CditorCommand, TableCellNavigationDirection};
use cditor_session::TableInteractionSnapshot;

use crate::surfaces::table_cell::TableCellLayoutKey;

use super::routing::BoundInputAction;

pub(super) fn table_cell_parley_target(
    layouts: &HashMap<TableCellLayoutKey, crate::text::RichTextPlatformLayout>,
    context: Option<&TableInteractionSnapshot>,
    action: BoundInputAction,
    preferred_x: Option<(cditor_core::ids::SurfaceId, f32)>,
) -> (
    Option<crate::text::ParleyTextPosition>,
    Option<(cditor_core::ids::SurfaceId, f32)>,
) {
    use crate::text::{
        ParleyMoveCommand, ParleySelection, ParleyTextPosition, TextGeometryOperation,
        record_snapshot_geometry, record_unavailable_geometry,
    };

    let (command, extend) = match action {
        BoundInputAction::MoveLeft { extend_selection } => {
            (ParleyMoveCommand::PreviousVisual, extend_selection)
        }
        BoundInputAction::MoveRight { extend_selection } => {
            (ParleyMoveCommand::NextVisual, extend_selection)
        }
        BoundInputAction::MoveUp { extend_selection } => {
            (ParleyMoveCommand::PreviousLine, extend_selection)
        }
        BoundInputAction::MoveDown { extend_selection } => {
            (ParleyMoveCommand::NextLine, extend_selection)
        }
        BoundInputAction::MoveToLineStart { extend_selection } => {
            (ParleyMoveCommand::LineStart, extend_selection)
        }
        BoundInputAction::MoveToLineEnd { extend_selection } => {
            (ParleyMoveCommand::LineEnd, extend_selection)
        }
        BoundInputAction::MoveToPreviousWord { extend_selection } => {
            (ParleyMoveCommand::PreviousVisualWord, extend_selection)
        }
        BoundInputAction::MoveToNextWord { extend_selection } => {
            (ParleyMoveCommand::NextVisualWord, extend_selection)
        }
        _ => return (None, None),
    };
    let Some(focused) = context.and_then(|context| context.focused_cell.as_ref()) else {
        return (None, None);
    };
    let (block_id, row, col, offset, affinity) = (
        focused.block_id,
        focused.row,
        focused.col,
        focused.offset,
        focused.affinity,
    );
    let surface_id = cditor_core::ids::SurfaceId::TableCell {
        block_id,
        row,
        column: col,
    };
    let Some(cache) = layouts.get(&TableCellLayoutKey { block_id, row, col }) else {
        record_unavailable_geometry();
        return (
            None,
            preferred_x.filter(|(surface, _)| *surface == surface_id),
        );
    };
    if cache.content_version != focused.block_content_version {
        record_unavailable_geometry();
        return (
            None,
            preferred_x.filter(|(surface, _)| *surface == surface_id),
        );
    }
    let layout = &cache.snapshot;
    record_snapshot_geometry(TextGeometryOperation::Navigation);
    let anchor_offset = if focused.selected_range.is_empty() {
        offset
    } else if focused.selection_reversed {
        focused.selected_range.end
    } else {
        focused.selected_range.start
    };
    let selection = ParleySelection {
        anchor: ParleyTextPosition::downstream(anchor_offset),
        focus: ParleyTextPosition { offset, affinity },
    };
    let current_preferred_x = preferred_x
        .filter(|(surface, _)| *surface == surface_id)
        .map(|(_, x)| x);
    let (moved, next_preferred_x) =
        layout.move_selection_with_preferred_x(selection, command, extend, current_preferred_x);
    (
        (moved.focus != (ParleyTextPosition { offset, affinity })).then_some(moved.focus),
        next_preferred_x.map(|x| (surface_id, x)),
    )
}

pub(super) fn table_cell_vertical_selection_target(
    layouts: &HashMap<TableCellLayoutKey, crate::text::RichTextPlatformLayout>,
    context: Option<&TableInteractionSnapshot>,
    direction: i32,
) -> Option<usize> {
    let focused = context?.focused_cell.as_ref()?;
    let (block_id, row, col, caret) = (focused.block_id, focused.row, focused.col, focused.offset);
    let cache = layouts.get(&TableCellLayoutKey { block_id, row, col })?;
    if cache.content_version != focused.block_content_version {
        return None;
    }
    let bounds = crate::text::platform_range_bounds(cache, caret..caret);
    let target = gpui::point(
        bounds.left() + bounds.size.width / 2.0,
        bounds.top() + bounds.size.height * direction as f32,
    );
    let next = crate::text::platform_index_for_point(cache, target);
    Some(if next == caret {
        if direction < 0 {
            0
        } else {
            cache.snapshot.text().len()
        }
    } else {
        next
    })
}

pub(super) fn table_cell_parley_selection_command(
    context: &TableInteractionSnapshot,
    position: crate::text::ParleyTextPosition,
    extend_selection: bool,
) -> Option<CditorCommand> {
    table_cell_offset_selection_command(
        context,
        position.offset,
        position.affinity,
        extend_selection,
    )
}

pub(super) fn table_cell_offset_selection_command(
    context: &TableInteractionSnapshot,
    focus_offset: usize,
    focus_affinity: cditor_core::edit::TextAffinity,
    extend_selection: bool,
) -> Option<CditorCommand> {
    let focused = context.focused_cell.as_ref()?;
    let (block_id, row, col, caret) = (focused.block_id, focused.row, focused.col, focused.offset);
    let anchor_offset = if extend_selection {
        if focused.selected_range.is_empty() {
            caret
        } else if focused.selection_reversed {
            focused.selected_range.end
        } else {
            focused.selected_range.start
        }
    } else {
        focus_offset
    };
    Some(CditorCommand::SetTableCellSelection {
        block_id,
        row,
        col,
        anchor_offset,
        focus_offset,
        focus_affinity,
    })
}

pub(super) const fn table_cell_navigation_command(
    direction: TableCellNavigationDirection,
    extend_selection: bool,
) -> CditorCommand {
    CditorCommand::NavigateTableCell {
        direction,
        extend_selection,
    }
}
