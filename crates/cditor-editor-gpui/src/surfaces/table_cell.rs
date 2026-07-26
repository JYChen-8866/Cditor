use std::collections::HashMap;

use cditor_core::ids::{BlockId, SurfaceId};
use cditor_core::rich_text::BlockPayloadView;
use cditor_runtime::{EditorViewProjection, TableCellPosition};
use cditor_session::{EditorSessionHandle, SurfaceVersionSnapshot};
use gpui::{Pixels, Point};

use crate::editor_view::CditorV2View;
use crate::features::table::{
    TableResizePreview, V1_TABLE_CELL_PADDING_X_PX, V1_TABLE_CELL_PADDING_Y_PX, core_text_align,
    table_cell_typography, table_view_with_resize_preview,
};
use crate::interaction::geometry::{ProjectedTableCellRect, ProjectedTextPlacement};
use crate::text::{
    RichTextElement, RichTextLayoutInput, RichTextPlatformLayout, TextLayoutPosition,
    TextLayoutSelection, TextLayoutSelectionKind, TextLayoutSurfaceId,
    record_synchronous_geometry_fallback, record_unavailable_geometry,
};
use crate::theme::GuiTheme;

use super::text::{ResolvedProjectedTextGeometry, projected_text_hit_point};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct TableCellLayoutKey {
    pub(crate) block_id: BlockId,
    pub(crate) row: usize,
    pub(crate) col: usize,
}

pub(crate) fn projected_table_cells_from_projection(
    projection: &EditorViewProjection,
    resize_preview: Option<TableResizePreview>,
) -> HashMap<TableCellLayoutKey, ProjectedTableCellRect> {
    let mut projected = HashMap::new();
    for block in &projection.blocks {
        if let Some(table_view) = block.table_view.as_ref() {
            let table_view =
                table_view_with_resize_preview(block.block_id, table_view, resize_preview);
            let content_version = match &block.payload {
                BlockPayloadView::Loaded(payload) => payload.content_version,
                _ => continue,
            };
            for cell in &table_view.visible_cells {
                let key = TableCellLayoutKey {
                    block_id: block.block_id,
                    row: cell.position.row,
                    col: cell.position.col,
                };
                projected.insert(
                    key,
                    ProjectedTableCellRect {
                        block_id: block.block_id,
                        row: cell.position.row,
                        col: cell.position.col,
                        content_version,
                        layout_version: block.layout.layout_version,
                        text_origin_x_in_table_px: f64::from(
                            cell.x_px + V1_TABLE_CELL_PADDING_X_PX,
                        ),
                        text_origin_y_in_table_px: f64::from(
                            cell.y_px + V1_TABLE_CELL_PADDING_Y_PX,
                        ),
                        text_width_px: f64::from(
                            (cell.width_px - V1_TABLE_CELL_PADDING_X_PX * 2.0).max(1.0),
                        ),
                        text_align: core_text_align(cell.align),
                        header: cell.header,
                        projected_horizontal_scroll_offset_px: f64::from(
                            table_view.horizontal_scroll_offset_px,
                        ),
                    },
                );
            }
        }
    }
    projected
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
        let projected = self.projected_table_cell(block_id, row, col)?;
        if !projected_cell_is_current(projected, current) {
            return None;
        }
        let cache =
            self.cache
                .table_cell_layouts
                .get(&TableCellLayoutKey { block_id, row, col })?;
        super::text::layout_cache_is_current(
            cache,
            current,
            Some(projected.text_width_px),
            Some(projected.text_align),
        )
        .then_some(cache)
    }

    pub(crate) fn projected_table_cell(
        &self,
        block_id: BlockId,
        row: usize,
        col: usize,
    ) -> Option<ProjectedTableCellRect> {
        self.interaction
            .projected_table_cells
            .get(&TableCellLayoutKey { block_id, row, col })
            .copied()
    }

    pub(crate) fn projected_table_cell_placement(
        &self,
        block_id: BlockId,
        row: usize,
        col: usize,
    ) -> Option<ProjectedTextPlacement> {
        let projected = self.projected_table_cell(block_id, row, col)?;
        let block = self
            .interaction
            .projected_block_rects
            .iter()
            .find(|block| block.block_id == block_id)
            .copied()?;
        let viewport_origin = self.document_viewport_origin()?;
        let live_scroll_offset = self.interaction.table_scroll_state.live_offset(block_id);
        let (scroll_x, scroll_y) =
            live_scroll_offset.unwrap_or((projected.projected_horizontal_scroll_offset_px, 0.0));
        Some(ProjectedTextPlacement::for_table_cell(
            viewport_origin,
            block,
            projected,
            self.interaction.presented_scroll_top,
            scroll_x,
            scroll_y,
        ))
    }

    pub(crate) fn resolved_table_cell_text_geometry(
        &self,
        current: SurfaceVersionSnapshot,
        block_id: BlockId,
        row: usize,
        col: usize,
    ) -> Option<ResolvedProjectedTextGeometry<'_>> {
        let placement = self.projected_table_cell_placement(block_id, row, col)?;
        let layout = self.current_table_cell_layout_cache(current, block_id, row, col)?;
        ResolvedProjectedTextGeometry::new(layout, placement)
    }

    pub(crate) fn text_position_for_table_cell_at_position(
        &self,
        block_id: BlockId,
        row: usize,
        col: usize,
        position: Point<Pixels>,
    ) -> Option<TextLayoutPosition> {
        let session = self.ready_session()?;
        let surface_id = surface_id(block_id, TableCellPosition { row, col });
        let current = session.surface_version(surface_id).ok().flatten()?;
        let projected = self.projected_table_cell(block_id, row, col)?;
        if !projected_cell_is_current(projected, current) {
            record_unavailable_geometry();
            return None;
        }
        let placement = self.projected_table_cell_placement(block_id, row, col)?;
        if let Some(geometry) = self.resolved_table_cell_text_geometry(current, block_id, row, col)
        {
            return Some(geometry.position_for_window_point(position));
        }
        record_synchronous_geometry_fallback();
        let Some(element) = cold_table_cell_text_element(session, current, projected) else {
            record_unavailable_geometry();
            return None;
        };
        if element.input.spans.is_empty() {
            return Some(TextLayoutPosition::downstream(0));
        }
        Some(element.hit_test_position(projected_text_hit_point(placement, position)))
    }

    pub(crate) fn text_selection_for_table_cell_at_position(
        &self,
        block_id: BlockId,
        row: usize,
        col: usize,
        position: Point<Pixels>,
        kind: TextLayoutSelectionKind,
    ) -> Option<TextLayoutSelection> {
        let session = self.ready_session()?;
        let surface_id = surface_id(block_id, TableCellPosition { row, col });
        let current = session.surface_version(surface_id).ok().flatten()?;
        let projected = self.projected_table_cell(block_id, row, col)?;
        if !projected_cell_is_current(projected, current) {
            record_unavailable_geometry();
            return None;
        }
        let placement = self.projected_table_cell_placement(block_id, row, col)?;
        if let Some(geometry) = self.resolved_table_cell_text_geometry(current, block_id, row, col)
        {
            return Some(geometry.selection_at_window_point(position, kind));
        }
        record_synchronous_geometry_fallback();
        let element = cold_table_cell_text_element(session, current, projected)?;
        Some(element.selection_at_point(projected_text_hit_point(placement, position), kind))
    }
}

