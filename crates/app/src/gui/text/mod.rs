mod background;
pub mod element;
#[cfg(test)]
#[path = "element_tests.rs"]
mod element_tests;
mod fallback_render;
pub mod input;
pub mod layout;
mod platform;

pub use element::RichTextElement;
pub use input::RichTextLayoutInput;
pub use layout::{
    CachedRichTextLayout, InlineStyle, RichTextLayout, RichTextLayoutCache, RichTextLayoutMetrics,
    TextCaretRect, TextHitPoint, TextLayoutKey, VisualRun, WrappedLine, wrap_rich_text,
};
pub(crate) use platform::{
    RichTextPlatformLayout, normalized_text_range, platform_cursor_bounds_for_offset,
    platform_index_for_point, platform_range_bounds, platform_range_segment_bounds,
};
