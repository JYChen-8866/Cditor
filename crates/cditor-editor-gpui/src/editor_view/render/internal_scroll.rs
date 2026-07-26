use std::collections::{HashMap, HashSet};

use cditor_core::ids::BlockId;
use cditor_runtime::EditorViewProjection;

use crate::editor_view::CditorV2View;
use crate::interaction::table_scroll::{TableScrollSnapshot, clamped_table_scroll_offset_x};
use crate::overlays::table::{table_hscroll_scroll_max, table_hscroll_track_width};

pub(super) struct InternalScrollProjection {
    pub(super) table_scroll_snapshots: HashMap<BlockId, TableScrollSnapshot>,
    pub(super) code_scroll_handles: HashMap<BlockId, gpui::ScrollHandle>,
    pub(super) code_caret_reveal_after_line_break: HashSet<BlockId>,
    pub(super) corrected_table_scroll_offsets: Vec<(BlockId, f32)>,
}

pub(super) fn prepare_internal_scroll_projection(
    view: &mut CditorV2View,
    projection: &EditorViewProjection,
) -> InternalScrollProjection {
    let mut table_scroll_snapshots = HashMap::new();
    let mut code_scroll_handles = HashMap::new();
    let mut code_caret_reveal_after_line_break = HashSet::new();
    let mut corrected_table_scroll_offsets = Vec::new();

    for block in &projection.blocks {
        if matches!(
            block.kind,
            cditor_core::rich_text::RichBlockKind::Code { .. }
        ) {
            code_scroll_handles.insert(block.block_id, view.code_scroll_handle(block.block_id));
            if view.take_code_caret_reveal_after_line_break(block.block_id) {
                code_caret_reveal_after_line_break.insert(block.block_id);
            }
        }

        let Some(table_view) = block.table_view.as_ref() else {
            continue;
        };
        let handle =
            view.table_scroll_handle(block.block_id, table_view.horizontal_scroll_offset_px);
        let viewport_measurement = view.stable_table_viewport_measurement(block.block_id, &handle);
        let mut projected_offset_x = table_view.horizontal_scroll_offset_px;
        if let Some(measurement) = viewport_measurement {
            let track_width_px = table_hscroll_track_width(measurement.viewport_width_px, 0.0);
            let max_offset_x = table_hscroll_scroll_max(table_view.width_px, track_width_px);
            projected_offset_x =
                clamped_table_scroll_offset_x(table_view.horizontal_scroll_offset_px, max_offset_x);
            if projected_offset_x != table_view.horizontal_scroll_offset_px {
                corrected_table_scroll_offsets.push((block.block_id, projected_offset_x));
            }
        }
        view.interaction
            .table_scroll_state
            .sync_handle_offset_x(block.block_id, projected_offset_x);
        table_scroll_snapshots.insert(
            block.block_id,
            TableScrollSnapshot {
                offset_y: f32::from(handle.offset().y),
                handle,
                viewport_measurement,
                offset_x: projected_offset_x,
            },
        );
    }

    InternalScrollProjection {
        table_scroll_snapshots,
        code_scroll_handles,
        code_caret_reveal_after_line_break,
        corrected_table_scroll_offsets,
    }
}
