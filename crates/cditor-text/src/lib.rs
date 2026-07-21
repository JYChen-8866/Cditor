//! Framework-independent text shaping and geometry for Cditor.
//!
//! This is the workspace's only direct Parley integration. GPUI painting and
//! native input remain adapters in `cditor-app`.

mod accessibility;
mod cache;
mod engine;
mod font_identity;
mod geometry;
mod input;
mod paint_plan;
mod segmented;
mod snapshot;
mod style;
mod text_snapshot;
mod theme;

#[cfg(test)]
mod tests;

pub use accessibility::{
    ParleyAccessibilityProjection, accessibility_node_ids, build_parley_accessibility_projection,
};
pub use cache::{
    CachedParleyLayout, CachedTextLayout, ParleyLayoutKey, ParleyShapeKey, TextLayoutCachePolicy,
    TextLayoutCachePriority, TextLayoutCacheRequest, TextLayoutCacheStats,
    TextLayoutCacheTrimReport, TextLayoutKey, TextLayoutMemoryPressure, TextRelayoutFallbackReason,
    TextRelayoutStrategy, TextShapeKey, apply_text_layout_memory_pressure, cached_parley_layout,
    cached_parley_layout_with_request, set_text_layout_cache_policy, set_text_layout_surface_pin,
    sync_automatic_text_layout_pins, text_layout_cache_stats,
};
pub use engine::{
    FontRegistrationError, ParleyAlignment, ParleyInlineBoxKind, ParleyInlineBoxSpec,
    ParleyLayoutOptions, ParleyTextIndent, RegisteredFontFamily, build_parley_layout,
    register_font_data,
};
pub use font_identity::{
    FontBlobDigest, FontFaceKey, FontInstanceKey, FontSynthesisKey, FontVariationSettingKey,
};
pub use geometry::{
    ParleyMoveCommand, ParleyRect, ParleySelection, ParleySelectionKind, ParleyTextPosition,
};
pub use input::{TextLayoutInput, TextLayoutSurfaceId};
pub use paint_plan::{
    ParleyPaintBackground, ParleyPaintDecoration, ParleyPaintFont, ParleyPaintFontStyle,
    ParleyPaintGlyph, ParleyPaintPlan, ParleyPaintRun,
};
pub use segmented::{
    LARGE_TEXT_SEGMENTATION_THRESHOLD_BYTES, SegmentScrollAnchor, SegmentedLayoutConfig,
    SegmentedTextLayout, requires_segmentation,
};
pub use snapshot::{
    ParleyLayoutSnapshot, ParleyLineSnapshot, ParleyPositionedInlineBox, PositionedInlineBox,
    TextClusterSnapshot, TextLayoutSnapshot, TextLineSnapshot,
};
pub use style::{
    ParleyBrush, ParleyFontSlant, ParleyLineHeight, ParleyStyleRun, ParleyTextStyleConfig,
    TextStyleRun, parley_style_runs,
};
pub use text_snapshot::TextSnapshot;
pub use theme::TextTheme;
