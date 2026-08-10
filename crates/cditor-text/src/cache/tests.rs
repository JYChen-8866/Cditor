use cditor_core::rich_text::{InlineMark, InlineSpan, RichBlockKind, TextAlign};

use super::*;
use crate::{InlineBoxKind, InlineBoxSpec, TextLineHeight, TextStyleConfig};

const TEST_BYTES_BUDGET: usize = 16 * 1024 * 1024;

fn theme() -> TextTheme {
    TextTheme {
        link_text: 0x2383e2,
        document_link_text: 0x9065b0,
        inline_code_text: 0xeb5757,
        inline_code_background: 0xf1f1ef,
    }
}

fn input(surface_id: TextLayoutSurfaceId, text: &str) -> TextLayoutInput {
    TextLayoutInput {
        surface_id,
        content_version: 1,
        layout_version: 1,
        kind: RichBlockKind::Paragraph,
        text_align: TextAlign::Start,
        spans: vec![InlineSpan::plain(text)],
        width_px: 200.0,
        theme_version: 1,
        font_version: 1,
    }
}

fn options(width: f32) -> TextLayoutOptions {
    TextLayoutOptions {
        width: Some(width),
        base_text_color: 0x37352f,
        base_style: TextStyleConfig {
            line_height: TextLineHeight::Absolute(24.0),
            ..TextStyleConfig::default()
        },
        ..TextLayoutOptions::default()
    }
}

fn request(priority: TextLayoutCachePriority, pin_surface: bool) -> TextLayoutCacheRequest {
    TextLayoutCacheRequest {
        priority,
        pin_surface,
    }
}

fn contains_surface(surface_id: TextLayoutSurfaceId) -> bool {
    TEXT_LAYOUT_CACHE.with(|cache| {
        cache
            .borrow()
            .entries
            .keys()
            .any(|key| key.shape.surface_id == surface_id)
    })
}

#[test]
fn surface_identity_distinguishes_table_cells_with_identical_text_and_versions() {
    reset_text_layout_cache_for_tests();
    let first = input(
        TextLayoutSurfaceId::TableCell {
            block_id: 7,
            row: 0,
            column: 0,
        },
        "same",
    );
    let second = input(
        TextLayoutSurfaceId::TableCell {
            block_id: 7,
            row: 0,
            column: 1,
        },
        "same",
    );

    let first_key = TextLayoutKey::from_input(&first, &options(200.0));
    let second_key = TextLayoutKey::from_input(&second, &options(200.0));

    assert_ne!(first_key, second_key);
}

#[test]
fn cache_probe_never_shapes_or_counts_a_miss() {
    reset_text_layout_cache_for_tests();
    let input = input(TextLayoutSurfaceId::Block(11), "probe only");
    let options = options(200.0);

    assert!(
        try_cached_text_layout_with_request(&input, &options, TextLayoutCacheRequest::visible())
            .is_none()
    );
    let empty_stats = text_layout_cache_stats();
    assert_eq!(empty_stats.entries, 0);
    assert_eq!(empty_stats.misses, 0);

    cached_text_layout(&input, theme(), &options);
    let cached =
        try_cached_text_layout_with_request(&input, &options, TextLayoutCacheRequest::visible())
            .expect("a populated exact key should be returned");
    assert!(cached.cache_hit);
    assert_eq!(cached.layout.text(), "probe only");
}

#[test]
fn compatible_probe_reuses_same_shape_without_reflow_or_miss_telemetry() {
    reset_text_layout_cache_for_tests();
    let input = input(TextLayoutSurfaceId::Block(12), "same shaped text");
    cached_text_layout(&input, theme(), &options(240.0));
    let before = text_layout_cache_stats();

    let fallback = try_compatible_text_layout_with_request(
        &input,
        &options(120.0),
        TextLayoutCacheRequest::visible(),
    )
    .expect("same shape at an old width is a valid transient fallback");
    let after = text_layout_cache_stats();

    assert_eq!(fallback.layout.text(), "same shaped text");
    assert_eq!(fallback.key.width_bits, Some(240.0_f32.to_bits()));
    assert_eq!(before.misses, after.misses);
    assert_eq!(before.reflows, after.reflows);
    assert_eq!(after.entries, 1);
}

