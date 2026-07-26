use std::ops::Range;

use cditor_runtime::EditorViewProjection;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PayloadScheduleRanges {
    pub(super) visible: Option<Range<usize>>,
    pub(super) prefetch: Option<Range<usize>>,
}

pub(super) fn payload_frame_plan(
    projection: &EditorViewProjection,
    storage_available: bool,
) -> PayloadScheduleRanges {
    let placeholder_count = projection
        .blocks
        .iter()
        .filter(|block| block.placeholder)
        .count();
    let presents_desired_visible = projection.payload_visible_block_range.is_empty()
        || (projection.render_window.block_range.start
            <= projection.payload_visible_block_range.start
            && projection.payload_visible_block_range.end
                <= projection.render_window.block_range.end);
    let visible_ready = presents_desired_visible
        && !projection.render_window.is_placeholder()
        && projection.blocks.iter().all(|block| {
            !projection
                .payload_visible_block_range
                .contains(&block.visible_index)
                || !block.placeholder
        });
    let stage = if projection.render_window.is_placeholder() {
        "projection.full-placeholder"
    } else if placeholder_count > 0 {
        "projection.partial-placeholder"
    } else if presents_desired_visible {
        "projection.resident"
    } else {
        "projection.stable-preparing"
    };
    crate::diagnostics::payload_pipeline::trace_payload_state(
        stage,
        format_args!(
            "generation={} window={:?} visible={:?} prefetch={:?} blocks={} placeholders={placeholder_count}",
            projection.window_generation,
            projection.render_window.block_range,
            projection.payload_visible_block_range,
            projection.payload_prefetch_block_range,
            projection.blocks.len()
        ),
    );

    if !storage_available {
        return PayloadScheduleRanges {
            visible: None,
            prefetch: None,
        };
    }
    payload_schedule_ranges(
        visible_ready,
        projection
            .placeholder_window_failure
            .as_ref()
            .is_none_or(|failure| failure.automatic_retry_pending),
        projection.payload_prefetch_resident,
        &projection.payload_visible_block_range,
        &projection.payload_prefetch_block_range,
    )
}

pub(super) fn payload_schedule_ranges(
    visible_ready: bool,
    visible_request_allowed: bool,
    prefetch_resident: bool,
    visible_range: &Range<usize>,
    prefetch_range: &Range<usize>,
) -> PayloadScheduleRanges {
    if !visible_ready {
        return PayloadScheduleRanges {
            visible: visible_request_allowed.then(|| visible_range.clone()),
            prefetch: None,
        };
    }

    PayloadScheduleRanges {
        visible: None,
        prefetch: (!prefetch_resident && prefetch_range != visible_range)
            .then(|| prefetch_range.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_old_projection_still_requests_the_remote_visible_target() {
        let ranges =
            payload_schedule_ranges(false, true, false, &(40_000..40_030), &(39_872..40_158));

        assert_eq!(ranges.visible, Some(40_000..40_030));
        assert_eq!(ranges.prefetch, None);
    }

    #[test]
    fn resident_visible_target_only_schedules_background_prefetch() {
        let ranges =
            payload_schedule_ranges(true, true, false, &(40_000..40_030), &(39_872..40_158));

        assert_eq!(ranges.visible, None);
        assert_eq!(ranges.prefetch, Some(39_872..40_158));
    }

    #[test]
    fn fully_resident_range_does_not_reenter_the_session_scheduler() {
        let ranges =
            payload_schedule_ranges(true, true, true, &(40_000..40_030), &(40_000..40_030));

        assert_eq!(
            ranges,
            PayloadScheduleRanges {
                visible: None,
                prefetch: None,
            }
        );
    }

    #[test]
    fn terminal_failure_waits_for_explicit_retry_without_replanning_each_frame() {
        let ranges =
            payload_schedule_ranges(false, false, false, &(40_000..40_030), &(39_872..40_158));

        assert_eq!(ranges.visible, None);
        assert_eq!(ranges.prefetch, None);
    }

    #[test]
    fn resident_prefetch_does_not_reenter_the_session_scheduler() {
        let ranges =
            payload_schedule_ranges(true, true, true, &(40_000..40_030), &(39_872..40_158));

        assert_eq!(ranges.visible, None);
        assert_eq!(ranges.prefetch, None);
    }
}
