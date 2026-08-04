use super::super::{
    RichTextLayoutInput, TextLayoutCacheRequest, TextLayoutOptions, TextLayoutSnapshot,
    cached_text_layout_with_request, try_cached_text_layout_with_request,
    try_compatible_text_layout_with_request,
};
use crate::theme::GuiTheme;

pub(super) fn resolve_measured_layout(
    input: &RichTextLayoutInput,
    theme: GuiTheme,
    options: &TextLayoutOptions,
    request: TextLayoutCacheRequest,
    require_prewarmed: bool,
) -> Option<TextLayoutSnapshot> {
    if !require_prewarmed {
        return Some(cached_text_layout_with_request(input, theme, options, request).layout);
    }
    try_cached_text_layout_with_request(input, options, request)
        .or_else(|| {
            try_compatible_text_layout_with_request(input, options, request).filter(|cached| {
                options
                    .width
                    .is_none_or(|width| cached.layout.full_width() <= width + 0.5)
            })
        })
        .map(|cached| cached.layout)
}

#[cfg(test)]
mod tests {
    use cditor_core::rich_text::{InlineSpan, RichBlockKind, TextAlign};

    use super::*;
    use crate::text::{TextLayoutSurfaceId, text_layout_cache_stats};

    fn input() -> RichTextLayoutInput {
        RichTextLayoutInput {
            block_id: 9_900_001,
            surface_id: TextLayoutSurfaceId::Block(9_900_001),
            content_version: 77,
            layout_version: 31,
            kind: RichBlockKind::Paragraph,
            text_align: TextAlign::Start,
            spans: vec![InlineSpan::plain("scheduler-owned shaping")].into(),
            width_px: 420.0,
            theme_version: 41,
            font_version: 43,
        }
    }

    #[test]
    fn prewarmed_mode_never_shapes_on_cache_miss_and_reuses_exact_hit() {
        let input = input();
        let options = TextLayoutOptions {
            width: Some(420.0),
            ..TextLayoutOptions::default()
        };
        let request = TextLayoutCacheRequest::visible();
        let before = text_layout_cache_stats();

        assert!(
            resolve_measured_layout(&input, GuiTheme::light(), &options, request, true).is_none()
        );
        let after_miss = text_layout_cache_stats();
        assert_eq!(before.misses, after_miss.misses);
        assert_eq!(before.reflows, after_miss.reflows);

        cached_text_layout_with_request(&input, GuiTheme::light(), &options, request);
        let resolved = resolve_measured_layout(&input, GuiTheme::light(), &options, request, true)
            .expect("scheduler-populated exact layout should be paintable");
        assert_eq!(resolved.text(), "scheduler-owned shaping");
    }

    #[test]
    fn prewarmed_mode_rejects_compatible_layout_that_exceeds_current_viewport() {
        let input = input();
        let wide = TextLayoutOptions {
            width: Some(420.0),
            ..TextLayoutOptions::default()
        };
        let narrow = TextLayoutOptions {
            width: Some(80.0),
            ..TextLayoutOptions::default()
        };
        let request = TextLayoutCacheRequest::visible();
        let wide_layout =
            cached_text_layout_with_request(&input, GuiTheme::light(), &wide, request).layout;
        assert!(wide_layout.full_width() > 80.0);

        assert!(
            resolve_measured_layout(&input, GuiTheme::light(), &narrow, request, true).is_none(),
            "a wide compatible layout must not paint outside a narrow viewport"
        );

        cached_text_layout_with_request(&input, GuiTheme::light(), &narrow, request);
        let exact = resolve_measured_layout(&input, GuiTheme::light(), &narrow, request, true)
            .expect("the exact narrow layout should become paintable");
        assert!(exact.line_count() > 1);
    }
}
