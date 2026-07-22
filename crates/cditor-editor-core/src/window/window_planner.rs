use std::collections::BTreeSet;
use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollDirection {
    Up,
    Down,
    Still,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WindowMemoryPressure {
    #[default]
    Normal,
    Warning,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowPlannerPolicy {
    pub enter_threshold_viewports: f64,
    pub exit_threshold_viewports: f64,
    pub min_stable_frames_before_trim: u32,
    pub min_ms_between_window_commits: u64,
    pub velocity_page_threshold_viewports_per_second: f64,
    pub max_velocity_prefetch_pages: usize,
}

impl Default for WindowPlannerPolicy {
    fn default() -> Self {
        Self {
            enter_threshold_viewports: 0.5,
            exit_threshold_viewports: 1.0,
            min_stable_frames_before_trim: 2,
            min_ms_between_window_commits: 16,
            velocity_page_threshold_viewports_per_second: 3.0,
            max_velocity_prefetch_pages: 4,
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
    last_velocity_viewports_per_second: f64,
    last_memory_pressure: WindowMemoryPressure,
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
            last_velocity_viewports_per_second: 0.0,
            last_memory_pressure: WindowMemoryPressure::Normal,
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
        self.plan_with_context(
            target_page,
            page_count,
            direction,
            0.0,
            WindowMemoryPressure::Normal,
        )
    }

    pub fn plan_with_context(
        &self,
        target_page: usize,
        page_count: usize,
        direction: ScrollDirection,
        velocity_viewports_per_second: f64,
        memory_pressure: WindowMemoryPressure,
    ) -> Range<usize> {
        let (before_pages, after_pages) = match memory_pressure {
            WindowMemoryPressure::Normal => (self.before_pages, self.after_pages),
            WindowMemoryPressure::Warning => {
                (self.before_pages.div_ceil(2), self.after_pages.div_ceil(2))
            }
            WindowMemoryPressure::Critical => (0, 0),
        };
        let velocity = if velocity_viewports_per_second.is_finite() {
            velocity_viewports_per_second.abs()
        } else {
            0.0
        };
        let threshold = self
            .policy
            .velocity_page_threshold_viewports_per_second
            .max(f64::EPSILON);
        let velocity_pages =
            ((velocity / threshold).floor() as usize).min(self.policy.max_velocity_prefetch_pages);
        let directional_pages = match memory_pressure {
            WindowMemoryPressure::Normal => 1 + velocity_pages,
            WindowMemoryPressure::Warning => 1 + velocity_pages.min(1),
            WindowMemoryPressure::Critical => 0,
        };
        let extra_before = if direction == ScrollDirection::Up {
            directional_pages
        } else {
            0
        };
        let extra_after = if direction == ScrollDirection::Down {
            directional_pages
        } else {
            0
        };
        let start = target_page.saturating_sub(before_pages + extra_before);
        let end = (target_page + after_pages + extra_after + 1).min(page_count);
        start..end
    }

    pub fn plan_commit(&mut self, request: WindowPlanRequest) -> WindowPlanDecision {
        self.last_velocity_viewports_per_second = request.velocity_viewports_per_second;
        self.last_memory_pressure = request.memory_pressure;
        let mut desired = self.plan_with_context(
            request.target_page,
            request.page_count,
            request.scroll_direction,
            request.velocity_viewports_per_second,
            request.memory_pressure,
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

            let pressure_trim = request.memory_pressure != WindowMemoryPressure::Normal
                && desired.start >= current_range.start
                && desired.end <= current_range.end
                && desired != *current_range;
            if !pressure_trim
                && !target_has_crossed_hysteresis(
                    request.target_page,
                    current_range,
                    request.position_in_page_viewports,
                    self.policy.enter_threshold_viewports,
                )
            {
                self.stable_frames = self.stable_frames.saturating_add(1);
                return WindowPlanDecision::Keep {
                    page_range: current_range.clone(),
                    reason: KeepReason::WithinHysteresis,
                };
            }

            if !pressure_trim
                && self.stable_frames.saturating_add(1) < self.policy.min_stable_frames_before_trim
            {
                self.stable_frames = self.stable_frames.saturating_add(1);
                return WindowPlanDecision::Keep {
                    page_range: current_range.clone(),
                    reason: KeepReason::WaitingStableFrames,
                };
            }

            if !pressure_trim
                && let Some(last_commit_ms) = self.last_commit_ms
                && request.now_ms.saturating_sub(last_commit_ms)
                    < self.policy.min_ms_between_window_commits
            {
                return WindowPlanDecision::Keep {
                    page_range: current_range.clone(),
                    reason: KeepReason::CommitDebounced,
                };
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
            last_velocity_viewports_per_second: self.last_velocity_viewports_per_second,
            last_memory_pressure: self.last_memory_pressure,
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
    pub velocity_viewports_per_second: f64,
    pub memory_pressure: WindowMemoryPressure,
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

#[derive(Debug, Clone, PartialEq)]
pub struct WindowPlannerDebugOverlay {
    pub current_page_range: Option<Range<usize>>,
    pub stable_frames: u32,
    pub last_commit_ms: Option<u64>,
    pub last_velocity_viewports_per_second: f64,
    pub last_memory_pressure: WindowMemoryPressure,
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
    fn velocity_expands_only_the_leading_edge_and_pressure_trims_overscan() {
        let planner = WindowPlanner::new(2, 2, WindowPlannerPolicy::default());

        assert_eq!(
            planner.plan_with_context(
                10,
                30,
                ScrollDirection::Down,
                9.0,
                WindowMemoryPressure::Normal,
            ),
            8..17
        );
        assert_eq!(
            planner.plan_with_context(
                10,
                30,
                ScrollDirection::Up,
                9.0,
                WindowMemoryPressure::Warning,
            ),
            7..12
        );
        assert_eq!(
            planner.plan_with_context(
                10,
                30,
                ScrollDirection::Down,
                f64::INFINITY,
                WindowMemoryPressure::Critical,
            ),
            10..11
        );
    }

    #[test]
    fn critical_pressure_trims_immediately_but_never_drops_pinned_pages() {
        let mut planner = WindowPlanner::new(
            3,
            3,
            WindowPlannerPolicy {
                min_stable_frames_before_trim: 10,
                min_ms_between_window_commits: 10_000,
                ..WindowPlannerPolicy::default()
            },
        );
        let first = planner.plan_commit(WindowPlanRequest {
            target_page: 10,
            page_count: 30,
            scroll_direction: ScrollDirection::Still,
            velocity_viewports_per_second: 0.0,
            memory_pressure: WindowMemoryPressure::Normal,
            position_in_page_viewports: 0.5,
            pinned_pages: BTreeSet::new(),
            now_ms: 0,
        });
        assert!(matches!(
            first,
            WindowPlanDecision::Commit { page_range } if page_range == (7..14)
        ));

        let trimmed = planner.plan_commit(WindowPlanRequest {
            target_page: 10,
            page_count: 30,
            scroll_direction: ScrollDirection::Still,
            velocity_viewports_per_second: 0.0,
            memory_pressure: WindowMemoryPressure::Critical,
            position_in_page_viewports: 0.5,
            pinned_pages: BTreeSet::from([9]),
            now_ms: 1,
        });
        assert!(matches!(
            trimmed,
            WindowPlanDecision::Commit { page_range } if page_range == (9..11)
        ));
        let overlay = planner.debug_overlay();
        assert_eq!(overlay.last_memory_pressure, WindowMemoryPressure::Critical);
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
                ..WindowPlannerPolicy::default()
            },
        );
        let pinned_pages = BTreeSet::new();

        assert!(matches!(
            planner.plan_commit(WindowPlanRequest {
                target_page: 10,
                page_count: 100,
                scroll_direction: ScrollDirection::Still,
                velocity_viewports_per_second: 0.0,
                memory_pressure: WindowMemoryPressure::Normal,
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
            velocity_viewports_per_second: 0.0,
            memory_pressure: WindowMemoryPressure::Normal,
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
            velocity_viewports_per_second: 0.0,
            memory_pressure: WindowMemoryPressure::Normal,
            position_in_page_viewports: 0.5,
            pinned_pages: pinned_pages.clone(),
            now_ms: 0,
        });

        assert!(matches!(
            planner.plan_commit(WindowPlanRequest {
                target_page: 7,
                page_count: 20,
                scroll_direction: ScrollDirection::Still,
                velocity_viewports_per_second: 0.0,
                memory_pressure: WindowMemoryPressure::Normal,
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
                velocity_viewports_per_second: 0.0,
                memory_pressure: WindowMemoryPressure::Normal,
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
            velocity_viewports_per_second: 0.0,
            memory_pressure: WindowMemoryPressure::Normal,
            position_in_page_viewports: 0.5,
            pinned_pages: pinned_pages.clone(),
            now_ms: 100,
        });

        assert!(matches!(
            planner.plan_commit(WindowPlanRequest {
                target_page: 3,
                page_count: 10,
                scroll_direction: ScrollDirection::Still,
                velocity_viewports_per_second: 0.0,
                memory_pressure: WindowMemoryPressure::Normal,
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
            velocity_viewports_per_second: 0.0,
            memory_pressure: WindowMemoryPressure::Normal,
            position_in_page_viewports: 0.5,
            pinned_pages: BTreeSet::new(),
            now_ms: 100,
        });

        let overlay = planner.debug_overlay();

        assert_eq!(overlay.current_page_range, Some(2..5));
        assert_eq!(overlay.last_commit_ms, Some(100));
    }
}
