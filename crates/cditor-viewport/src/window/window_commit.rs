use std::ops::Range;

/// Lifecycle phase for the viewport window currently being presented.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowLoadState {
    CurrentStable,
    PreparingNext,
    ColdPlaceholder,
    Failed,
}

/// Geometry and presentation identity of one bounded document window.
///
/// `visible_block_range` is the minimum readiness core. Overscan blocks may
/// remain cold without allowing a missing viewport block into an atomic swap.
#[derive(Debug, Clone, PartialEq)]
pub struct WindowCommitTarget {
    pub structure_version: u64,
    pub page_range: Range<usize>,
    pub block_range: Range<usize>,
    pub visible_block_range: Range<usize>,
    pub presented_scroll_top: f64,
}

impl WindowCommitTarget {
    fn same_window_as(&self, other: &Self) -> bool {
        self.structure_version == other.structure_version
            && self.page_range == other.page_range
            && self.block_range == other.block_range
            && self.visible_block_range == other.visible_block_range
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum WindowCommitDecision {
    Stable(WindowCommitTarget),
    ColdPlaceholder(WindowCommitTarget),
    FailedTarget {
        target: WindowCommitTarget,
        stable: Option<WindowCommitTarget>,
    },
}

/// Single source of truth for desired, preparing, and stable window identity.
///
/// Storage/layout owners decide readiness; this coordinator only owns the
/// framework-independent lifecycle. A ready target is committed atomically in
/// the same synchronous projection transaction that builds the visible frame.
#[derive(Debug)]
pub struct WindowCommitCoordinator {
    generation: u64,
    state: WindowLoadState,
    desired: Option<WindowCommitTarget>,
    preparing: Option<WindowCommitTarget>,
    stable: Option<WindowCommitTarget>,
}

impl Default for WindowCommitCoordinator {
    fn default() -> Self {
        Self {
            generation: 0,
            state: WindowLoadState::ColdPlaceholder,
            desired: None,
            preparing: None,
            stable: None,
        }
    }
}

impl WindowCommitCoordinator {
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn state(&self) -> WindowLoadState {
        self.state
    }

    pub fn stable(&self) -> Option<&WindowCommitTarget> {
        self.stable.as_ref()
    }

    pub fn preparing(&self) -> Option<&WindowCommitTarget> {
        self.preparing.as_ref()
    }

    pub fn reconcile(
        &mut self,
        desired: WindowCommitTarget,
        desired_ready: bool,
        stable_valid: bool,
        desired_failed: bool,
    ) -> WindowCommitDecision {
        let invalidated_stable = self.stable.is_some() && !stable_valid;
        if !stable_valid {
            self.stable = None;
        }

        let desired_changed = self
            .desired
            .as_ref()
            .is_none_or(|current| !current.same_window_as(&desired));
        if desired_changed || invalidated_stable {
            self.generation = self.generation.saturating_add(1);
        }
        self.desired = Some(desired.clone());

        if desired_ready {
            self.preparing = Some(desired);
            let committed = self
                .preparing
                .take()
                .expect("a ready window target was prepared");
            self.stable = Some(committed.clone());
            self.state = WindowLoadState::CurrentStable;
            return WindowCommitDecision::Stable(committed);
        }

        self.preparing = Some(desired.clone());
        if desired_failed {
            let stable = self.stable.clone();
            self.preparing = None;
            self.state = WindowLoadState::Failed;
            return WindowCommitDecision::FailedTarget {
                target: desired,
                stable,
            };
        }

        if let Some(stable) = self.stable.clone() {
            self.state = WindowLoadState::PreparingNext;
            WindowCommitDecision::Stable(stable)
        } else {
            self.state = WindowLoadState::ColdPlaceholder;
            WindowCommitDecision::ColdPlaceholder(desired)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(start: usize, scroll_top: f64) -> WindowCommitTarget {
        WindowCommitTarget {
            structure_version: 1,
            page_range: start / 10..start / 10 + 1,
            block_range: start..start + 10,
            visible_block_range: start + 2..start + 6,
            presented_scroll_top: scroll_top,
        }
    }

    #[test]
    fn cold_target_stays_placeholder_until_ready_then_commits_atomically() {
        let mut coordinator = WindowCommitCoordinator::default();
        let cold = target(100, 3_200.0);

        assert_eq!(
            coordinator.reconcile(cold.clone(), false, false, false),
            WindowCommitDecision::ColdPlaceholder(cold.clone())
        );
        assert_eq!(coordinator.state(), WindowLoadState::ColdPlaceholder);
        assert_eq!(coordinator.generation(), 1);

        assert_eq!(
            coordinator.reconcile(cold.clone(), true, false, false),
            WindowCommitDecision::Stable(cold.clone())
        );
        assert_eq!(coordinator.stable(), Some(&cold));
        assert_eq!(coordinator.state(), WindowLoadState::CurrentStable);
    }

    #[test]
    fn preparing_remote_target_keeps_the_last_stable_window() {
        let mut coordinator = WindowCommitCoordinator::default();
        let stable = target(0, 0.0);
        let desired = target(100, 3_200.0);
        coordinator.reconcile(stable.clone(), true, false, false);

        assert_eq!(
            coordinator.reconcile(desired.clone(), false, true, false),
            WindowCommitDecision::Stable(stable)
        );
        assert_eq!(coordinator.preparing(), Some(&desired));
        assert_eq!(coordinator.state(), WindowLoadState::PreparingNext);
    }

    #[test]
    fn latest_target_advances_generation_but_scroll_within_one_target_does_not() {
        let mut coordinator = WindowCommitCoordinator::default();
        coordinator.reconcile(target(0, 0.0), true, false, false);
        let first_generation = coordinator.generation();
        coordinator.reconcile(target(0, 12.0), true, true, false);
        assert_eq!(coordinator.generation(), first_generation);

        coordinator.reconcile(target(100, 3_200.0), false, true, false);
        assert_eq!(coordinator.generation(), first_generation + 1);
        coordinator.reconcile(target(200, 6_400.0), false, true, false);
        assert_eq!(coordinator.generation(), first_generation + 2);
    }

    #[test]
    fn terminal_failure_releases_preparing_and_keeps_stable_for_retry() {
        let mut coordinator = WindowCommitCoordinator::default();
        let stable = target(0, 0.0);
        let failed = target(100, 3_200.0);
        coordinator.reconcile(stable.clone(), true, false, false);

        assert_eq!(
            coordinator.reconcile(failed.clone(), false, true, true),
            WindowCommitDecision::FailedTarget {
                target: failed,
                stable: Some(stable.clone()),
            }
        );
        assert_eq!(coordinator.stable(), Some(&stable));
        assert!(coordinator.preparing().is_none());
        assert_eq!(coordinator.state(), WindowLoadState::Failed);
    }

    #[test]
    fn invalidated_stable_window_falls_back_to_cold_placeholder() {
        let mut coordinator = WindowCommitCoordinator::default();
        coordinator.reconcile(target(0, 0.0), true, false, false);
        let generation = coordinator.generation();
        let desired = target(100, 3_200.0);

        assert_eq!(
            coordinator.reconcile(desired.clone(), false, false, false),
            WindowCommitDecision::ColdPlaceholder(desired)
        );
        assert!(coordinator.stable().is_none());
        assert_eq!(coordinator.generation(), generation + 1);
    }
}
