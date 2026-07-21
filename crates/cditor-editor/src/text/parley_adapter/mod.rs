mod exact_raster;
mod paint;

use cditor_core::rich_text::{InlineSpan, RichBlockKind};

use crate::text::RichTextLayoutInput;
use crate::theme::GuiTheme;

pub use cditor_text::{
    CachedParleyLayout, ParleyAccessibilityProjection, ParleyAlignment, ParleyBrush,
    ParleyFontSlant, ParleyInlineBoxKind, ParleyInlineBoxSpec, ParleyLayoutKey,
    ParleyLayoutOptions, ParleyLayoutSnapshot, ParleyLineHeight, ParleyLineSnapshot,
    ParleyMoveCommand, ParleyPositionedInlineBox, ParleyRect, ParleySelection, ParleySelectionKind,
    ParleyShapeKey, ParleyStyleRun, ParleyTextIndent, ParleyTextPosition, ParleyTextStyleConfig,
    TextLayoutCacheRequest, TextLayoutSurfaceId, accessibility_node_ids,
    build_parley_accessibility_projection, sync_automatic_text_layout_pins,
};
pub(crate) use paint::{paint_parley_layout, parley_background_quads};

pub fn build_parley_layout(
    input: &RichTextLayoutInput,
    theme: GuiTheme,
    options: &ParleyLayoutOptions,
) -> ParleyLayoutSnapshot {
    cditor_text::build_parley_layout(&input.to_text_layout_input(), text_theme(theme), options)
}

pub fn cached_parley_layout(
    input: &RichTextLayoutInput,
    theme: GuiTheme,
    options: &ParleyLayoutOptions,
) -> CachedParleyLayout {
    cditor_text::cached_parley_layout(&input.to_text_layout_input(), text_theme(theme), options)
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

pub fn parley_style_runs(
    spans: &[InlineSpan],
    kind: &RichBlockKind,
    theme: GuiTheme,
    base_text_color: u32,
    base_style: &ParleyTextStyleConfig,
    mono_font_family: &str,
) -> Vec<ParleyStyleRun> {
    cditor_text::parley_style_runs(
        spans,
        kind,
        text_theme(theme),
        base_text_color,
        base_style,
        mono_font_family,
    )
}

fn text_theme(theme: GuiTheme) -> cditor_text::TextTheme {
    cditor_text::TextTheme {
        link_text: theme.focused,
        inline_code_text: theme.inline_code_text,
        inline_code_background: theme.inline_code_background,
    }
}
