use super::super::{
    RichTextLayoutInput, TextLayoutCacheRequest, TextLayoutOptions, TextLayoutSnapshot,
    cached_text_layout_with_request, try_cached_text_layout_with_request,
    try_compatible_text_layout_with_request,
};
use crate::diagnostics::text_layout::{ResolutionOutcome, ResolutionState, trace_resolution};
use crate::text::text_layout_cache_stats;
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
    let requested_width_bits = options.width.map(f32::to_bits);
    if let Some(cached) = try_cached_text_layout_with_request(input, options, request) {
        trace_resolution(
            input.surface_id,
            ResolutionState::new(
                requested_width_bits,
                cached.key.width_bits,
                ResolutionOutcome::Exact,
                text_layout_cache_stats(),
            ),
        );
        return Some(cached.layout);
    }
    let compatible = try_compatible_text_layout_with_request(input, options, request);
    let source_width_bits = compatible.as_ref().and_then(|cached| cached.key.width_bits);
    let accepted = compatible.filter(|cached| {
        cached.key.alignment == options.alignment
            && wrap_widths_are_compatible(cached.key.width_bits, requested_width_bits)
    });
    trace_resolution(
        input.surface_id,
        ResolutionState::new(
            requested_width_bits,
            source_width_bits,
            if accepted.is_some() {
                ResolutionOutcome::CompatibleAccepted
            } else if source_width_bits.is_some() {
                ResolutionOutcome::CompatibleRejected
            } else {
                ResolutionOutcome::Missing
            },
            text_layout_cache_stats(),
        ),
    );
    accepted.map(|cached| cached.layout)
}

fn wrap_widths_are_compatible(source: Option<u32>, requested: Option<u32>) -> bool {
    match (source.map(f32::from_bits), requested.map(f32::from_bits)) {
        (None, None) => true,
        (Some(source), Some(requested)) => {
            source.is_finite() && requested.is_finite() && (source - requested).abs() <= 1.0
        }
        _ => false,
    }
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
    fn prewarmed_mode_rejects_compatible_layout_with_a_different_wrap_width() {
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

        assert!(
            resolve_measured_layout(&input, GuiTheme::light(), &wide, request, true).is_some(),
            "the exact wide layout remains paintable"
        );

        cached_text_layout_with_request(&input, GuiTheme::light(), &narrow, request);
        let exact = resolve_measured_layout(&input, GuiTheme::light(), &narrow, request, true)
            .expect("the exact narrow layout should become paintable");
        assert!(exact.line_count() > 1);
    }

    #[test]
    fn prewarmed_mode_accepts_one_pixel_measurement_rounding() {
        let input = input();
        let projected = TextLayoutOptions {
            width: Some(420.0),
            ..TextLayoutOptions::default()
        };
        let measured = TextLayoutOptions {
            width: Some(419.0),
            ..TextLayoutOptions::default()
        };
        let request = TextLayoutCacheRequest::visible();
        cached_text_layout_with_request(&input, GuiTheme::light(), &projected, request);

        assert!(
            resolve_measured_layout(&input, GuiTheme::light(), &measured, request, true).is_some(),
            "one-pixel GPUI measurement rounding should reuse the projected layout"
        );
    }

    #[test]
    fn prewarmed_mode_does_not_stretch_a_pathologically_narrow_layout() {
        let input = input();
        let narrow = TextLayoutOptions {
            width: Some(1.0),
            ..TextLayoutOptions::default()
        };
        let wide = TextLayoutOptions {
            width: Some(420.0),
            ..TextLayoutOptions::default()
        };
        let request = TextLayoutCacheRequest::visible();
        let narrow_layout =
            cached_text_layout_with_request(&input, GuiTheme::light(), &narrow, request).layout;
        assert!(narrow_layout.line_count() > 1);

        assert!(
            resolve_measured_layout(&input, GuiTheme::light(), &wide, request, true).is_none(),
            "a transient one-pixel layout must never be painted after the editor widens"
        );
    }
}
