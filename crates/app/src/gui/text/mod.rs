mod background;
mod diagnostics;
pub mod element;
#[cfg(test)]
#[path = "element_tests.rs"]
mod element_tests;
mod geometry;
pub mod input;
mod parley_adapter;
mod platform;

pub(crate) use diagnostics::{
    TextGeometryOperation, record_snapshot_geometry, record_synchronous_geometry_fallback,
    record_unavailable_geometry, text_geometry_telemetry,
};
pub use element::{ParleyInlineBoxRenderer, RichTextElement, RichTextTypography};
pub use geometry::{TextCaretRect, TextHitPoint};
pub use input::RichTextLayoutInput;
pub use parley_adapter::{
    CachedParleyLayout, ParleyAccessibilityProjection, ParleyAlignment, ParleyBrush,
    ParleyFontSlant, ParleyInlineBoxKind, ParleyInlineBoxSpec, ParleyLayoutKey,
    ParleyLayoutOptions, ParleyLayoutSnapshot, ParleyLineHeight, ParleyLineSnapshot,
    ParleyMoveCommand, ParleyPositionedInlineBox, ParleyRect, ParleySelection, ParleySelectionKind,
    ParleyShapeKey, ParleyStyleRun, ParleyTextIndent, ParleyTextPosition, ParleyTextStyleConfig,
    TextLayoutCacheRequest, TextLayoutSurfaceId, accessibility_node_ids,
    build_parley_accessibility_projection, build_parley_layout, cached_parley_layout,
    cached_parley_layout_with_request, parley_style_runs, sync_automatic_text_layout_pins,
};
#[cfg(test)]
pub(crate) use platform::test_platform_layout;
pub(crate) use platform::{
    RichTextPlatformLayout, TextPlatformLayoutIdentity, platform_index_for_point,
    platform_range_bounds, platform_text_position_for_point,
};
