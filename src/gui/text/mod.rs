pub mod element;
pub mod input;
pub mod layout;

pub use element::RichTextElement;
pub(crate) use element::{RichTextPlatformLayout, platform_index_for_point, platform_range_bounds};
pub use input::RichTextLayoutInput;
pub use layout::{
    CachedRichTextLayout, InlineStyle, RichTextLayout, RichTextLayoutCache, RichTextLayoutMetrics,
    TextCaretRect, TextHitPoint, TextLayoutKey, VisualRun, WrappedLine, wrap_rich_text,
};