#[test]
fn lru_evicts_the_oldest_entry_within_the_lowest_priority() {
    reset_text_layout_cache_for_tests();
    set_text_layout_cache_policy(TextLayoutCachePolicy::new(2, TEST_BYTES_BUDGET));
    let first_surface = TextLayoutSurfaceId::Block(1);
    let second_surface = TextLayoutSurfaceId::Block(2);
    let third_surface = TextLayoutSurfaceId::Block(3);
    let first = input(first_surface, "first");
    let second = input(second_surface, "second");

    cached_text_layout_with_request(
        &first,
        theme(),
        &options(200.0),
        request(TextLayoutCachePriority::Offscreen, false),
    );
    cached_text_layout_with_request(
        &second,
        theme(),
        &options(200.0),
        request(TextLayoutCachePriority::Offscreen, false),
    );
    cached_text_layout_with_request(
        &first,
        theme(),
        &options(200.0),
        request(TextLayoutCachePriority::Offscreen, false),
    );
    cached_text_layout_with_request(
        &input(third_surface, "third"),
        theme(),
        &options(200.0),
        request(TextLayoutCachePriority::Offscreen, false),
    );

    assert!(contains_surface(first_surface));
    assert!(!contains_surface(second_surface));
    assert!(contains_surface(third_surface));
}

#[test]
fn entry_and_byte_budgets_are_both_enforced() {
    reset_text_layout_cache_for_tests();
    set_text_layout_cache_policy(TextLayoutCachePolicy::new(10, 1));

    let result = cached_text_layout(
        &input(TextLayoutSurfaceId::Block(1), "larger than one byte"),
        theme(),
        &options(200.0),
    );
    let stats = text_layout_cache_stats();

    assert!(result.estimated_bytes > 1);
    assert_eq!(stats.entries, 0);
    assert_eq!(stats.estimated_bytes, 0);
    assert_eq!(stats.evictions, 1);
}

#[test]
fn editing_pin_survives_capacity_pressure_and_visible_request_releases_it() {
    reset_text_layout_cache_for_tests();
    set_text_layout_cache_policy(TextLayoutCachePolicy::new(1, TEST_BYTES_BUDGET));
    let editing_surface = TextLayoutSurfaceId::Block(1);
    let editing = input(editing_surface, "editing");

    cached_text_layout_with_request(
        &editing,
        theme(),
        &options(200.0),
        TextLayoutCacheRequest::editing(),
    );
    cached_text_layout(
        &input(TextLayoutSurfaceId::Block(2), "visible"),
        theme(),
        &options(200.0),
    );

    assert!(contains_surface(editing_surface));
    assert_eq!(text_layout_cache_stats().pinned_entries, 1);

    let visible_result = cached_text_layout(&editing, theme(), &options(200.0));
    assert!(visible_result.cache_hit);
    let report = apply_text_layout_memory_pressure(TextLayoutMemoryPressure::Critical);
    assert_eq!(report.remaining_entries, 0);
    assert!(!report.over_budget_due_to_pins);
}

#[test]
fn warning_pressure_evicts_offscreen_then_overscan_before_visible_and_editing() {
    reset_text_layout_cache_for_tests();
    set_text_layout_cache_policy(TextLayoutCachePolicy::new(4, TEST_BYTES_BUDGET));
    let fixtures = [
        (1, TextLayoutCachePriority::Offscreen),
        (2, TextLayoutCachePriority::Overscan),
        (3, TextLayoutCachePriority::Visible),
        (4, TextLayoutCachePriority::Editing),
    ];
    for (block_id, priority) in fixtures {
        cached_text_layout_with_request(
            &input(TextLayoutSurfaceId::Block(block_id), "priority"),
            theme(),
            &options(200.0),
            request(priority, false),
        );
    }

    let report = apply_text_layout_memory_pressure(TextLayoutMemoryPressure::Warning);

    assert_eq!(report.remaining_entries, 2);
    assert!(!contains_surface(TextLayoutSurfaceId::Block(1)));
    assert!(!contains_surface(TextLayoutSurfaceId::Block(2)));
    assert!(contains_surface(TextLayoutSurfaceId::Block(3)));
    assert!(contains_surface(TextLayoutSurfaceId::Block(4)));
}

#[test]
fn critical_pressure_preserves_explicit_pins_and_reports_over_budget() {
    reset_text_layout_cache_for_tests();
    set_text_layout_cache_policy(TextLayoutCachePolicy::new(4, TEST_BYTES_BUDGET));
    let pinned_surface = TextLayoutSurfaceId::Block(1);
    let other_surface = TextLayoutSurfaceId::Block(2);
    cached_text_layout(&input(pinned_surface, "pinned"), theme(), &options(200.0));
    cached_text_layout(&input(other_surface, "other"), theme(), &options(200.0));
    set_text_layout_surface_pin(pinned_surface, true);

    let report = apply_text_layout_memory_pressure(TextLayoutMemoryPressure::Critical);

    assert!(contains_surface(pinned_surface));
    assert!(!contains_surface(other_surface));
    assert_eq!(report.remaining_entries, 1);
    assert!(report.over_budget_due_to_pins);
    set_text_layout_surface_pin(pinned_surface, false);
}

