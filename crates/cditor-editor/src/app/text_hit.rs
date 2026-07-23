use gpui::{Pixels, Point};

use cditor_core::ids::{BlockId, SurfaceId};
use cditor_runtime::DocumentRuntime;

use crate::app::cditor_v2_view::CditorV2View;
use crate::app::cditor_v2_view::TableCellLayoutKey;
use crate::app::input_trace::trace_input;
use crate::app::interaction::geometry::FallbackViewportOrigin;
use crate::text::{
    ParleySelectionKind, ParleyTextPosition, RichTextElement, RichTextLayoutInput,
    RichTextPlatformLayout, platform_text_position_for_point, record_synchronous_geometry_fallback,
    record_unavailable_geometry, text_geometry_telemetry,
};

pub(in crate::app) fn selection_kind_for_click_count(
    click_count: usize,
) -> Option<ParleySelectionKind> {
    match click_count {
        0 | 1 => None,
        2 => Some(ParleySelectionKind::Word),
        _ => Some(ParleySelectionKind::Line),
    }
}

impl CditorV2View {
    pub(in crate::app) fn current_text_layout_cache(
        &self,
        runtime: &DocumentRuntime,
        block_id: BlockId,
    ) -> Option<&RichTextPlatformLayout> {
        let cache = self.text_layouts.get(&block_id)?;
        let current_content_version = runtime.block_content_version(block_id)?;
        (cache.content_version == current_content_version).then_some(cache)
    }

    pub(in crate::app) fn current_table_cell_layout_cache(
        &self,
        runtime: &DocumentRuntime,
        block_id: BlockId,
        row: usize,
        col: usize,
    ) -> Option<&RichTextPlatformLayout> {
        let cache = self
            .table_cell_layouts
            .get(&TableCellLayoutKey { block_id, row, col })?;
        table_cell_layout_cache_is_current(runtime, cache, block_id).then_some(cache)
    }

    pub(in crate::app) fn current_text_surface_layout_cache(
        &self,
        runtime: &DocumentRuntime,
        surface_id: SurfaceId,
    ) -> Option<&RichTextPlatformLayout> {
        match surface_id {
            SurfaceId::Block(block_id) => self.current_text_layout_cache(runtime, block_id),
            SurfaceId::TableCell {
                block_id,
                row,
                column,
            } => self.current_table_cell_layout_cache(runtime, block_id, row, column),
            SurfaceId::ImageCaption { .. } | SurfaceId::CollectionTitle { .. } => {
                let cache = self.text_surface_layouts.get(&surface_id)?;
                let current = runtime.text_surface_snapshot(surface_id)?;
                (cache.content_version == current.identity.content_version).then_some(cache)
            }
            SurfaceId::Ephemeral { .. } => None,
        }
    }

    pub(in crate::app) fn text_position_for_surface_at_position(
        &self,
        surface_id: SurfaceId,
        position: Point<Pixels>,
    ) -> Option<ParleyTextPosition> {
        let runtime = self.ready_runtime_ref()?;
        let cache = self.current_text_surface_layout_cache(runtime, surface_id)?;
        Some(platform_text_position_for_point(cache, position))
    }

    pub(in crate::app) fn text_position_for_block_at_position(
        &self,
        block_id: BlockId,
        position: Point<Pixels>,
    ) -> Option<ParleyTextPosition> {
        let runtime = self.ready_runtime_ref()?;
        if let Some(cache) = self.current_text_layout_cache(runtime, block_id) {
            return Some(platform_text_position_for_point(cache, position));
        }
        record_synchronous_geometry_fallback();
        let fallback =
            self.fallback_text_position_for_block_at_position(runtime, block_id, position);
        if fallback.is_none() {
            record_unavailable_geometry();
        }
        trace_input(
            "geometry.sync_layout_fallback",
            format_args!(
                "block={block_id} available={} telemetry={:?}",
                fallback.is_some(),
                text_geometry_telemetry()
            ),
        );
        fallback
    }

