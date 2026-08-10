mod background;
mod caret_blink;
mod caret_ownership;
mod caret_reveal;
mod diagnostics;
pub mod element;
#[cfg(test)]
#[path = "element_tests.rs"]
mod element_tests;
mod geometry;
pub mod input;
mod layout_adapter;
mod platform;
mod search;
mod segmented_element;
mod segmented_platform;
mod segmented_viewport;

pub use caret_blink::CaretBlink;
pub(crate) use caret_ownership::{platform_text_cursor_ownership, should_paint_custom_caret};
pub(crate) use diagnostics::{
    TextGeometryOperation, record_snapshot_geometry, record_synchronous_geometry_fallback,
    record_unavailable_geometry, text_geometry_telemetry,
};
pub use element::{RichTextElement, RichTextTypography};
#[cfg(test)]
pub use geometry::TextCaretRect;
pub use geometry::TextHitPoint;
pub use input::RichTextLayoutInput;
#[cfg(test)]
pub use layout_adapter::{InlineBoxKind, build_text_layout};
pub use layout_adapter::{
    InlineBoxSpec, PositionedInlineBox, TextAccessibilityProjection, TextAlignment, TextBrush,
    TextFontSlant, TextLayoutCacheRequest, TextLayoutMoveCommand, TextLayoutOptions,
    TextLayoutPosition, TextLayoutRect, TextLayoutSelection, TextLayoutSelectionKind,
    TextLayoutSnapshot, TextLayoutSurfaceId, TextLineHeight, TextStyleConfig,
    accessibility_node_ids, build_text_accessibility_projection, cached_text_layout_with_request,
    sync_automatic_text_layout_pins, text_layout_cache_stats, try_cached_text_layout_with_request,
    try_compatible_text_layout_with_request, try_stale_text_layout_for_surface,
};
#[cfg(test)]
pub(crate) use platform::test_platform_layout;
pub(crate) use platform::{
    RichTextPlatformLayout, TextPlatformLayoutIdentity, platform_range_bounds_at,
    platform_text_position_for_local_point,
};
pub(crate) use search::{TextSearchRange, search_background};
pub(crate) use segmented_element::SegmentedRichTextElement;
pub(crate) use segmented_platform::{
    PlatformTextLayoutSnapshot, SegmentedLayoutFragment, SegmentedPlatformLayout,
};
pub(crate) use segmented_viewport::SegmentedTextViewport;
