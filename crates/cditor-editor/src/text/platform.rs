use std::ops::Range;

use cditor_core::edit::{InternalTextOffset, TextAffinity, TextOffsetMap};
use cditor_core::ids::{BlockId, SurfaceId};
use cditor_runtime::{InputSessionIdentity, TableCellPosition};
use gpui::{Bounds, Pixels, Point, point, px, size};

use super::{
    ParleyAccessibilityProjection, ParleyLayoutSnapshot, ParleySelection, ParleyTextPosition,
    TextGeometryOperation, record_snapshot_geometry,
};

pub(crate) struct RichTextPlatformLayout {
    pub block_id: BlockId,
    pub surface_id: SurfaceId,
    pub content_version: u64,
    pub layout_version: u64,
    pub input_session_identity: Option<InputSessionIdentity>,
    pub snapshot: ParleyLayoutSnapshot,
    pub accessibility: Option<ParleyAccessibilityProjection>,
    pub bounds: Bounds<Pixels>,
    pub measured_height: f64,
    pub table_cell_position: Option<TableCellPosition>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TextPlatformLayoutIdentity {
    pub surface_id: SurfaceId,
    pub content_version: u64,
    pub layout_version: u64,
}

impl RichTextPlatformLayout {
    pub fn identity(&self) -> TextPlatformLayoutIdentity {
        TextPlatformLayoutIdentity {
            surface_id: self.surface_id,
            content_version: self.content_version,
            layout_version: self.layout_version,
        }
    }
}

pub(crate) fn platform_range_bounds(
    cache: &RichTextPlatformLayout,
    range: Range<usize>,
) -> Bounds<Pixels> {
    record_snapshot_geometry(TextGeometryOperation::RangeBounds);
    let range = normalized_text_range(cache.snapshot.text(), range);
    let mut local_rects = if range.is_empty() {
        vec![
            cache
                .snapshot
                .caret_rect(ParleyTextPosition::downstream(range.start), 1.0),
        ]
    } else {
        cache.snapshot.selection_rects(ParleySelection {
            anchor: ParleyTextPosition::downstream(range.start),
            focus: ParleyTextPosition {
                offset: range.end,
                affinity: TextAffinity::Upstream,
            },
        })
    };
    if local_rects.is_empty() {
        local_rects.push(
            cache
                .snapshot
                .caret_rect(ParleyTextPosition::downstream(range.start), 1.0),
        );
    }

    let mut rects = local_rects.into_iter().map(|rect| {
        Bounds::new(
            point(
                cache.bounds.left() + px(rect.x),
                cache.bounds.top() + px(rect.y),
            ),
            size(px(rect.width.max(1.0)), px(rect.height.max(0.0))),
        )
    });
    let mut union = rects
        .next()
        .expect("Parley range geometry always yields a caret or selection rectangle");
    for rect in rects {
        union = Bounds::from_corners(
            point(union.left().min(rect.left()), union.top().min(rect.top())),
            point(
                union.right().max(rect.right()),
                union.bottom().max(rect.bottom()),
            ),
        );
    }
    union
}

pub(crate) fn platform_index_for_point(
    cache: &RichTextPlatformLayout,
    position: Point<Pixels>,
) -> usize {
    platform_text_position_for_point(cache, position).offset
}

pub(crate) fn platform_text_position_for_point(
    cache: &RichTextPlatformLayout,
    position: Point<Pixels>,
) -> ParleyTextPosition {
    record_snapshot_geometry(TextGeometryOperation::PointHit);
    let local_x = f32::from(position.x - cache.bounds.left());
    let local_y = f32::from(position.y - cache.bounds.top());
    let mut position = cache.snapshot.position_for_point(local_x, local_y);
    position.offset =
        normalized_text_range(cache.snapshot.text(), position.offset..position.offset).start;
    position
}

fn normalized_text_range(text: &str, range: Range<usize>) -> Range<usize> {
    let offsets = TextOffsetMap::build(text);
    let range = offsets
        .normalize_internal_range(InternalTextOffset(range.start)..InternalTextOffset(range.end));
    range.start.0..range.end.0
}

#[cfg(test)]
pub(crate) fn test_platform_layout(
    block_id: BlockId,
    content_version: u64,
    text: &str,
    bounds: Bounds<Pixels>,
    table_cell_position: Option<TableCellPosition>,
) -> RichTextPlatformLayout {
    use cditor_core::rich_text::{InlineSpan, RichBlockKind, TextAlign};

    use crate::theme::GuiTheme;
    use crate::text::{
        ParleyLayoutOptions, ParleyLineHeight, ParleyTextStyleConfig, RichTextLayoutInput,
        TextLayoutSurfaceId, build_parley_layout,
    };

    let surface_id = table_cell_position.map_or(TextLayoutSurfaceId::Block(block_id), |position| {
        TextLayoutSurfaceId::TableCell {
            block_id,
            row: position.row,
            column: position.col,
        }
    });
    let input = RichTextLayoutInput {
        block_id,
        surface_id,
        content_version,
        layout_version: 1,
        kind: RichBlockKind::Paragraph,
        text_align: TextAlign::Start,
        spans: vec![InlineSpan::plain(text)],
        width_px: f64::from(f32::from(bounds.size.width)),
        theme_version: 1,
        font_version: 1,
    };
    let snapshot = build_parley_layout(
        &input,
        GuiTheme::light(),
        &ParleyLayoutOptions {
            width: Some(f32::from(bounds.size.width)),
            base_style: ParleyTextStyleConfig {
                line_height: ParleyLineHeight::Absolute(24.0),
                ..ParleyTextStyleConfig::default()
            },
            ..ParleyLayoutOptions::default()
        },
    );
    RichTextPlatformLayout {
        block_id,
        surface_id,
        content_version,
        layout_version: input.layout_version,
        input_session_identity: None,
        snapshot,
        accessibility: None,
        bounds,
        measured_height: f64::from(f32::from(bounds.size.height)),
        table_cell_position,
    }
}

#[cfg(test)]
mod tests {
    use cditor_core::rich_text::{InlineSpan, RichBlockKind, TextAlign};
    use gpui::{point, px, size};