    pub(in crate::app) fn text_position_for_table_cell_at_position(
        &self,
        block_id: BlockId,
        row: usize,
        col: usize,
        position: Point<Pixels>,
    ) -> Option<ParleyTextPosition> {
        let runtime = self.ready_runtime_ref()?;
        let Some(cache) = self.current_table_cell_layout_cache(runtime, block_id, row, col) else {
            record_unavailable_geometry();
            return None;
        };
        Some(platform_text_position_for_point(cache, position))
    }

    fn fallback_text_position_for_block_at_position(
        &self,
        runtime: &DocumentRuntime,
        block_id: BlockId,
        position: Point<Pixels>,
    ) -> Option<ParleyTextPosition> {
        let rect = self
            .projected_block_rects
            .iter()
            .find(|rect| rect.block_id == block_id)?;
        let viewport_origin = self.infer_document_viewport_origin()?;
        let payload = runtime.block_payload_record(block_id)?;
        let spans = match &payload.payload {
            cditor_core::rich_text::BlockPayload::RichText { spans } => spans.clone(),
            cditor_core::rich_text::BlockPayload::Code { text, .. } => {
                vec![cditor_core::rich_text::InlineSpan::plain(text)]
            }
            cditor_core::rich_text::BlockPayload::Html { html, .. } => {
                vec![cditor_core::rich_text::InlineSpan::plain(html)]
            }
            _ => return Some(ParleyTextPosition::downstream(0)),
        };
        let text = cditor_core::rich_text::plain_text_from_spans(&spans);
        if text.is_empty() {
            return Some(ParleyTextPosition::downstream(0));
        }
        let hit_point = fallback_text_hit_point(
            position,
            viewport_origin,
            rect.document_top,
            rect.text_origin_x_in_block_px,
            rect.text_origin_y_in_block_px,
            runtime.scroll.global_scroll_top,
        );
        let input = RichTextLayoutInput {
            block_id,
            surface_id: crate::text::TextLayoutSurfaceId::Block(block_id),
            content_version: payload.content_version,
            layout_version: runtime.block_layout_version(block_id)?,
            kind: payload.kind,
            text_align: cditor_core::rich_text::TextAlign::Start,
            spans,
            width_px: rect.text_width_px,
            theme_version: 1,
            font_version: 1,
        };
        Some(
            RichTextElement::new(input, crate::theme::GuiTheme::light())
                .hit_test_position(hit_point),
        )
    }

    pub(in crate::app) fn infer_document_viewport_origin(&self) -> Option<FallbackViewportOrigin> {
        let runtime = self.ready_runtime_ref()?;
        let focused = runtime.focused_block_id().and_then(|block_id| {
            viewport_origin_for_block(
                runtime,
                &self.projected_block_rects,
                &self.text_layouts,
                block_id,
            )
        });
        focused.or_else(|| {
            self.projected_block_rects.iter().find_map(|rect| {
                viewport_origin_for_block(
                    runtime,
                    &self.projected_block_rects,
                    &self.text_layouts,
                    rect.block_id,
                )
            })
        })
    }
}

fn viewport_origin_for_block(
    runtime: &DocumentRuntime,
    rects: &[crate::app::interaction::geometry::ProjectedBlockRect],
    layouts: &std::collections::HashMap<BlockId, RichTextPlatformLayout>,
    block_id: BlockId,
) -> Option<FallbackViewportOrigin> {
    let cache = layouts.get(&block_id)?;
    let rect = rects.iter().find(|rect| rect.block_id == block_id)?;
    if runtime.block_content_version(block_id)? != cache.content_version {
        return None;
    }
    Some(FallbackViewportOrigin {
        x: f32::from(cache.bounds.left()) as f64 - rect.text_origin_x_in_block_px,
        y: f32::from(cache.bounds.top()) as f64 - rect.document_top
            + runtime.scroll.global_scroll_top
            - rect.text_origin_y_in_block_px,
    })
}

