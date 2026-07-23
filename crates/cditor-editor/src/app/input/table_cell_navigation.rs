use std::collections::HashMap;

use cditor_runtime::DocumentRuntime;

use crate::app::cditor_v2_view::TableCellLayoutKey;

use super::actions::BoundInputAction;

pub(super) fn table_cell_parley_target(
    layouts: &HashMap<TableCellLayoutKey, crate::text::RichTextPlatformLayout>,
    runtime: Option<&DocumentRuntime>,
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
    let Some(runtime) = runtime else {
        return (None, None);
    };
    let Some((block_id, row, col, offset, affinity)) = runtime.focused_table_cell_text_position()
    else {
        return (None, None);
    };
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
    if Some(cache.content_version) != runtime.block_content_version(block_id) {
        record_unavailable_geometry();
        return (
            None,
            preferred_x.filter(|(surface, _)| *surface == surface_id),
        );
    }
    let layout = &cache.snapshot;
    record_snapshot_geometry(TextGeometryOperation::Navigation);
    let anchor_offset = runtime
        .focused_table_cell_selection_state()
        .filter(|(id, focused_row, focused_col, _, _, _)| {
            (*id, *focused_row, *focused_col) == (block_id, row, col)
        })
        .map(|(_, _, _, range, reversed, _)| {
            if range.is_empty() {
                offset
            } else if reversed {
                range.end
            } else {
                range.start
            }
        })
        .unwrap_or(offset);
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
    runtime: Option<&DocumentRuntime>,
    direction: i32,
) -> Option<usize> {
    let runtime = runtime?;
    let (block_id, row, col, caret) = runtime.focused_table_cell_offset()?;
    let cache = layouts.get(&TableCellLayoutKey { block_id, row, col })?;
    if cache.content_version != runtime.block_content_version(block_id)? {
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

pub(super) fn dispatch_table_cell_parley_selection(
    runtime: &mut DocumentRuntime,
    position: crate::text::ParleyTextPosition,
    extend_selection: bool,
) -> Option<bool> {
    dispatch_table_cell_offset_selection(
        runtime,
        position.offset,
        position.affinity,
        extend_selection,
    )
}

pub(super) fn dispatch_table_cell_offset_selection(
    runtime: &mut DocumentRuntime,
    focus_offset: usize,
    focus_affinity: cditor_core::edit::TextAffinity,
    extend_selection: bool,
) -> Option<bool> {
    let (block_id, row, col, caret, _) = runtime.focused_table_cell_text_position()?;
    let anchor_offset = if extend_selection {
        runtime
            .focused_table_cell_selection_state()
            .map(|(_, _, _, range, reversed, _)| {
                if range.is_empty() {
                    caret
                } else if reversed {
                    range.end
                } else {
                    range.start
                }
            })
            .unwrap_or(caret)
    } else {
        focus_offset
    };
    runtime
        .dispatch(cditor_editor_protocol::command::CommandEnvelope::new(
            cditor_editor_protocol::command::CditorCommand::SetTableCellSelection {
                block_id,
                row,
                col,
                anchor_offset,
                focus_offset,
                focus_affinity,
            },
            cditor_editor_protocol::command::CommandSource::Keyboard,
        ))
        .ok()
        .map(|outcome| outcome.changed())
}

pub(super) fn dispatch_table_cell_navigation(
    runtime: &mut DocumentRuntime,
    direction: cditor_editor_protocol::command::TableCellNavigationDirection,
    extend_selection: bool,
) -> bool {
    runtime
        .dispatch(cditor_editor_protocol::command::CommandEnvelope::new(
            cditor_editor_protocol::command::CditorCommand::NavigateTableCell {
                direction,
                extend_selection,
            },
            cditor_editor_protocol::command::CommandSource::Keyboard,
        ))
        .is_ok_and(|outcome| outcome.changed())
}
