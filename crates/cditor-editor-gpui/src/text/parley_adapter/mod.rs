mod exact_raster;
mod paint;

use crate::text::RichTextLayoutInput;
use crate::theme::GuiTheme;

#[cfg(test)]
pub use cditor_text::ParleyInlineBoxKind;
pub use cditor_text::{
    CachedParleyLayout, ParleyAccessibilityProjection, ParleyAlignment, ParleyBrush,
    ParleyFontSlant, ParleyInlineBoxSpec, ParleyLayoutOptions, ParleyLayoutSnapshot,
    ParleyLineHeight, ParleyMoveCommand, ParleyPositionedInlineBox, ParleyRect, ParleySelection,
    ParleySelectionKind, ParleyTextPosition, ParleyTextStyleConfig, TextLayoutCacheRequest,
    TextLayoutSurfaceId, accessibility_node_ids, build_parley_accessibility_projection,
    sync_automatic_text_layout_pins,
};
pub(crate) use paint::{paint_parley_layout, parley_background_quads};

#[cfg(test)]
pub fn build_parley_layout(
    input: &RichTextLayoutInput,
    theme: GuiTheme,
    options: &ParleyLayoutOptions,
) -> ParleyLayoutSnapshot {
    cditor_text::build_parley_layout(&input.to_text_layout_input(), text_theme(theme), options)
}

pub fn cached_parley_layout_with_request(
    input: &RichTextLayoutInput,
    theme: GuiTheme,
    options: &ParleyLayoutOptions,
    request: TextLayoutCacheRequest,
) -> CachedParleyLayout {
    cditor_text::cached_parley_layout_with_request(
        &input.to_text_layout_input(),
        text_theme(theme),
        options,
        request,
    )
}

fn text_theme(theme: GuiTheme) -> cditor_text::TextTheme {
    cditor_text::TextTheme {
        link_text: theme.focused,
        inline_code_text: theme.inline_code_text,
        inline_code_background: theme.inline_code_background,
    }
}
