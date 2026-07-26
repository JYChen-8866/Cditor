use cditor_editor_protocol::command::{CditorCommand, TableCellNavigationDirection};
use cditor_session::TableInteractionSnapshot;

use super::routing::BoundInputAction;

pub(super) fn table_cell_text_layout_target(
    cache: Option<&crate::text::RichTextPlatformLayout>,
    context: Option<&TableInteractionSnapshot>,
    action: BoundInputAction,
    preferred_x: Option<(cditor_core::ids::SurfaceId, f32)>,
) -> (
    Option<crate::text::TextLayoutPosition>,
    Option<(cditor_core::ids::SurfaceId, f32)>,
) {
    use crate::text::{
        TextGeometryOperation, TextLayoutMoveCommand, TextLayoutPosition, TextLayoutSelection,
        record_snapshot_geometry, record_unavailable_geometry,
    };

    let (command, extend) = match action {
        BoundInputAction::MoveLeft { extend_selection } => {
            (TextLayoutMoveCommand::PreviousVisual, extend_selection)
        }
        BoundInputAction::MoveRight { extend_selection } => {
            (TextLayoutMoveCommand::NextVisual, extend_selection)
        }
        BoundInputAction::MoveUp { extend_selection } => {
            (TextLayoutMoveCommand::PreviousLine, extend_selection)
        }
        BoundInputAction::MoveDown { extend_selection } => {
            (TextLayoutMoveCommand::NextLine, extend_selection)
        }
        BoundInputAction::MoveToLineStart { extend_selection } => {
            (TextLayoutMoveCommand::LineStart, extend_selection)
        }
        BoundInputAction::MoveToLineEnd { extend_selection } => {
            (TextLayoutMoveCommand::LineEnd, extend_selection)
        }
        BoundInputAction::MoveToPreviousWord { extend_selection } => {
            (TextLayoutMoveCommand::PreviousVisualWord, extend_selection)
        }
        BoundInputAction::MoveToNextWord { extend_selection } => {
            (TextLayoutMoveCommand::NextVisualWord, extend_selection)
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
    let Some(cache) = cache else {
        record_unavailable_geometry();
        return (
            None,
            preferred_x.filter(|(surface, _)| *surface == surface_id),
        );
    };
    if cache.surface_id != surface_id || cache.content_version != focused.block_content_version {
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
    let selection = TextLayoutSelection {
        anchor: TextLayoutPosition::downstream(anchor_offset),
        focus: TextLayoutPosition { offset, affinity },
    };
    let current_preferred_x = preferred_x
        .filter(|(surface, _)| *surface == surface_id)
        .map(|(_, x)| x);
    let (moved, next_preferred_x) =
        layout.move_selection_with_preferred_x(selection, command, extend, current_preferred_x);
    (
        (moved.focus != (TextLayoutPosition { offset, affinity })).then_some(moved.focus),
        next_preferred_x.map(|x| (surface_id, x)),
    )
}

pub(super) fn table_cell_vertical_selection_target(
    cache: Option<&crate::text::RichTextPlatformLayout>,
    context: Option<&TableInteractionSnapshot>,
    direction: i32,
) -> Option<usize> {
    let focused = context?.focused_cell.as_ref()?;
    let (block_id, row, col, caret) = (focused.block_id, focused.row, focused.col, focused.offset);
    let surface_id = cditor_core::ids::SurfaceId::TableCell {
        block_id,
        row,
        column: col,
    };
    let cache = cache?;
    if cache.surface_id != surface_id || cache.content_version != focused.block_content_version {
        return None;
    }
    let caret_rect = cache
        .snapshot
        .caret_rect(crate::text::TextLayoutPosition::downstream(caret), 1.0);
    let next = crate::text::platform_text_position_for_local_point(
        cache,
        crate::text::TextHitPoint {
            x: f64::from(caret_rect.x + caret_rect.width / 2.0),
            y: f64::from(caret_rect.y + caret_rect.height * direction as f32),
        },
    )
    .offset;
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

pub(super) fn table_cell_text_layout_selection_command(
    context: &TableInteractionSnapshot,
    position: crate::text::TextLayoutPosition,
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