pub(in crate::app) fn table_cell_layout_cache_is_current(
    runtime: &DocumentRuntime,
    cache: &RichTextPlatformLayout,
    block_id: BlockId,
) -> bool {
    runtime
        .block_content_version(block_id)
        .is_some_and(|current_content_version| cache.content_version == current_content_version)
}

pub(in crate::app) fn fallback_text_hit_point(
    position: Point<Pixels>,
    viewport_origin: FallbackViewportOrigin,
    document_top: f64,
    text_origin_x_in_block_px: f64,
    text_origin_y_in_block_px: f64,
    global_scroll_top: f64,
) -> crate::text::TextHitPoint {
    let text_origin_x = viewport_origin.x + text_origin_x_in_block_px;
    let text_origin_y =
        viewport_origin.y + document_top - global_scroll_top + text_origin_y_in_block_px;
    crate::text::TextHitPoint {
        x: f32::from(position.x) as f64 - text_origin_x,
        y: f32::from(position.y) as f64 - text_origin_y,
    }
}

#[cfg(test)]
mod tests {
    use cditor_core::rich_text::{BlockPayload, BlockPayloadRecord, RichBlockKind};
    use cditor_runtime::TableCellPosition;
    use gpui::{Bounds, Size, point, px};

    use super::*;

    #[test]
    fn fallback_text_hit_point_accounts_for_scroll_and_text_origin() {
        let hit = fallback_text_hit_point(
            point(px(180.0), px(260.0)),
            FallbackViewportOrigin { x: 100.0, y: 40.0 },
            500.0,
            32.0,
            12.0,
            320.0,
        );

        assert_eq!(hit.x, 48.0);
        assert_eq!(hit.y, 28.0);
    }

    #[test]
    fn table_cell_layout_cache_rejects_stale_content_version_for_candidate_bounds() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord {
                block_id: 1,
                content_version: 1,
                kind: RichBlockKind::Table,
                payload: BlockPayload::Table(cditor_core::rich_text::TablePayload {
                    rows: vec![cditor_core::rich_text::TableRowPayload {
                        cells: vec![cditor_core::rich_text::TableCellPayload::plain("cell")],
                        height: Default::default(),
                    }],
                    columns: Vec::new(),
                    header_rows: 0,
                    header_cols: 0,
                    header_style: Default::default(),
                }),
            }],
            720.0,
        );
        crate::test_support::focus_table_cell_at_offset(&mut runtime, 1, 0, 0, 4);
        crate::test_support::replace_realtime_text(&mut runtime, None, "\nmore");
        let current_version = runtime.block_content_version(1).unwrap();
        let stale_cache = crate::text::test_platform_layout(
            1,
            current_version.saturating_sub(1),
            "cell",
            Bounds {
                origin: point(px(10.0), px(20.0)),
                size: Size {
                    width: px(120.0),
                    height: px(36.0),
                },
            },
            Some(TableCellPosition { row: 0, col: 0 }),
        );
        assert!(!table_cell_layout_cache_is_current(
            &runtime,
            &stale_cache,
            1
        ));
        let current_cache = crate::text::test_platform_layout(
            1,
            current_version,
            "cell\nmore",
            Bounds {
                origin: point(px(10.0), px(20.0)),
                size: Size {
                    width: px(120.0),
                    height: px(88.0),
                },
            },
            Some(TableCellPosition { row: 0, col: 0 }),
        );

        assert!(table_cell_layout_cache_is_current(
            &runtime,
            &current_cache,
            1
        ));
    }

    #[test]
    fn click_count_maps_single_to_caret_double_to_word_and_triple_to_line() {
        assert_eq!(selection_kind_for_click_count(1), None);
        assert_eq!(
            selection_kind_for_click_count(2),
            Some(ParleySelectionKind::Word)
        );
        assert_eq!(
            selection_kind_for_click_count(3),
            Some(ParleySelectionKind::Line)
        );
        assert_eq!(
            selection_kind_for_click_count(5),
            Some(ParleySelectionKind::Line)
        );
    }
}
