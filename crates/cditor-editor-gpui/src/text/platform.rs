use std::ops::Range;

use cditor_core::edit::{InternalTextOffset, TextOffsetMap};
use cditor_core::ids::{BlockId, SurfaceId};
use cditor_core::rich_text::TextAlign;
use cditor_runtime::{InputSessionIdentity, TableCellPosition};
use gpui::{Bounds, Pixels, Point, point, px, size};

use super::{
    PlatformTextLayoutSnapshot, TextAccessibilityProjection, TextGeometryOperation, TextHitPoint,
    TextLayoutPosition, record_snapshot_geometry,
};

pub(crate) struct RichTextPlatformLayout {
    pub block_id: BlockId,
    pub surface_id: SurfaceId,
    pub content_version: u64,
    pub layout_version: u64,
    /// Exact width supplied to the text shaper, in GPUI logical pixels.
    ///
    /// This is deliberately separate from `bounds`: bounds are the most recent
    /// paint placement, while this value controls soft wrapping and hit geometry.
    pub wrap_width_px: f32,
    pub text_align: TextAlign,
    pub input_session_identity: Option<InputSessionIdentity>,
    pub snapshot: PlatformTextLayoutSnapshot,
    pub accessibility: Option<TextAccessibilityProjection>,
    pub bounds: Bounds<Pixels>,
    pub measured_height: f64,
    pub table_cell_position: Option<TableCellPosition>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TextPlatformLayoutIdentity {
    pub surface_id: SurfaceId,
    pub content_version: u64,
    pub layout_version: u64,
    pub wrap_width_bits: u32,
    pub text_align: TextAlign,
}

impl RichTextPlatformLayout {
    pub fn identity(&self) -> TextPlatformLayoutIdentity {
        TextPlatformLayoutIdentity {
            surface_id: self.surface_id,
            content_version: self.content_version,
            layout_version: self.layout_version,
            wrap_width_bits: self.wrap_width_px.to_bits(),
            text_align: self.text_align,
        }
    }

