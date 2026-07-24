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
pub use element::{RichTextElement, RichTextTypography};
#[cfg(test)]
pub use geometry::TextCaretRect;
pub use geometry::TextHitPoint;
pub use input::RichTextLayoutInput;
pub use parley_adapter::{
    ParleyAccessibilityProjection, ParleyAlignment, ParleyBrush, ParleyFontSlant,
    ParleyInlineBoxSpec, ParleyLayoutOptions, ParleyLayoutSnapshot, ParleyLineHeight,
    ParleyMoveCommand, ParleyPositionedInlineBox, ParleyRect, ParleySelection, ParleySelectionKind,
    ParleyTextPosition, ParleyTextStyleConfig, TextLayoutCacheRequest, TextLayoutSurfaceId,
    accessibility_node_ids, build_parley_accessibility_projection,
    cached_parley_layout_with_request, sync_automatic_text_layout_pins,
};
#[cfg(test)]
pub use parley_adapter::{ParleyInlineBoxKind, build_parley_layout};
#[cfg(test)]
pub(crate) use platform::test_platform_layout;
pub(crate) use platform::{
    RichTextPlatformLayout, TextPlatformLayoutIdentity, platform_index_for_point,
    platform_range_bounds, platform_text_position_for_point,
};