fn projected_cell_is_current(
    projected: ProjectedTableCellRect,
    current: SurfaceVersionSnapshot,
) -> bool {
    projected.content_version == current.content_version
        && projected.layout_version == current.layout_version
        && current.surface_id
            == surface_id(
                projected.block_id,
                TableCellPosition {
                    row: projected.row,
                    col: projected.col,
                },
            )
}

fn cold_table_cell_text_element(
    session: &EditorSessionHandle,
    current: SurfaceVersionSnapshot,
    projected: ProjectedTableCellRect,
) -> Option<RichTextElement> {
    let state = session
        .text_surface_state(current.surface_id)
        .ok()
        .flatten()?;
    if state.snapshot.identity.content_version != current.content_version {
        return None;
    }
    let input = RichTextLayoutInput::from_text_surface_snapshot(
        state.snapshot,
        current.layout_version,
        projected.text_align,
        projected.text_width_px,
        1,
        1,
    );
    Some(
        RichTextElement::new(input, GuiTheme::light())
            .with_typography(table_cell_typography(projected.header)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use cditor_core::rich_text::{
        BlockPayload, BlockPayloadRecord, InlineSpan, RichBlockKind, TableCellAlign,
        TableCellPayload, TableColumnPayload, TablePayload, TableRowPayload, TableTrackSize,
        TextAlign,
    };
    use cditor_runtime::DocumentRuntime;
    use gpui::{AppContext, Bounds, Size, TestAppContext, point, px};

    use crate::document::DocumentLayoutMetrics;
    use crate::features::table::TableAxis;
    use crate::interaction::geometry::{
        DocumentViewportOrigin, projected_block_rects_from_projection,
    };

    fn table_runtime(text: &str, align: TableCellAlign, header: bool) -> DocumentRuntime {
        DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord {
                block_id: 1,
                content_version: 1,
                kind: RichBlockKind::Table,
                payload: BlockPayload::Table(TablePayload {
                    rows: vec![TableRowPayload {
                        cells: vec![TableCellPayload {
                            spans: vec![InlineSpan::plain(text)],
                            align,
                            ..Default::default()
                        }],
                        height: TableTrackSize::Px(48),
                    }],
                    columns: vec![TableColumnPayload {
                        width: TableTrackSize::Px(220),
                    }],
                    header_rows: usize::from(header),
                    ..Default::default()
                }),
            }],
            720.0,
        )
    }

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

    #[test]
    fn projected_cell_geometry_uses_padding_alignment_and_live_resize_width() {
        let runtime = table_runtime("header", TableCellAlign::Right, true);
        let projection = runtime.projection_for_window();
        let key = TableCellLayoutKey {
            block_id: 1,
            row: 0,
            col: 0,
        };

        let initial = projected_table_cells_from_projection(&projection, None)[&key];
        assert_eq!(initial.text_origin_x_in_table_px, 10.0);
        assert_eq!(initial.text_origin_y_in_table_px, 7.0);
        assert_eq!(initial.text_width_px, 200.0);
        assert_eq!(initial.text_align, TextAlign::End);
        assert!(initial.header);

        let resized = projected_table_cells_from_projection(
            &projection,
            Some((1, TableAxis::Column, 0, 300.0)),
        )[&key];
        assert_eq!(resized.text_width_px, 280.0);
        assert_eq!(resized.layout_version, initial.layout_version);
    }

    #[gpui::test]
    fn navigation_cache_rejects_stale_layout_version_and_live_resize_width(
        cx: &mut TestAppContext,
    ) {
        let text = "one two three four five";
        let runtime = table_runtime(text, TableCellAlign::Left, false);
        let projection = runtime.projection_for_window();
        let view = cx.new(|cx| CditorV2View::from_runtime(runtime, false, cx));

        view.update(cx, |view, _cx| {
            view.interaction.projected_table_cells =
                projected_table_cells_from_projection(&projection, None);
            let key = TableCellLayoutKey {
                block_id: 1,
                row: 0,
                col: 0,
            };
            let projected = view.interaction.projected_table_cells[&key];
            let surface = surface_id(1, TableCellPosition { row: 0, col: 0 });
            let current = view
                .ready_session()
                .unwrap()
                .surface_version(surface)
                .unwrap()
                .unwrap();
            let mut layout = crate::text::test_platform_layout(
                1,
                current.content_version,
                text,
                Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: Size {
                        width: px(projected.text_width_px as f32),
                        height: px(80.0),
                    },
                },
                Some(TableCellPosition { row: 0, col: 0 }),
            );
            layout.layout_version = current.layout_version;
            view.cache.table_cell_layouts.insert(key, layout, None);

            assert!(
                view.current_table_cell_layout_cache(current, 1, 0, 0)
                    .is_some()
            );

            view.interaction.projected_table_cells = projected_table_cells_from_projection(
                &projection,
                Some((1, TableAxis::Column, 0, 300.0)),
            );
            assert!(
                view.current_table_cell_layout_cache(current, 1, 0, 0)
                    .is_none(),
                "a resize preview must reject geometry shaped at the previous wrap width"
            );

            view.interaction.projected_table_cells =
                projected_table_cells_from_projection(&projection, None);
            assert!(
                view.current_table_cell_layout_cache(
                    SurfaceVersionSnapshot {
                        layout_version: current.layout_version.saturating_add(1),
                        ..current
                    },
                    1,
                    0,
                    0,
                )
                .is_none(),
                "navigation must not consume geometry from an older layout generation"
            );
        });
    }

    #[gpui::test]
    fn table_hit_uses_current_projection_and_live_inner_scroll_not_stale_paint_bounds(
        cx: &mut TestAppContext,
    ) {
        let text = "zero target end";
        let target_offset = text.find("target").unwrap() + 2;
        let runtime = table_runtime(text, TableCellAlign::Left, false);
        let projection = runtime.projection_for_window();
        let view = cx.new(|cx| CditorV2View::from_runtime(runtime, false, cx));

        view.update(cx, |view, _cx| {
            view.interaction.document_viewport_origin =
                Some(DocumentViewportOrigin { x: 120.0, y: 40.0 });
            view.interaction.presented_scroll_top = 20_000_000.25;
            view.interaction.projected_block_rects = projected_block_rects_from_projection(
                &projection,
                DocumentLayoutMetrics::default(),
            );
            view.interaction.projected_block_rects[0].document_top = 20_000_128.25;
            view.interaction.projected_table_cells =
                projected_table_cells_from_projection(&projection, None);
            let handle = view.table_scroll_handle(1, -80.5);
            handle.set_offset(point(px(-80.5), px(-12.25)));

            let surface = surface_id(1, TableCellPosition { row: 0, col: 0 });
            let current = view
                .ready_session()
                .unwrap()
                .surface_version(surface)
                .unwrap()
                .unwrap();
            let projected = view.projected_table_cell(1, 0, 0).unwrap();
            let placement = view.projected_table_cell_placement(1, 0, 0).unwrap();
            let mut stale = crate::text::test_platform_layout(
                1,
                current.content_version,
                text,
                Bounds {
                    origin: point(px(8.0), px(16.0)),
                    size: Size {
                        width: px(projected.text_width_px as f32),
                        height: px(48.0),
                    },
                },
                Some(TableCellPosition { row: 0, col: 0 }),
            );
            stale.layout_version = current.layout_version;
            let caret = stale
                .snapshot
                .caret_rect(TextLayoutPosition::downstream(target_offset), 1.0);
            let click = point(
                px((placement.window_origin_x_px + f64::from(caret.x)) as f32),
                px((placement.window_origin_y_px + f64::from(caret.y + caret.height / 2.0)) as f32),
            );
            view.cache.table_cell_layouts.insert(
                TableCellLayoutKey {
                    block_id: 1,
                    row: 0,
                    col: 0,
                },
                stale,
                None,
            );

            assert_eq!(
                view.text_position_for_table_cell_at_position(1, 0, 0, click)
                    .unwrap()
                    .offset,
                target_offset,
            );
            let caret_bounds = view
                .resolved_table_cell_text_geometry(current, 1, 0, 0)
                .unwrap()
                .bounds_for_range(target_offset..target_offset);
            assert_eq!(
                f32::from(caret_bounds.left()),
                (placement.window_origin_x_px + f64::from(caret.x)) as f32,
            );
            assert_eq!(
                f32::from(caret_bounds.top()),
                (placement.window_origin_y_px + f64::from(caret.y)) as f32,
            );
        });
    }

    #[gpui::test]
    fn cold_table_multiclick_respects_end_alignment_and_header_typography(cx: &mut TestAppContext) {
        let text = "zero target end";
        let word_start = text.find("target").unwrap();
        let target_offset = word_start + 2;
        let runtime = table_runtime(text, TableCellAlign::Right, true);
        let projection = runtime.projection_for_window();
        let view = cx.new(|cx| CditorV2View::from_runtime(runtime, false, cx));

        view.update(cx, |view, _cx| {
            view.interaction.document_viewport_origin =
                Some(DocumentViewportOrigin { x: 20.0, y: 30.0 });
            view.interaction.projected_block_rects = projected_block_rects_from_projection(
                &projection,
                DocumentLayoutMetrics::default(),
            );
            view.interaction.projected_table_cells =
                projected_table_cells_from_projection(&projection, None);
            let surface = surface_id(1, TableCellPosition { row: 0, col: 0 });
            let session = view.ready_session().unwrap();
            let current = session.surface_version(surface).unwrap().unwrap();
            let projected = view.projected_table_cell(1, 0, 0).unwrap();
            let placement = view.projected_table_cell_placement(1, 0, 0).unwrap();
            let element = cold_table_cell_text_element(session, current, projected).unwrap();
            let caret = element.local_caret_rect_for_offset(target_offset);
            let click = point(
                px((placement.window_origin_x_px + f64::from(caret.x)) as f32),
                px((placement.window_origin_y_px + f64::from(caret.y + caret.height / 2.0)) as f32),
            );

            assert_eq!(
                view.text_position_for_table_cell_at_position(1, 0, 0, click)
                    .unwrap()
                    .offset,
                target_offset,
            );
            let selection = view
                .text_selection_for_table_cell_at_position(
                    1,
                    0,
                    0,
                    click,
                    TextLayoutSelectionKind::Word,
                )
                .unwrap();
            let selected = selection.anchor.offset.min(selection.focus.offset)
                ..selection.anchor.offset.max(selection.focus.offset);
            assert_eq!(selected, word_start..word_start + "target".len());
        });
    }
}