    /// Returns whether this snapshot was shaped under the supplied text
    /// constraints. Both sides are represented as `f32` because GPUI and the
    /// text layout engine shape in logical `f32` pixels.
    pub(crate) fn matches_text_constraints(
        &self,
        wrap_width_px: f64,
        text_align: TextAlign,
    ) -> bool {
        wrap_width_px.is_finite()
            && self.wrap_width_px.is_finite()
            && self.wrap_width_px.to_bits() == (wrap_width_px as f32).to_bits()
            && self.text_align == text_align
    }
}

#[cfg(test)]
pub(crate) fn platform_range_bounds(
    cache: &RichTextPlatformLayout,
    range: Range<usize>,
) -> Bounds<Pixels> {
    platform_range_bounds_at(cache, range, cache.bounds.origin)
}

pub(crate) fn platform_range_bounds_at(
    cache: &RichTextPlatformLayout,
    range: Range<usize>,
    element_origin: Point<Pixels>,
) -> Bounds<Pixels> {
    record_snapshot_geometry(TextGeometryOperation::RangeBounds);
    let range = normalized_text_range(cache.snapshot.text(), range);
    let mut local_rects = if range.is_empty() {
        vec![
            cache
                .snapshot
                .caret_rect(TextLayoutPosition::downstream(range.start), 1.0),
        ]
    } else {
        cache.snapshot.range_rects(range.clone())
    };
    if local_rects.is_empty() {
        local_rects.push(
            cache
                .snapshot
                .caret_rect(TextLayoutPosition::downstream(range.start), 1.0),
        );
    }

    let mut rects = local_rects.into_iter().map(|rect| {
        Bounds::new(
            point(element_origin.x + px(rect.x), element_origin.y + px(rect.y)),
            size(px(rect.width.max(1.0)), px(rect.height.max(0.0))),
        )
    });
    let mut union = rects
        .next()
        .expect("TextLayout range geometry always yields a caret or selection rectangle");
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

#[cfg(test)]
pub(crate) fn platform_text_position_for_point(
    cache: &RichTextPlatformLayout,
    position: Point<Pixels>,
) -> TextLayoutPosition {
    let local_x = f32::from(position.x - cache.bounds.left());
    let local_y = f32::from(position.y - cache.bounds.top());
    platform_text_position_for_local_point(
        cache,
        TextHitPoint {
            x: f64::from(local_x),
            y: f64::from(local_y),
        },
    )
}

pub(crate) fn platform_text_position_for_local_point(
    cache: &RichTextPlatformLayout,
    point: TextHitPoint,
) -> TextLayoutPosition {
    record_snapshot_geometry(TextGeometryOperation::PointHit);
    let mut position = cache
        .snapshot
        .position_for_point(point.x as f32, point.y as f32);
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

    use crate::text::{
        RichTextLayoutInput, TextLayoutOptions, TextLayoutSurfaceId, TextLineHeight,
        TextStyleConfig, build_text_layout,
    };
    use crate::theme::GuiTheme;

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
        spans: vec![InlineSpan::plain(text)].into(),
        width_px: f64::from(f32::from(bounds.size.width)),
        theme_version: 1,
        font_version: 1,
    };
    let snapshot = build_text_layout(
        &input,
        GuiTheme::light(),
        &TextLayoutOptions {
            width: Some(f32::from(bounds.size.width)),
            base_style: TextStyleConfig {
                line_height: TextLineHeight::Absolute(24.0),
                ..TextStyleConfig::default()
            },
            ..TextLayoutOptions::default()
        },
    );
    RichTextPlatformLayout {
        block_id,
        surface_id,
        content_version,
        layout_version: input.layout_version,
        wrap_width_px: f32::from(bounds.size.width),
        text_align: input.text_align,
        input_session_identity: None,
        snapshot: snapshot.into(),
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
    use crate::text::{
        RichTextLayoutInput, TextAlignment, TextLayoutOptions, TextLayoutSurfaceId, TextLineHeight,
        TextStyleConfig, build_text_layout,
    };
    use crate::theme::GuiTheme;

    #[test]
    fn range_and_point_geometry_round_trip_through_the_same_text_layout_snapshot() {
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
    fn local_point_hit_is_independent_of_last_paint_bounds() {
        let first = platform_layout("ab\u{4e2d}cd", TextAlign::Start);
        let mut moved = platform_layout("ab\u{4e2d}cd", TextAlign::Start);
        moved.bounds = Bounds::new(point(px(20_000_000.0), px(20_000_000.0)), moved.bounds.size);
        let caret = first
            .snapshot
            .caret_rect(TextLayoutPosition::downstream(5), 1.0);
        let point = TextHitPoint {
            x: f64::from(caret.x),
            y: f64::from(caret.y + caret.height / 2.0),
        };

        let first_hit = platform_text_position_for_local_point(&first, point);
        let moved_hit = platform_text_position_for_local_point(&moved, point);
        assert_eq!(first_hit, moved_hit);
        assert_eq!(first_hit.offset, 5);
        assert!(first.snapshot.text().is_char_boundary(first_hit.offset));
    }

    #[test]
    fn invalid_cjk_offsets_are_normalized_before_text_layout_geometry_queries() {
        let cache = platform_layout("埃塞", TextAlign::Start);

        assert_eq!(normalized_text_range(cache.snapshot.text(), 1..4), 0..6);
        assert_eq!(normalized_text_range(cache.snapshot.text(), 2..2), 0..0);
        let bounds = platform_range_bounds(&cache, 1..4);
        assert!(bounds.size.width > px(0.0));
    }

    #[test]
    fn platform_layout_identity_includes_wrap_width_and_alignment() {
        let start = platform_layout("soft wrapping text", TextAlign::Start);
        let mut resized = platform_layout("soft wrapping text", TextAlign::Start);
        resized.wrap_width_px = 280.0;
        let mut centered = platform_layout("soft wrapping text", TextAlign::Center);
        centered.wrap_width_px = start.wrap_width_px;

        assert_ne!(start.identity(), resized.identity());
        assert_ne!(start.identity(), centered.identity());
    }

    #[test]
    fn platform_layout_rejects_mismatched_text_constraints() {
        let cache = platform_layout("soft wrapping text", TextAlign::Center);

        assert!(cache.matches_text_constraints(300.0, TextAlign::Center));
        assert!(!cache.matches_text_constraints(280.0, TextAlign::Center));
        assert!(!cache.matches_text_constraints(300.0, TextAlign::End));
    }

    #[test]
    fn empty_centered_surface_uses_text_layout_for_caret_and_ime_bounds() {
        let cache = platform_layout("", TextAlign::Center);
        let bounds = platform_range_bounds(&cache, 0..0);
        let local = cache
            .snapshot
            .caret_rect(TextLayoutPosition::downstream(0), 1.0);

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

    #[test]
    fn platform_geometry_registry_evicts_oldest_unpinned_surface() {
        let mut cache = crate::cache::PlatformGeometryRegistry::new(2);
        let mut first = platform_layout("first", TextAlign::Start);
        first.block_id = 1;
        first.surface_id = TextLayoutSurfaceId::Block(1);
        let mut second = platform_layout("second", TextAlign::Start);
        second.block_id = 2;
        second.surface_id = TextLayoutSurfaceId::Block(2);
        let mut third = platform_layout("third", TextAlign::Start);
        third.block_id = 3;
        third.surface_id = TextLayoutSurfaceId::Block(3);

        cache.insert(1, first, None);
        cache.insert(2, second, None);
        cache.insert(3, third, Some(TextLayoutSurfaceId::Block(1)));

        assert!(cache.contains_key(&1));
        assert!(!cache.contains_key(&2));
        assert!(cache.contains_key(&3));
    }

    #[test]
    fn platform_geometry_registry_does_not_duplicate_text_snapshot_budgeting() {
        let first = platform_layout(&"a".repeat(4_096), TextAlign::Start);
        let mut second = platform_layout(&"b".repeat(4_096), TextAlign::Start);
        second.block_id = 2;
        second.surface_id = TextLayoutSurfaceId::Block(2);
        let mut cache = crate::cache::PlatformGeometryRegistry::new(8);

        cache.insert(1, first, None);
        cache.insert(2, second, None);

        assert_eq!(cache.len(), 2);
        assert!(cache.contains_key(&1));
        assert!(cache.contains_key(&2));
        assert_eq!(
            cache.estimated_metadata_bytes(),
            2 * std::mem::size_of::<RichTextPlatformLayout>()
        );
    }

    fn platform_layout(text: &str, text_align: TextAlign) -> RichTextPlatformLayout {
        let input = RichTextLayoutInput {
            block_id: 1,
            surface_id: TextLayoutSurfaceId::Block(1),
            content_version: 1,
            layout_version: 1,
            kind: RichBlockKind::Paragraph,
            text_align,
            spans: vec![InlineSpan::plain(text)].into(),
            width_px: 300.0,
            theme_version: 1,
            font_version: 1,
        };
        let snapshot = build_text_layout(
            &input,
            GuiTheme::light(),
            &TextLayoutOptions {
                width: Some(300.0),
                alignment: TextAlignment::from_core(text_align),
                base_style: TextStyleConfig {
                    line_height: TextLineHeight::Absolute(24.0),
                    ..TextStyleConfig::default()
                },
                ..TextLayoutOptions::default()
            },
        );
        RichTextPlatformLayout {
            block_id: 1,
            surface_id: TextLayoutSurfaceId::Block(1),
            content_version: 1,
            layout_version: 1,
            wrap_width_px: 300.0,
            text_align,
            input_session_identity: None,
            snapshot: snapshot.into(),
            accessibility: None,
            bounds: Bounds::new(point(px(120.0), px(240.0)), size(px(300.0), px(120.0))),
            measured_height: 120.0,
            table_cell_position: None,
        }
    }
}
