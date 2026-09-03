mod exact_raster;
mod paint;

pub(crate) use exact_raster::{exact_raster_cache_stats, trim_exact_raster_cache};

use crate::text::RichTextLayoutInput;
use crate::theme::GuiTheme;

pub use cditor_text::{
    CachedTextLayout, InlineBoxKind, InlineBoxSpec, PositionedInlineBox,
    TextAccessibilityProjection, TextAlignment, TextBrush, TextFontSlant, TextLayoutCacheRequest,
    TextLayoutMoveCommand, TextLayoutOptions, TextLayoutPosition, TextLayoutRect,
    TextLayoutSelection, TextLayoutSelectionKind, TextLayoutSnapshot, TextLayoutSurfaceId,
    TextLineHeight, TextStyleConfig, accessibility_node_ids, build_text_accessibility_projection,
    sync_automatic_text_layout_pins, text_layout_cache_stats,
};
pub(crate) use paint::{paint_text_layout, text_background_quads};

#[cfg(test)]
pub fn build_text_layout(
    input: &RichTextLayoutInput,
    theme: GuiTheme,
    options: &TextLayoutOptions,
) -> TextLayoutSnapshot {
    cditor_text::build_text_layout(&input.to_text_layout_input(), text_theme(theme), options)
}

pub fn cached_text_layout_with_request(
    input: &RichTextLayoutInput,
    theme: GuiTheme,
    options: &TextLayoutOptions,
    request: TextLayoutCacheRequest,
) -> CachedTextLayout {
    cditor_text::cached_text_layout_with_request(
        &input.to_text_layout_input(),
        text_theme(theme),
        options,
        request,
    )
}

pub fn try_cached_text_layout_with_request(
    input: &RichTextLayoutInput,
    options: &TextLayoutOptions,
    request: TextLayoutCacheRequest,
) -> Option<CachedTextLayout> {
    cditor_text::try_cached_text_layout_with_request(
        &input.to_text_layout_input(),
        options,
        request,
    )
}

pub fn try_compatible_text_layout_with_request(
    input: &RichTextLayoutInput,
    options: &TextLayoutOptions,
    request: TextLayoutCacheRequest,
) -> Option<CachedTextLayout> {
    cditor_text::try_compatible_text_layout_with_request(
        &input.to_text_layout_input(),
        options,
        request,
    )
}

pub fn try_stale_text_layout_for_surface(
    input: &RichTextLayoutInput,
    options: &TextLayoutOptions,
    request: TextLayoutCacheRequest,
) -> Option<CachedTextLayout> {
    cditor_text::try_stale_text_layout_for_surface(&input.to_text_layout_input(), options, request)
}

fn text_theme(theme: GuiTheme) -> cditor_text::TextTheme {
    cditor_text::TextTheme {
        link_text: theme.inline_code_text,
        document_link_text: theme.document_link,
        inline_code_text: theme.inline_code_text,
        inline_code_background: theme.inline_code_background,
    }
}