#[test]
fn frame_pin_sync_releases_an_offscreen_surface_after_focus_moves() {
    reset_text_layout_cache_for_tests();
    set_text_layout_cache_policy(TextLayoutCachePolicy::new(4, TEST_BYTES_BUDGET));
    let previous_surface = TextLayoutSurfaceId::Block(1);
    cached_text_layout_with_request(
        &input(previous_surface, "previous focus"),
        theme(),
        &options(200.0),
        TextLayoutCacheRequest::editing(),
    );
    sync_automatic_text_layout_pins(&[previous_surface]);

    sync_automatic_text_layout_pins(&[TextLayoutSurfaceId::Block(2)]);
    let report = apply_text_layout_memory_pressure(TextLayoutMemoryPressure::Critical);

    assert_eq!(report.remaining_entries, 0);
    assert!(!contains_surface(previous_surface));
}

#[test]
fn stats_distinguish_hits_misses_reflows_and_evictions() {
    reset_text_layout_cache_for_tests();
    set_text_layout_cache_policy(TextLayoutCachePolicy::new(2, TEST_BYTES_BUDGET));
    let input = input(
        TextLayoutSurfaceId::Block(1),
        "wrap this text over several words",
    );

    let first = cached_text_layout(&input, theme(), &options(80.0));
    let second = cached_text_layout(&input, theme(), &options(80.0));
    let reflowed = cached_text_layout(&input, theme(), &options(400.0));
    let stats = text_layout_cache_stats();

    assert_eq!(
        first.strategy,
        TextRelayoutStrategy::FullBuild(TextRelayoutFallbackReason::NoPreviousSnapshot)
    );
    assert_eq!(second.strategy, TextRelayoutStrategy::CacheHit);
    assert_eq!(reflowed.strategy, TextRelayoutStrategy::Reflow);
    assert_eq!(stats.hits, 1);
    assert_eq!(stats.misses, 2);
    assert_eq!(stats.reflows, 1);
    assert_eq!(stats.entries, 1);
    assert!(stats.estimated_bytes > 0);
}

#[test]
fn rapid_width_changes_keep_only_the_current_geometry_per_surface() {
    reset_text_layout_cache_for_tests();
    let input = input(TextLayoutSurfaceId::Block(81), "resize cache convergence");

    for width in 100..400 {
        cached_text_layout(&input, theme(), &options(width as f32));
    }

    let stats = text_layout_cache_stats();
    assert_eq!(stats.entries, 1);
    assert!(stats.evictions >= 299);
    assert!(
        try_cached_text_layout_with_request(
            &input,
            &options(399.0),
            TextLayoutCacheRequest::visible(),
        )
        .is_some()
    );
    assert!(
        try_cached_text_layout_with_request(
            &input,
            &options(398.0),
            TextLayoutCacheRequest::visible(),
        )
        .is_none()
    );
    assert!(
        try_cached_text_layout_with_request(
            &input,
            &options(397.0),
            TextLayoutCacheRequest::visible(),
        )
        .is_none()
    );
}

#[test]
fn pinned_surface_does_not_retain_obsolete_resize_history() {
    reset_text_layout_cache_for_tests();
    let input = input(TextLayoutSurfaceId::Block(82), "pinned resize cache");

    for width in 100..400 {
        cached_text_layout_with_request(
            &input,
            theme(),
            &options(width as f32),
            TextLayoutCacheRequest::editing(),
        );
    }

    let stats = text_layout_cache_stats();
    assert_eq!(stats.entries, 1);
    assert_eq!(stats.pinned_entries, 1);
    assert!(!stats.over_budget_due_to_pins);
}

#[test]
fn new_text_identity_replaces_old_history_for_the_same_surface() {
    reset_text_layout_cache_for_tests();
    let mut input = input(TextLayoutSurfaceId::Block(83), "old content");
    cached_text_layout(&input, theme(), &options(200.0));
    cached_text_layout(&input, theme(), &options(201.0));
    input.content_version += 1;
    input.spans = vec![InlineSpan::plain("new content")];

    cached_text_layout(&input, theme(), &options(202.0));

    assert_eq!(text_layout_cache_stats().entries, 1);
}

#[test]
fn layout_generation_change_reflows_without_reshaping() {
    reset_text_layout_cache_for_tests();
    let base = input(TextLayoutSurfaceId::Block(1), "same shape");
    cached_text_layout(&base, theme(), &options(200.0));
    let mut changed = base.clone();
    changed.layout_version += 1;

    let result = cached_text_layout(&changed, theme(), &options(200.0));

    assert_eq!(result.strategy, TextRelayoutStrategy::Reflow);
    assert!(result.reflowed);
}

