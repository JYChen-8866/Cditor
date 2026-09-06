use super::*;

fn test_bounds() -> Bounds<Pixels> {
    Bounds {
        origin: point(px(100.0), px(20.0)),
        size: size(px(12.0), px(320.0)),
    }
}

#[test]
fn thumb_metrics_follow_scroll_progress_and_minimum_extent() {
    let style = InteractiveScrollbarStyle::notion(0xc7c7c5, 0x9b9a97);
    let top = scrollbar_metrics(
        test_bounds(),
        ScrollbarAxis::Vertical,
        ScrollbarModel {
            offset_px: 0.0,
            max_offset_px: 320.0,
            visible_fraction: 0.5,
        },
        style,
    )
    .unwrap();
    let bottom = scrollbar_metrics(
        test_bounds(),
        ScrollbarAxis::Vertical,
        ScrollbarModel {
            offset_px: 320.0,
            max_offset_px: 320.0,
            visible_fraction: 0.5,
        },
        style,
    )
    .unwrap();

    assert_eq!(top.track_start_px, 23.0);
    assert_eq!(f32::from(top.thumb_bounds.top()), 23.0);
    assert_eq!(f32::from(bottom.thumb_bounds.bottom()), 337.0);
    assert!(bottom.thumb_extent_px >= style.min_thumb_extent_px);
}

#[test]
fn track_click_and_drag_map_to_clamped_offsets() {
    let metrics = ScrollbarMetrics {
        thumb_bounds: Bounds::default(),
        track_start_px: 20.0,
        thumb_extent_px: 80.0,
        travel_px: 240.0,
        max_offset_px: 600.0,
    };
    let mapped =
        scrollbar_offset_for_pointer(metrics, 260.0, metrics.thumb_extent_px / 2.0).unwrap();
    assert_eq!(mapped, 500.0);
    assert_eq!(
        scrollbar_offset_for_pointer(metrics, -100.0, 40.0),
        Some(0.0)
    );
    assert_eq!(
        scrollbar_offset_for_pointer(metrics, 1_000.0, 40.0),
        Some(600.0)
    );
}

#[test]
fn hitbox_expands_across_the_track_without_changing_main_axis() {
    let hitbox = scrollbar_hitbox_bounds(test_bounds(), ScrollbarAxis::Vertical, 16.0);
    assert_eq!(hitbox.size.width, px(16.0));
    assert_eq!(hitbox.size.height, px(320.0));
    assert_eq!(hitbox.top(), px(20.0));
}

#[test]
fn horizontal_metrics_move_along_x_and_preserve_cross_axis_centering() {
    let bounds = Bounds {
        origin: point(px(20.0), px(100.0)),
        size: size(px(400.0), px(12.0)),
    };
    let metrics = scrollbar_metrics(
        bounds,
        ScrollbarAxis::Horizontal,
        ScrollbarModel {
            offset_px: 300.0,
            max_offset_px: 600.0,
            visible_fraction: 0.5,
        },
        InteractiveScrollbarStyle::notion(0xc7c7c5, 0x9b9a97),
    )
    .unwrap();

    assert_eq!(metrics.thumb_bounds.size.height, px(4.0));
    assert_eq!(metrics.thumb_bounds.origin.y, px(104.0));
    assert!(metrics.thumb_bounds.left() > bounds.left());
    assert!(metrics.thumb_bounds.right() < bounds.right());
}

#[test]
fn thumb_grab_offset_does_not_jump_during_drag() {
    let metrics = ScrollbarMetrics {
        thumb_bounds: Bounds::default(),
        track_start_px: 20.0,
        thumb_extent_px: 80.0,
        travel_px: 240.0,
        max_offset_px: 600.0,
    };

    let pointer_at_grab = 20.0 + 120.0 + 15.0;
    assert_eq!(
        scrollbar_offset_for_pointer(metrics, pointer_at_grab, 15.0),
        Some(300.0)
    );
}

#[test]
fn metrics_are_hidden_when_content_does_not_scroll() {
    assert!(
        scrollbar_metrics(
            test_bounds(),
            ScrollbarAxis::Vertical,
            ScrollbarModel {
                offset_px: 0.0,
                max_offset_px: 0.0,
                visible_fraction: 1.0,
            },
            InteractiveScrollbarStyle::notion(0xc7c7c5, 0x9b9a97),
        )
        .is_none()
    );
}

#[test]
fn degenerate_tracks_do_not_produce_offsets() {
    let metrics = ScrollbarMetrics {
        thumb_bounds: Bounds::default(),
        track_start_px: 0.0,
        thumb_extent_px: 20.0,
        travel_px: 0.0,
        max_offset_px: 100.0,
    };
    assert_eq!(scrollbar_offset_for_pointer(metrics, 50.0, 10.0), None);
}

#[test]
fn drag_state_is_scoped_to_the_owning_scrollbar() {
    set_drag_state(Some(ScrollbarDragState {
        owner: 7,
        grab_offset_px: 12.0,
    }));

    assert!(drag_state().is_some_and(|state| state.owner == 7));
    assert!(drag_state().is_none_or(|state| state.owner != 9));

    set_drag_state(None);
    assert_eq!(drag_state(), None);
}

#[test]
fn shared_hover_animation_interpolates_between_idle_and_active_thickness() {
    let style = InteractiveScrollbarStyle::notion(0xc7c7c5, 0x9b9a97);

    assert_eq!(animated_scrollbar_thickness(style, 0.0), 4.0);
    assert_eq!(animated_scrollbar_thickness(style, 0.5), 7.0);
    assert_eq!(animated_scrollbar_thickness(style, 1.0), 10.0);
    assert_eq!(
        SCROLLBAR_HOVER_ANIMATION_DURATION,
        Duration::from_millis(120)
    );
}

#[test]
fn hover_animation_starts_when_the_target_changes_instead_of_consuming_idle_time() {
    let start = Instant::now();
    let mut animation = ScrollbarAnimationState::new(false, start);

    assert_eq!(animation.update(true, start + Duration::from_secs(5)), 0.0);
    assert_eq!(
        animation.update(
            true,
            start + Duration::from_secs(5) + Duration::from_millis(60)
        ),
        0.5
    );
    assert!(animation.animating());
}

#[test]
fn auto_hide_visibility_matches_gpui_component_scrolling_mode() {
    let now = Instant::now();
    let recent = now - Duration::from_millis(250);
    let idle = now - SCROLLBAR_AUTO_HIDE_IDLE;

    assert!(scrollbar_should_be_visible(
        true,
        false,
        false,
        false,
        Some(recent),
        now,
    ));
    assert!(!scrollbar_should_be_visible(
        true,
        false,
        false,
        false,
        Some(idle),
        now,
    ));
    assert!(scrollbar_should_be_visible(
        true, true, false, true, None, now,
    ));
    assert!(!scrollbar_should_be_visible(
        true, true, false, false, None, now,
    ));
    assert!(scrollbar_should_be_visible(
        true, false, true, false, None, now,
    ));
    assert!(scrollbar_should_be_visible(
        false, false, false, false, None, now,
    ));
}
