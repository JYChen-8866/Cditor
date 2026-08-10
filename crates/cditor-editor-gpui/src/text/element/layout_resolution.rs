use super::super::{
    RichTextLayoutInput, TextLayoutCacheRequest, TextLayoutOptions, TextLayoutSnapshot,
    cached_text_layout_with_request, try_cached_text_layout_with_request,
    try_compatible_text_layout_with_request, try_stale_text_layout_for_surface,
};
use crate::diagnostics::text_layout::{ResolutionOutcome, ResolutionState, trace_resolution};
use crate::text::text_layout_cache_stats;
use crate::theme::GuiTheme;

/// A snapshot usable for this paint pass. `stale` marks a last-resort reuse of
/// a snapshot whose shape identity no longer matches the input (old content or
/// styling): it may be painted, but must not be published as interaction
/// geometry or measured-height feedback.
pub(super) struct MeasuredLayout {
    pub(super) snapshot: TextLayoutSnapshot,
    pub(super) stale: bool,
}

pub(super) fn resolve_measured_layout(
    input: &RichTextLayoutInput,
    theme: GuiTheme,
    options: &TextLayoutOptions,
    request: TextLayoutCacheRequest,
    require_prewarmed: bool,
) -> Option<MeasuredLayout> {
    if !require_prewarmed {
        return Some(MeasuredLayout {
            snapshot: cached_text_layout_with_request(input, theme, options, request).layout,
            stale: false,
        });
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
        return Some(MeasuredLayout {
            snapshot: cached.layout,
            stale: false,
        });
    }
    let compatible = try_compatible_text_layout_with_request(input, options, request);
    let source_width_bits = compatible.as_ref().and_then(|cached| cached.key.width_bits);
    let accepted = compatible.filter(|cached| {
        cached.key.alignment == options.alignment
            && wrap_widths_are_compatible(cached.key.width_bits, requested_width_bits)
    });
    if let Some(accepted) = accepted {
        trace_resolution(
            input.surface_id,
            ResolutionState::new(
                requested_width_bits,
                source_width_bits,
                ResolutionOutcome::CompatibleAccepted,
                text_layout_cache_stats(),
            ),
        );
        return Some(MeasuredLayout {
            snapshot: accepted.layout,
            stale: false,
        });
    }
    // Last resort: the newest snapshot for this surface, whatever its shape
    // identity. Content or styling may lag by a frame (a code block whose
    // async highlight just landed, an edited block whose re-shape is queued),
    // which is strictly better than flashing skeleton bars over text the user
    // is reading. The caller has already enqueued the real shape; the next
    // admitted frame converges. Geometry must still match, or the stale text
    // would paint at the wrong wrap width.
    let stale = try_stale_text_layout_for_surface(input, options, request).filter(|cached| {
        cached.key.alignment == options.alignment
            && wrap_widths_are_compatible(cached.key.width_bits, requested_width_bits)
    });
    if stale.is_none() && crate::diagnostics::flash::enabled() {
        let diagnosis =
            cditor_text::diagnose_text_layout_miss(&input.to_text_layout_input(), options);
        crate::diagnostics::flash::trace(
            "text.resolve-miss",
            format_args!(
                "surface={:?} content_v={} layout_v={} requested_width={:?} reason={:?} newest_same_surface_width={:?} newest_alignment={:?}",
                input.surface_id,
                input.content_version,
                input.layout_version,
                options.width,
                diagnosis.reason,
                diagnosis.newest_same_surface_width,
                diagnosis.newest_same_surface_alignment,
            ),
        );
    }
    trace_resolution(
        input.surface_id,
        ResolutionState::new(
            requested_width_bits,
            source_width_bits,
            if stale.is_some() {
                ResolutionOutcome::StaleAccepted
            } else if source_width_bits.is_some() {
                ResolutionOutcome::CompatibleRejected
            } else {
                ResolutionOutcome::Missing
            },
            text_layout_cache_stats(),
        ),
    );
    stale.map(|cached| MeasuredLayout {
        snapshot: cached.layout,
        stale: true,
    })
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
        assert!(!resolved.stale);
        assert_eq!(resolved.snapshot.text(), "scheduler-owned shaping");
    }

    #[test]
    fn prewarmed_mode_paints_a_stale_snapshot_while_a_reshape_is_pending() {
        let input = input();
        let options = TextLayoutOptions {
            width: Some(420.0),
            ..TextLayoutOptions::default()
        };
        let request = TextLayoutCacheRequest::visible();
        cached_text_layout_with_request(&input, GuiTheme::light(), &options, request);

        // The block was edited (or its async syntax highlight landed): the
        // shape identity changes while the re-shape sits in the scheduler
        // queue. The previous snapshot substitutes instead of skeleton bars.
        let mut edited = super::tests::input();
        edited.content_version = 78;
        let resolved = resolve_measured_layout(&edited, GuiTheme::light(), &options, request, true)
            .expect("the previous snapshot substitutes while the re-shape is pending");
        assert!(resolved.stale);
        assert_eq!(resolved.snapshot.text(), "scheduler-owned shaping");

        // A surface with no history still paints nothing: skeleton bars are
        // reserved for true cold starts.
        let mut cold = super::tests::input();
        cold.block_id = 9_900_002;
        cold.surface_id = TextLayoutSurfaceId::Block(9_900_002);
        assert!(
            resolve_measured_layout(&cold, GuiTheme::light(), &options, request, true).is_none()
        );
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
        assert!(exact.snapshot.line_count() > 1);
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
