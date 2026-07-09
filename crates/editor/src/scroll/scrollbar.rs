use crate::scroll::virtual_scroll::{
    LayoutPx, ScrollOrigin, VirtualScrollError, VirtualScrollState,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrollbarPolicy {
    pub track_height: LayoutPx,
    pub min_thumb_height: LayoutPx,
    pub local_list_state_scrollbar_enabled: bool,
}

impl Default for ScrollbarPolicy {
    fn default() -> Self {
        Self {
            track_height: 600.0,
            min_thumb_height: 24.0,
            local_list_state_scrollbar_enabled: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrollbarVisualState {
    pub enabled: bool,
    pub track_height: LayoutPx,
    pub thumb_height: LayoutPx,
    pub thumb_top: LayoutPx,
    pub scroll_ratio: f64,
}

impl ScrollbarVisualState {
    pub fn from_virtual_scroll(state: &VirtualScrollState, policy: ScrollbarPolicy) -> Self {
        let total_height = state.displayed_total_height.max(0.0);
        let viewport_height = state.viewport_height.max(0.0);
        let track_height = policy.track_height.max(0.0);
        let enabled = total_height > viewport_height && track_height > 0.0;

        if !enabled {
            return Self {
                enabled: false,
                track_height,
                thumb_height: track_height,
                thumb_top: 0.0,
                scroll_ratio: 0.0,
            };
        }

        let raw_thumb_height = track_height * viewport_height / total_height;
        let thumb_height = raw_thumb_height
            .max(policy.min_thumb_height.min(track_height))
            .min(track_height);
        let max_scroll_top = (state.model_total_height - viewport_height).max(0.0);
        let scroll_ratio = if max_scroll_top <= 0.0 {
            0.0
        } else {
            (state.global_scroll_top / max_scroll_top).clamp(0.0, 1.0)
        };
        let max_thumb_top = (track_height - thumb_height).max(0.0);
        let thumb_top = (scroll_ratio * max_thumb_top).clamp(0.0, max_thumb_top);

        Self {
            enabled,
            track_height,
            thumb_height,
            thumb_top,
            scroll_ratio,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScrollbarDragSession {
    pub frozen_total_height: LayoutPx,
    pub start_global_scroll_top: LayoutPx,
    pub start_thumb_top: LayoutPx,
    pub pending_layout_corrections: Vec<PendingHeightCorrection>,
}

impl ScrollbarDragSession {
    pub fn begin(state: &mut VirtualScrollState, visual: ScrollbarVisualState) -> Self {
        state.displayed_total_height = state.model_total_height;
        Self {
            frozen_total_height: state.displayed_total_height,
            start_global_scroll_top: state.global_scroll_top,
            start_thumb_top: visual.thumb_top,
            pending_layout_corrections: Vec::new(),
        }
    }

    pub fn push_pending_height_correction(&mut self, correction: PendingHeightCorrection) {
        self.pending_layout_corrections.push(correction);
    }

    pub fn drag_to_thumb_top(
        &self,
        state: &mut VirtualScrollState,
        policy: ScrollbarPolicy,
        thumb_top: LayoutPx,
    ) -> Result<ScrollbarDragUpdate, VirtualScrollError> {
        let visual = visual_for_total_height(state, policy, self.frozen_total_height);
        if !visual.enabled {
            return Ok(ScrollbarDragUpdate {
                global_scroll_top: 0.0,
                drag_ratio: 0.0,
                visual,
            });
        }

        let max_thumb_top = (policy.track_height - visual.thumb_height).max(0.0);
        let clamped_thumb_top = thumb_top.clamp(0.0, max_thumb_top);
        let drag_ratio = if max_thumb_top <= 0.0 {
            0.0
        } else {
            clamped_thumb_top / max_thumb_top
        };
        let frozen_max_scroll_top = (self.frozen_total_height - state.viewport_height).max(0.0);
        let next_global_scroll_top = drag_ratio * frozen_max_scroll_top;
        state.scroll_to_global_offset(next_global_scroll_top, ScrollOrigin::UserScrollbar)?;

        Ok(ScrollbarDragUpdate {
            global_scroll_top: state.global_scroll_top,
            drag_ratio,
            visual: ScrollbarVisualState {
                thumb_top: clamped_thumb_top,
                scroll_ratio: drag_ratio,
                ..visual
            },
        })
    }

    pub fn finish(self, state: &mut VirtualScrollState) -> ScrollbarDragEnd {
        state.displayed_total_height = self.frozen_total_height;
        ScrollbarDragEnd {
            pending_layout_corrections: self.pending_layout_corrections.len(),
            should_restore_anchor: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrollbarDragUpdate {
    pub global_scroll_top: LayoutPx,
    pub drag_ratio: f64,
    pub visual: ScrollbarVisualState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrollbarDragEnd {
    pub pending_layout_corrections: usize,
    pub should_restore_anchor: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PendingHeightCorrection {
    pub old_total_height: LayoutPx,
    pub new_total_height: LayoutPx,
}

fn visual_for_total_height(
    state: &VirtualScrollState,
    policy: ScrollbarPolicy,
    displayed_total_height: LayoutPx,
) -> ScrollbarVisualState {
    let mut copied = *state;
    copied.displayed_total_height = displayed_total_height;
    ScrollbarVisualState::from_virtual_scroll(&copied, policy)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visual_thumb_height_top_and_min_size_are_computed_from_virtual_scroll() {
        let mut state = VirtualScrollState::new(100.0, 1_000.0).unwrap();
        state
            .scroll_to_global_offset(450.0, ScrollOrigin::ProgrammaticVirtualScroll)
            .unwrap();
        let visual = ScrollbarVisualState::from_virtual_scroll(
            &state,
            ScrollbarPolicy {
                track_height: 200.0,
                min_thumb_height: 30.0,
                local_list_state_scrollbar_enabled: false,
            },
        );

        assert!(visual.enabled);
        assert_eq!(visual.thumb_height, 30.0);
        assert_eq!(visual.scroll_ratio, 0.5);
        assert_eq!(visual.thumb_top, 85.0);
    }

    #[test]
    fn scrollbar_is_disabled_when_total_height_fits_viewport() {
        let state = VirtualScrollState::new(1_000.0, 500.0).unwrap();
        let visual = ScrollbarVisualState::from_virtual_scroll(&state, ScrollbarPolicy::default());

        assert!(!visual.enabled);
        assert_eq!(visual.thumb_top, 0.0);
    }

    #[test]
    fn drag_precision_maps_thumb_ratio_to_global_scroll_with_frozen_total() {
        let mut state = VirtualScrollState::new(100.0, 1_000.0).unwrap();
        let policy = ScrollbarPolicy {
            track_height: 200.0,
            min_thumb_height: 20.0,
            local_list_state_scrollbar_enabled: false,
        };
        let visual = ScrollbarVisualState::from_virtual_scroll(&state, policy);
        let session = ScrollbarDragSession::begin(&mut state, visual);

        let update = session.drag_to_thumb_top(&mut state, policy, 90.0).unwrap();

        assert_eq!(update.drag_ratio, 0.5);
        assert_eq!(update.global_scroll_top, 450.0);
        assert_eq!(state.global_scroll_top, 450.0);
        assert_eq!(state.origin, ScrollOrigin::UserScrollbar);
    }

    #[test]
    fn height_correction_during_drag_does_not_make_thumb_jump() {
        let mut state = VirtualScrollState::new(100.0, 1_000.0).unwrap();
        let policy = ScrollbarPolicy {
            track_height: 200.0,
            min_thumb_height: 20.0,
            local_list_state_scrollbar_enabled: false,
        };
        let visual = ScrollbarVisualState::from_virtual_scroll(&state, policy);
        let mut session = ScrollbarDragSession::begin(&mut state, visual);

        state.set_model_total_height(2_000.0).unwrap();
        session.push_pending_height_correction(PendingHeightCorrection {
            old_total_height: 1_000.0,
            new_total_height: 2_000.0,
        });
        let update = session.drag_to_thumb_top(&mut state, policy, 90.0).unwrap();

        assert_eq!(session.frozen_total_height, 1_000.0);
        assert_eq!(update.drag_ratio, 0.5);
        assert_eq!(update.global_scroll_top, 450.0);
        assert_eq!(update.visual.thumb_height, 20.0);
    }

    #[test]
    fn trace_has_no_thumb_reverse_jump_during_drag() {
        let mut state = VirtualScrollState::new(100.0, 1_000.0).unwrap();
        let policy = ScrollbarPolicy {
            track_height: 200.0,
            min_thumb_height: 20.0,
            local_list_state_scrollbar_enabled: false,
        };
        let visual = ScrollbarVisualState::from_virtual_scroll(&state, policy);
        let session = ScrollbarDragSession::begin(&mut state, visual);
        let mut previous_thumb_top = 0.0;
        let mut reverse_jump_count = 0;

        for thumb_top in (0..=180).step_by(5) {
            state
                .set_model_total_height(1_000.0 + thumb_top as f64 * 10.0)
                .unwrap();
            let update = session
                .drag_to_thumb_top(&mut state, policy, thumb_top as f64)
                .unwrap();
            if update.visual.thumb_top < previous_thumb_top {
                reverse_jump_count += 1;
            }
            previous_thumb_top = update.visual.thumb_top;
        }

        assert_eq!(reverse_jump_count, 0);
    }

    #[test]
    fn mouseup_reports_anchor_restore_after_pending_corrections() {
        let mut state = VirtualScrollState::new(100.0, 1_000.0).unwrap();
        let visual = ScrollbarVisualState::from_virtual_scroll(&state, ScrollbarPolicy::default());
        let mut session = ScrollbarDragSession::begin(&mut state, visual);
        session.push_pending_height_correction(PendingHeightCorrection {
            old_total_height: 1_000.0,
            new_total_height: 1_200.0,
        });

        let end = session.finish(&mut state);

        assert_eq!(end.pending_layout_corrections, 1);
        assert!(end.should_restore_anchor);
    }
}
