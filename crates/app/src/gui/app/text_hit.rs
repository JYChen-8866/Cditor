use gpui::{Pixels, Point};

use cditor_core::ids::BlockId;
use cditor_runtime::DocumentRuntime;

use crate::gui::app::cditor_v2_view::CditorV2View;
use crate::gui::app::cditor_v2_view::TableCellLayoutKey;
use crate::gui::app::interaction::geometry::FallbackViewportOrigin;
use crate::gui::text::{
    RichTextLayoutInput, RichTextPlatformLayout, platform_index_for_point, wrap_rich_text,
};

impl CditorV2View {
    pub(in crate::gui::app) fn current_text_layout_cache(
        &self,
        runtime: &DocumentRuntime,
        block_id: BlockId,
    ) -> Option<&RichTextPlatformLayout> {
        let cache = self.text_layouts.get(&block_id)?;
        let current_content_version = runtime.block_content_version(block_id)?;
        (cache.content_version == current_content_version).then_some(cache)
    }

    pub(in crate::gui::app) fn current_table_cell_layout_cache(
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

    pub(in crate::gui::app) fn text_offset_for_block_at_position(
        &self,
        block_id: BlockId,
        position: Point<Pixels>,
    ) -> Option<usize> {
        let runtime = self.ready_runtime_ref()?;
        if let Some(cache) = self.current_text_layout_cache(runtime, block_id) {
            return Some(platform_index_for_point(cache, position));
        }
        self.fallback_text_offset_for_block_at_position(runtime, block_id, position)
    }

    pub(in crate::gui::app) fn text_offset_for_table_cell_at_position(
        &self,
        block_id: BlockId,
        row: usize,
        col: usize,
        position: Point<Pixels>,
    ) -> Option<usize> {
        let runtime = self.ready_runtime_ref()?;
        let cache = self.current_table_cell_layout_cache(runtime, block_id, row, col)?;
        Some(platform_index_for_point(cache, position))
    }

    fn fallback_text_offset_for_block_at_position(
        &self,
        runtime: &DocumentRuntime,
        block_id: BlockId,
        position: Point<Pixels>,
    ) -> Option<usize> {
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
            _ => return Some(0),
        };
        let text = cditor_core::rich_text::plain_text_from_spans(&spans);
        if text.is_empty() {
            return Some(0);
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
            content_version: payload.content_version,
            layout_version: 0,
            kind: payload.kind,
            spans,
            width_px: rect.text_width_px,
            theme_version: 1,
            font_version: 1,
        };
        let layout = wrap_rich_text(&input);
        Some(layout.offset_for_point(&text, hit_point))
    }

    pub(in crate::gui::app) fn infer_document_viewport_origin(
        &self,
    ) -> Option<FallbackViewportOrigin> {
        self.text_layouts.iter().find_map(|(block_id, cache)| {
            let rect = self
                .projected_block_rects
                .iter()
                .find(|rect| rect.block_id == *block_id)?;
            let runtime = self.ready_runtime_ref()?;
            if runtime.block_content_version(*block_id)? != cache.content_version {
                return None;
            }
            Some(FallbackViewportOrigin {
                x: f32::from(cache.bounds.left()) as f64 - rect.text_origin_x_in_block_px,
                y: f32::from(cache.bounds.top()) as f64 - rect.document_top
                    + runtime.scroll.global_scroll_top
                    - rect.text_origin_y_in_block_px,
            })
        })
    }
}

pub(in crate::gui::app) fn table_cell_layout_cache_is_current(
    runtime: &DocumentRuntime,
    cache: &RichTextPlatformLayout,
    block_id: BlockId,
) -> bool {
    runtime
        .block_content_version(block_id)
        .is_some_and(|current_content_version| cache.content_version == current_content_version)
}

pub(in crate::gui::app) fn fallback_text_hit_point(
    position: Point<Pixels>,
    viewport_origin: FallbackViewportOrigin,
    document_top: f64,
    text_origin_x_in_block_px: f64,
    text_origin_y_in_block_px: f64,
    global_scroll_top: f64,
) -> crate::gui::text::TextHitPoint {
    let text_origin_x = viewport_origin.x + text_origin_x_in_block_px;
    let text_origin_y =
        viewport_origin.y + document_top - global_scroll_top + text_origin_y_in_block_px;
    crate::gui::text::TextHitPoint {
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
        runtime.focus_table_cell_at_offset(1, 0, 0, 4).unwrap();
        runtime
            .replace_text_in_focused_range(None, "\nmore")
            .unwrap();
        let current_version = runtime.block_content_version(1).unwrap();
        let stale_cache = RichTextPlatformLayout {
            block_id: 1,
            content_version: current_version.saturating_sub(1),
            text: "cell".to_owned(),
            lines: Vec::new(),
            bounds: Bounds {
                origin: point(px(10.0), px(20.0)),
                size: Size {
                    width: px(120.0),
                    height: px(36.0),
                },
            },
            line_height: px(17.5),
            measured_height: 36.0,
            table_cell_position: Some(TableCellPosition { row: 0, col: 0 }),
        };
        assert!(!table_cell_layout_cache_is_current(
            &runtime,
            &stale_cache,
            1
        ));
        let current_cache = RichTextPlatformLayout {
            content_version: current_version,
            bounds: Bounds {
                origin: point(px(10.0), px(20.0)),
                size: Size {
                    width: px(120.0),
                    height: px(88.0),
                },
            },
            measured_height: 88.0,
            ..stale_cache
        };

        assert!(table_cell_layout_cache_is_current(
            &runtime,
            &current_cache,
            1
        ));
    }
}
