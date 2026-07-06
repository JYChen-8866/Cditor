use std::collections::BTreeSet;
use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollDirection {
    Up,
    Down,
    Still,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowPlannerPolicy {
    pub enter_threshold_viewports: f64,
    pub exit_threshold_viewports: f64,
    pub min_stable_frames_before_trim: u32,
    pub min_ms_between_window_commits: u64,
}

impl Default for WindowPlannerPolicy {
    fn default() -> Self {
        Self {
            enter_threshold_viewports: 0.5,
            exit_threshold_viewports: 1.0,
            min_stable_frames_before_trim: 2,
            min_ms_between_window_commits: 16,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WindowPlanner {
    pub before_pages: usize,
    pub after_pages: usize,
    pub policy: WindowPlannerPolicy,
    current_range: Option<Range<usize>>,
    stable_frames: u32,
    last_commit_ms: Option<u64>,
}

impl WindowPlanner {
    pub fn new(before_pages: usize, after_pages: usize, policy: WindowPlannerPolicy) -> Self {
        Self {
            before_pages,
            after_pages,
            policy,
            current_range: None,
            stable_frames: 0,
            last_commit_ms: None,
        }
    }

    pub fn plan(&self, target_page: usize, page_count: usize) -> Range<usize> {
        let start = target_page.saturating_sub(self.before_pages);
        let end = (target_page + self.after_pages + 1).min(page_count);
        start..end
    }

    pub fn plan_with_direction(
        &self,
        target_page: usize,
        page_count: usize,
        direction: ScrollDirection,
    ) -> Range<usize> {
        let extra_before = usize::from(direction == ScrollDirection::Up);
        let extra_after = usize::from(direction == ScrollDirection::Down);
        let start = target_page.saturating_sub(self.before_pages + extra_before);
        let end = (target_page + self.after_pages + extra_after + 1).min(page_count);
        start..end
    }

    pub fn plan_commit(&mut self, request: WindowPlanRequest) -> WindowPlanDecision {
        let mut desired = self.plan_with_direction(
            request.target_page,
            request.page_count,
            request.scroll_direction,
        );
        desired = include_pinned_pages(desired, request.page_count, &request.pinned_pages);

        let current = self.current_range.clone();
        if let Some(current_range) = &current {
            if current_range == &desired {
                self.stable_frames = self.stable_frames.saturating_add(1);
                return WindowPlanDecision::Keep {
                    page_range: current_range.clone(),
                    reason: KeepReason::Unchanged,
                };
            }

            if !target_has_crossed_hysteresis(
                request.target_page,
                current_range,
                request.position_in_page_viewports,
                self.policy.enter_threshold_viewports,
            ) {
                self.stable_frames = self.stable_frames.saturating_add(1);
                return WindowPlanDecision::Keep {
                    page_range: current_range.clone(),
                    reason: KeepReason::WithinHysteresis,
                };
            }

            if self.stable_frames.saturating_add(1) < self.policy.min_stable_frames_before_trim {
                self.stable_frames = self.stable_frames.saturating_add(1);
                return WindowPlanDecision::Keep {
                    page_range: current_range.clone(),
                    reason: KeepReason::WaitingStableFrames,
                };
            }

            if let Some(last_commit_ms) = self.last_commit_ms {
                if request.now_ms.saturating_sub(last_commit_ms)
                    < self.policy.min_ms_between_window_commits
                {
                    return WindowPlanDecision::Keep {
                        page_range: current_range.clone(),
                        reason: KeepReason::CommitDebounced,
                    };
                }
            }
        }

        self.current_range = Some(desired.clone());
        self.stable_frames = 0;
        self.last_commit_ms = Some(request.now_ms);
        WindowPlanDecision::Commit {
            page_range: desired,
        }
    }

    pub fn debug_overlay(&self) -> WindowPlannerDebugOverlay {
        WindowPlannerDebugOverlay {
            current_page_range: self.current_range.clone(),
            stable_frames: self.stable_frames,
            last_commit_ms: self.last_commit_ms,
        }
    }
}

impl Default for WindowPlanner {
    fn default() -> Self {
        Self::new(1, 1, WindowPlannerPolicy::default())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WindowPlanRequest {
    pub target_page: usize,
    pub page_count: usize,
    pub scroll_direction: ScrollDirection,
    pub position_in_page_viewports: f64,
    pub pinned_pages: BTreeSet<usize>,
    pub now_ms: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WindowPlanDecision {
    Keep {
        page_range: Range<usize>,
        reason: KeepReason,
    },
    Commit {
        page_range: Range<usize>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeepReason {
    Unchanged,
    WithinHysteresis,
    WaitingStableFrames,
    CommitDebounced,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowPlannerDebugOverlay {
    pub current_page_range: Option<Range<usize>>,
    pub stable_frames: u32,
    pub last_commit_ms: Option<u64>,
}

fn include_pinned_pages(
    mut range: Range<usize>,
    page_count: usize,
    pinned_pages: &BTreeSet<usize>,
) -> Range<usize> {
    for page in pinned_pages
        .iter()
        .copied()
        .filter(|page| *page < page_count)
    {
        range.start = range.start.min(page);
        range.end = range.end.max(page + 1);
    }
    range
}

fn target_has_crossed_hysteresis(
    target_page: usize,
    current_range: &Range<usize>,
    position_in_page_viewports: f64,
    enter_threshold_viewports: f64,
) -> bool {
    if target_page + 1 == current_range.start {
        return position_in_page_viewports < 1.0 - enter_threshold_viewports;
    }
    if target_page == current_range.end {
        return position_in_page_viewports > enter_threshold_viewports;
    }
    if target_page < current_range.start || target_page >= current_range.end {
        return true;
    }

    if target_page == current_range.start {
        return position_in_page_viewports < 1.0 - enter_threshold_viewports;
    }
    if target_page + 1 == current_range.end {
        return position_in_page_viewports > enter_threshold_viewports;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_before_after_pages_around_current_page() {
        let planner = WindowPlanner::new(1, 2, WindowPlannerPolicy::default());

        assert_eq!(planner.plan(5, 10), 4..8);
        assert_eq!(planner.plan(0, 10), 0..3);
        assert_eq!(planner.plan(9, 10), 8..10);
    }

    #[test]
    fn fast_down_and_up_scroll_prefetch_directionally() {
        let planner = WindowPlanner::new(1, 1, WindowPlannerPolicy::default());

        assert_eq!(
            planner.plan_with_direction(5, 10, ScrollDirection::Down),
            4..8
        );
        assert_eq!(
            planner.plan_with_direction(5, 10, ScrollDirection::Up),
            3..7
        );
    }

    #[test]
    fn boundary_hysteresis_prevents_repeated_ab_commits() {
        let mut planner = WindowPlanner::new(
            0,
            0,
            WindowPlannerPolicy {
                enter_threshold_viewports: 0.5,
                exit_threshold_viewports: 1.0,
                min_stable_frames_before_trim: 0,
                min_ms_between_window_commits: 0,
            },
        );
        let pinned_pages = BTreeSet::new();

        assert!(matches!(
            planner.plan_commit(WindowPlanRequest {
                target_page: 10,
                page_count: 100,
                scroll_direction: ScrollDirection::Still,
                position_in_page_viewports: 0.5,
                pinned_pages: pinned_pages.clone(),
                now_ms: 0,
            }),
            WindowPlanDecision::Commit { page_range } if page_range == (10..11)
        ));

        let decision = planner.plan_commit(WindowPlanRequest {
            target_page: 11,
            page_count: 100,
            scroll_direction: ScrollDirection::Still,
            position_in_page_viewports: 0.49,
            pinned_pages,
            now_ms: 16,
        });

        assert!(matches!(
            decision,
            WindowPlanDecision::Keep {
                reason: KeepReason::WithinHysteresis,
                ..
            }
        ));
    }

    #[test]
    fn requires_stable_frames_before_trim() {
        let mut planner = WindowPlanner::new(
            0,
            0,
            WindowPlannerPolicy {
                min_stable_frames_before_trim: 2,
                min_ms_between_window_commits: 0,
                ..WindowPlannerPolicy::default()
            },
        );
        let pinned_pages = BTreeSet::new();
        planner.plan_commit(WindowPlanRequest {
            target_page: 5,
            page_count: 20,
            scroll_direction: ScrollDirection::Still,
            position_in_page_viewports: 0.5,
            pinned_pages: pinned_pages.clone(),
            now_ms: 0,
        });

        assert!(matches!(
            planner.plan_commit(WindowPlanRequest {
                target_page: 7,
                page_count: 20,
                scroll_direction: ScrollDirection::Still,
                position_in_page_viewports: 0.5,
                pinned_pages: pinned_pages.clone(),
                now_ms: 16,
            }),
            WindowPlanDecision::Keep {
                reason: KeepReason::WaitingStableFrames,
                ..
            }
        ));
        assert!(matches!(
            planner.plan_commit(WindowPlanRequest {
                target_page: 7,
                page_count: 20,
                scroll_direction: ScrollDirection::Still,
                position_in_page_viewports: 0.5,
                pinned_pages,
                now_ms: 48,
            }),
            WindowPlanDecision::Commit { page_range } if page_range == (7..8)
        ));
    }

    #[test]
    fn debounces_window_commits_by_min_ms() {
        let mut planner = WindowPlanner::new(
            0,
            0,
            WindowPlannerPolicy {
                min_stable_frames_before_trim: 0,
                min_ms_between_window_commits: 50,
                ..WindowPlannerPolicy::default()
            },
        );
        let pinned_pages = BTreeSet::new();
        planner.plan_commit(WindowPlanRequest {
            target_page: 1,
            page_count: 10,
            scroll_direction: ScrollDirection::Still,
            position_in_page_viewports: 0.5,
            pinned_pages: pinned_pages.clone(),
            now_ms: 100,
        });

        assert!(matches!(
            planner.plan_commit(WindowPlanRequest {
                target_page: 3,
                page_count: 10,
                scroll_direction: ScrollDirection::Still,
                position_in_page_viewports: 0.5,
                pinned_pages,
                now_ms: 120,
            }),
            WindowPlanDecision::Keep {
                reason: KeepReason::CommitDebounced,
                ..
            }
        ));
    }

    #[test]
    fn pinned_pages_are_never_trimmed_by_planner() {
        let planner = WindowPlanner::new(0, 0, WindowPlannerPolicy::default());
        let pinned_pages = BTreeSet::from([2, 9]);
        let range = include_pinned_pages(planner.plan(5, 10), 10, &pinned_pages);

        assert_eq!(range, 2..10);
    }

    #[test]
    fn debug_overlay_exposes_current_window_page_range() {
        let mut planner = WindowPlanner::default();
        planner.plan_commit(WindowPlanRequest {
            target_page: 3,
            page_count: 10,
            scroll_direction: ScrollDirection::Still,
            position_in_page_viewports: 0.5,
            pinned_pages: BTreeSet::new(),
            now_ms: 100,
        });

        let overlay = planner.debug_overlay();

        assert_eq!(overlay.current_page_range, Some(2..5));
        assert_eq!(overlay.last_commit_ms, Some(100));
    }
}
