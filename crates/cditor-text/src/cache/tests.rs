use cditor_core::rich_text::{InlineMark, InlineSpan, RichBlockKind, TextAlign};

use super::*;
use crate::{ParleyInlineBoxKind, ParleyInlineBoxSpec, ParleyLineHeight, ParleyTextStyleConfig};

const TEST_BYTES_BUDGET: usize = 16 * 1024 * 1024;

fn theme() -> TextTheme {
    TextTheme {
        link_text: 0x2383e2,
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

fn options(width: f32) -> ParleyLayoutOptions {
    ParleyLayoutOptions {
        width: Some(width),
        base_text_color: 0x37352f,
        base_style: ParleyTextStyleConfig {
            line_height: ParleyLineHeight::Absolute(24.0),
            ..ParleyTextStyleConfig::default()
        },
        ..ParleyLayoutOptions::default()
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
    clear_parley_layout_cache();
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
fn lru_evicts_the_oldest_entry_within_the_lowest_priority() {
    clear_parley_layout_cache();
    set_text_layout_cache_policy(TextLayoutCachePolicy::new(2, TEST_BYTES_BUDGET));
    let first_surface = TextLayoutSurfaceId::Block(1);
    let second_surface = TextLayoutSurfaceId::Block(2);
    let third_surface = TextLayoutSurfaceId::Block(3);
    let first = input(first_surface, "first");
    let second = input(second_surface, "second");

    cached_parley_layout_with_request(
        &first,
        theme(),
        &options(200.0),
        request(TextLayoutCachePriority::Offscreen, false),
    );
    cached_parley_layout_with_request(
        &second,
        theme(),
        &options(200.0),
        request(TextLayoutCachePriority::Offscreen, false),
    );
    cached_parley_layout_with_request(
        &first,
        theme(),
        &options(200.0),
        request(TextLayoutCachePriority::Offscreen, false),
    );
    cached_parley_layout_with_request(
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
    clear_parley_layout_cache();
    set_text_layout_cache_policy(TextLayoutCachePolicy::new(10, 1));

    let result = cached_parley_layout(
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
    clear_parley_layout_cache();
    set_text_layout_cache_policy(TextLayoutCachePolicy::new(1, TEST_BYTES_BUDGET));
    let editing_surface = TextLayoutSurfaceId::Block(1);
    let editing = input(editing_surface, "editing");

    cached_parley_layout_with_request(
        &editing,
        theme(),
        &options(200.0),
        TextLayoutCacheRequest::editing(),
    );
    cached_parley_layout(
        &input(TextLayoutSurfaceId::Block(2), "visible"),
        theme(),
        &options(200.0),
    );

    assert!(contains_surface(editing_surface));
    assert_eq!(text_layout_cache_stats().pinned_entries, 1);

    let visible_result = cached_parley_layout(&editing, theme(), &options(200.0));
    assert!(visible_result.cache_hit);
    let report = apply_text_layout_memory_pressure(TextLayoutMemoryPressure::Critical);
    assert_eq!(report.remaining_entries, 0);
    assert!(!report.over_budget_due_to_pins);
}

#[test]
fn warning_pressure_evicts_offscreen_then_overscan_before_visible_and_editing() {
    clear_parley_layout_cache();
    set_text_layout_cache_policy(TextLayoutCachePolicy::new(4, TEST_BYTES_BUDGET));
    let fixtures = [
        (1, TextLayoutCachePriority::Offscreen),
        (2, TextLayoutCachePriority::Overscan),
        (3, TextLayoutCachePriority::Visible),
        (4, TextLayoutCachePriority::Editing),
    ];
    for (block_id, priority) in fixtures {
        cached_parley_layout_with_request(
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
    clear_parley_layout_cache();
    set_text_layout_cache_policy(TextLayoutCachePolicy::new(4, TEST_BYTES_BUDGET));
    let pinned_surface = TextLayoutSurfaceId::Block(1);
    let other_surface = TextLayoutSurfaceId::Block(2);
    cached_parley_layout(&input(pinned_surface, "pinned"), theme(), &options(200.0));
    cached_parley_layout(&input(other_surface, "other"), theme(), &options(200.0));
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
    clear_parley_layout_cache();
    set_text_layout_cache_policy(TextLayoutCachePolicy::new(4, TEST_BYTES_BUDGET));
    let previous_surface = TextLayoutSurfaceId::Block(1);
    cached_parley_layout_with_request(
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
    clear_parley_layout_cache();
    set_text_layout_cache_policy(TextLayoutCachePolicy::new(2, TEST_BYTES_BUDGET));
    let input = input(
        TextLayoutSurfaceId::Block(1),
        "wrap this text over several words",
    );

    let first = cached_parley_layout(&input, theme(), &options(80.0));
    let second = cached_parley_layout(&input, theme(), &options(80.0));
    let reflowed = cached_parley_layout(&input, theme(), &options(400.0));
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
    assert_eq!(stats.entries, 2);
    assert!(stats.estimated_bytes > 0);
}

#[test]
fn layout_generation_change_reflows_without_reshaping() {
    clear_parley_layout_cache();
    let base = input(TextLayoutSurfaceId::Block(1), "same shape");
    cached_parley_layout(&base, theme(), &options(200.0));
    let mut changed = base.clone();
    changed.layout_version += 1;

    let result = cached_parley_layout(&changed, theme(), &options(200.0));

    assert_eq!(result.strategy, TextRelayoutStrategy::Reflow);
    assert!(result.reflowed);
}

#[test]
fn focused_relayout_does_not_invalidate_one_hundred_visible_surfaces() {
    clear_parley_layout_cache();
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
        let result = cached_parley_layout(input, theme(), &options(320.0));
        assert!(matches!(
            result.strategy,
            TextRelayoutStrategy::FullBuild(TextRelayoutFallbackReason::NoPreviousSnapshot)
        ));
    }

    let mut focused = visible[41].clone();
    focused.layout_version += 1;
    let focused_result = cached_parley_layout_with_request(
        &focused,
        theme(),
        &options(280.0),
        request(TextLayoutCachePriority::Editing, false),
    );

    assert_eq!(focused_result.strategy, TextRelayoutStrategy::Reflow);
    assert_eq!(text_layout_cache_stats().entries, 101);
    for (index, input) in visible.iter().enumerate() {
        if index == 41 {
            continue;
        }
        assert_eq!(
            cached_parley_layout(input, theme(), &options(320.0)).strategy,
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
            options.inline_boxes.push(ParleyInlineBoxSpec {
                id: 1,
                kind: ParleyInlineBoxKind::InFlow,
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
    mutate: impl FnOnce(&mut TextLayoutInput, &mut ParleyLayoutOptions),
    expected: TextRelayoutFallbackReason,
) {
    clear_parley_layout_cache();
    let base_options = options(200.0);
    cached_parley_layout(base, theme(), &base_options);
    let mut changed = base.clone();
    let mut changed_options = base_options;
    mutate(&mut changed, &mut changed_options);

    let result = cached_parley_layout(&changed, theme(), &changed_options);

    assert_eq!(result.strategy, TextRelayoutStrategy::FullBuild(expected));
    assert!(!result.reflowed);
}