    use super::*;
    use crate::theme::GuiTheme;
    use crate::text::{
        ParleyAlignment, ParleyLayoutOptions, ParleyLineHeight, ParleyTextStyleConfig,
        RichTextLayoutInput, TextLayoutSurfaceId, build_parley_layout,
    };

    #[test]
    fn range_and_point_geometry_round_trip_through_the_same_parley_snapshot() {
        let cache = platform_layout("ab中cd", TextAlign::Start);
        let caret = platform_range_bounds(&cache, 5..5);
        let hit = platform_text_position_for_point(
            &cache,
            point(caret.left(), caret.top() + caret.size.height / 2.0),
        );

        assert_eq!(hit.offset, 5);
        assert!(cache.snapshot.text().is_char_boundary(hit.offset));
    }

    #[test]
    fn invalid_cjk_offsets_are_normalized_before_parley_geometry_queries() {
        let cache = platform_layout("埃塞", TextAlign::Start);

        assert_eq!(normalized_text_range(cache.snapshot.text(), 1..4), 0..6);
        assert_eq!(normalized_text_range(cache.snapshot.text(), 2..2), 0..0);
        let bounds = platform_range_bounds(&cache, 1..4);
        assert!(bounds.size.width > px(0.0));
    }

    #[test]
    fn empty_centered_surface_uses_parley_for_caret_and_ime_bounds() {
        let cache = platform_layout("", TextAlign::Center);
        let bounds = platform_range_bounds(&cache, 0..0);
        let local = cache
            .snapshot
            .caret_rect(ParleyTextPosition::downstream(0), 1.0);

        assert_eq!(bounds.left(), cache.bounds.left() + px(local.x));
        assert_eq!(bounds.top(), cache.bounds.top() + px(local.y));
        assert_eq!(bounds.size.height, px(local.height));
    }

    #[test]
    fn platform_geometry_queries_feed_fallback_rate_telemetry() {
        crate::text::diagnostics::reset_text_geometry_telemetry();
        let cache = platform_layout("telemetry", TextAlign::Start);
        let bounds = platform_range_bounds(&cache, 2..2);
        let _ = platform_text_position_for_point(
            &cache,
            point(bounds.left(), bounds.top() + bounds.size.height / 2.0),
        );

        let stats = crate::text::text_geometry_telemetry();
        assert_eq!(stats.range_bounds_queries, 1);
        assert_eq!(stats.point_hit_queries, 1);
        assert_eq!(stats.synchronous_layout_fallbacks, 0);
        assert_eq!(stats.fallback_rate(), 0.0);
    }

    fn platform_layout(text: &str, text_align: TextAlign) -> RichTextPlatformLayout {
        let input = RichTextLayoutInput {
            block_id: 1,
            surface_id: TextLayoutSurfaceId::Block(1),
            content_version: 1,
            layout_version: 1,
            kind: RichBlockKind::Paragraph,
            text_align,
            spans: vec![InlineSpan::plain(text)],
            width_px: 300.0,
            theme_version: 1,
            font_version: 1,
        };
        let snapshot = build_parley_layout(
            &input,
            GuiTheme::light(),
            &ParleyLayoutOptions {
                width: Some(300.0),
                alignment: ParleyAlignment::from_core(text_align),
                base_style: ParleyTextStyleConfig {
                    line_height: ParleyLineHeight::Absolute(24.0),
                    ..ParleyTextStyleConfig::default()
                },
                ..ParleyLayoutOptions::default()
            },
        );
        RichTextPlatformLayout {
            block_id: 1,
            surface_id: TextLayoutSurfaceId::Block(1),
            content_version: 1,
            layout_version: 1,
            input_session_identity: None,
            snapshot,
            accessibility: None,
            bounds: Bounds::new(point(px(120.0), px(240.0)), size(px(300.0), px(120.0))),
            measured_height: 120.0,
            table_cell_position: None,
        }
    }
}
