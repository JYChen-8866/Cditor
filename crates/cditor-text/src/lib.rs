//! Framework-independent text shaping and geometry for Cditor.
//!
//! This is the workspace's only direct Parley integration. GPUI painting and
//! native input remain adapters in `cditor-editor-gpui`.

mod accessibility;
mod bundled_fonts;
mod cache;
mod engine;
mod font_identity;
mod geometry;
mod geometry_snapshot;
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
    TextAccessibilityProjection, accessibility_node_ids, build_text_accessibility_projection,
};
pub use bundled_fonts::{
    DOCUMENT_BODY_FONT_FAMILY, document_body_font_family, set_document_body_font_family,
};
pub use cache::{
    CachedTextLayout, TextLayoutCachePolicy, TextLayoutCachePriority, TextLayoutCacheRequest,
    TextLayoutCacheStats, TextLayoutCacheTrimReport, TextLayoutKey, TextLayoutMemoryPressure,
    TextRelayoutFallbackReason, TextRelayoutStrategy, TextShapeKey,
    apply_text_layout_memory_pressure, cached_text_layout, cached_text_layout_with_request,
    set_text_layout_cache_policy, set_text_layout_surface_pin, sync_automatic_text_layout_pins,
    text_layout_cache_stats, try_cached_text_layout_with_request,
    try_compatible_text_layout_with_request,
};
pub use engine::{
    FontRegistrationError, InlineBoxKind, InlineBoxSpec, RegisteredFontFamily, TextAlignment,
    TextIndent, TextLayoutOptions, build_text_layout, register_font_data,
};
pub use font_identity::{
    FontBlobDigest, FontFaceKey, FontInstanceKey, FontSynthesisKey, FontVariationSettingKey,
};
pub use geometry::{
    TextLayoutMoveCommand, TextLayoutPosition, TextLayoutRect, TextLayoutSelection,
    TextLayoutSelectionKind,
};
pub use geometry_snapshot::{TextCaretStop, TextGeometrySnapshot};
pub use input::{TextLayoutInput, TextLayoutSurfaceId};
pub use paint_plan::{
    TextPaintBackground, TextPaintDecoration, TextPaintFont, TextPaintFontStyle, TextPaintGlyph,
    TextPaintPlan, TextPaintRun,
};
pub use segmented::{
    LARGE_TEXT_SEGMENTATION_THRESHOLD_BYTES, SegmentScrollAnchor, SegmentedLayoutConfig,
    SegmentedTextLayout, requires_segmentation,
};
pub use snapshot::{
    PositionedInlineBox, TextClusterSnapshot, TextLayoutSnapshot, TextLineSnapshot,
};
pub use style::{
    NOTION_BOLD_FONT_WEIGHT, TextBrush, TextFontSlant, TextLineHeight, TextStyleConfig,
    TextStyleRun, text_style_runs,
};
pub use text_snapshot::TextSnapshot;
pub use theme::TextTheme;