#[test]
fn focused_relayout_does_not_invalidate_one_hundred_visible_surfaces() {
    reset_text_layout_cache_for_tests();
    set_text_layout_cache_policy(TextLayoutCachePolicy::new(256, TEST_BYTES_BUDGET));
    let visible = (0..100)
        .map(|index| {
            input(
                TextLayoutSurfaceId::Block(index + 1),
                &format!("visible surface {index} remains cached"),
            )
        })
        .collect::<Vec<_>>();
    for input in &visible {
        let result = cached_text_layout(input, theme(), &options(320.0));
        assert!(matches!(
            result.strategy,
            TextRelayoutStrategy::FullBuild(TextRelayoutFallbackReason::NoPreviousSnapshot)
        ));
    }

    let mut focused = visible[41].clone();
    focused.layout_version += 1;
    let focused_result = cached_text_layout_with_request(
        &focused,
        theme(),
        &options(280.0),
        request(TextLayoutCachePriority::Editing, false),
    );

    assert_eq!(focused_result.strategy, TextRelayoutStrategy::Reflow);
    assert_eq!(text_layout_cache_stats().entries, 100);
    for (index, input) in visible.iter().enumerate() {
        if index == 41 {
            continue;
        }
        assert_eq!(
            cached_text_layout(input, theme(), &options(320.0)).strategy,
            TextRelayoutStrategy::CacheHit
        );
    }
}

#[test]
fn full_build_reports_why_incremental_relayout_is_not_valid() {
    let base = input(TextLayoutSurfaceId::Block(1), "base");

    assert_full_build_reason(
        &base,
        |changed, _| {
            changed.content_version += 1;
            changed.spans = vec![InlineSpan::plain("changed")];
        },
        TextRelayoutFallbackReason::ContentChanged,
    );
    assert_full_build_reason(
        &base,
        |changed, _| changed.spans[0].marks.push(InlineMark::Bold),
        TextRelayoutFallbackReason::StyleChanged,
    );
    assert_full_build_reason(
        &base,
        |_, options| {
            options.inline_boxes.push(InlineBoxSpec {
                id: 1,
                kind: InlineBoxKind::InFlow,
                index: 0,
                width: 20.0,
                height: 20.0,
            });
        },
        TextRelayoutFallbackReason::InlineObjectsChanged,
    );
    assert_full_build_reason(
        &base,
        |changed, _| changed.font_version += 1,
        TextRelayoutFallbackReason::FontChanged,
    );
    assert_full_build_reason(
        &base,
        |_, options| options.display_scale = 2.0,
        TextRelayoutFallbackReason::ScaleChanged,
    );
}

fn assert_full_build_reason(
    base: &TextLayoutInput,
    mutate: impl FnOnce(&mut TextLayoutInput, &mut TextLayoutOptions),
    expected: TextRelayoutFallbackReason,
) {
    reset_text_layout_cache_for_tests();
    let base_options = options(200.0);
    cached_text_layout(base, theme(), &base_options);
    let mut changed = base.clone();
    let mut changed_options = base_options;
    mutate(&mut changed, &mut changed_options);

    let result = cached_text_layout(&changed, theme(), &changed_options);

    assert_eq!(result.strategy, TextRelayoutStrategy::FullBuild(expected));
    assert!(!result.reflowed);
}

#[test]
fn stale_surface_lookup_returns_the_newest_snapshot_across_shape_changes() {
    let surface = TextLayoutSurfaceId::Block(981);
    let visible = request(TextLayoutCachePriority::Visible, false);
    let first = input(surface, "stale fallback source");
    cached_text_layout_with_request(&first, theme(), &options(200.0), visible);

    // A shape-identity change (edit or restyle) misses both the exact and
    // compatible lookups but still finds the previous snapshot for the
    // surface.
    let mut edited = input(surface, "stale fallback source edited");
    edited.content_version = 2;
    assert!(try_cached_text_layout_with_request(&edited, &options(200.0), visible).is_none());
    assert!(try_compatible_text_layout_with_request(&edited, &options(200.0), visible).is_none());
    let stale = try_stale_text_layout_for_surface(&edited, &options(200.0), visible)
        .expect("previous surface snapshot is the last-resort fallback");
    assert_eq!(stale.layout.text(), "stale fallback source");
    assert_eq!(stale.key.shape.content_version, 1);

    // Other surfaces never leak in.
    let other = input(TextLayoutSurfaceId::Block(982), "unrelated");
    assert!(try_stale_text_layout_for_surface(&other, &options(200.0), visible).is_none());
}
