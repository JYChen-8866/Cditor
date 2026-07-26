use std::sync::{Arc, Mutex};

use cditor_runtime::{WorkerLane, WorkerPoolPolicy, WorkerTaskKind};

#[derive(Clone)]
pub(crate) struct EditorWorkerAdmission {
    shared: Arc<Mutex<WorkerAdmissionState>>,
}

struct WorkerAdmissionState {
    policy: WorkerPoolPolicy,
    running_interactive: usize,
    running_background: usize,
}

pub(crate) struct WorkerPermit {
    shared: Arc<Mutex<WorkerAdmissionState>>,
    lane: WorkerLane,
}

impl Default for EditorWorkerAdmission {
    fn default() -> Self {
        Self::new(WorkerPoolPolicy::default())
    }
}

impl EditorWorkerAdmission {
    fn new(policy: WorkerPoolPolicy) -> Self {
        Self {
            shared: Arc::new(Mutex::new(WorkerAdmissionState {
                policy,
                running_interactive: 0,
                running_background: 0,
            })),
        }
    }

    pub(crate) fn try_acquire(&self, kind: WorkerTaskKind) -> Option<WorkerPermit> {
        let lane = kind.default_lane();
        let mut state = self.shared.lock().ok()?;
        let (running, limit) = match lane {
            WorkerLane::Interactive => (state.running_interactive, state.policy.interactive_lanes),
            WorkerLane::Background => (state.running_background, state.policy.background_lanes),
        };
        if running >= limit {
            return None;
        }
        match lane {
            WorkerLane::Interactive => {
                state.running_interactive = state.running_interactive.saturating_add(1)
            }
            WorkerLane::Background => {
                state.running_background = state.running_background.saturating_add(1)
            }
        }
        drop(state);
        Some(WorkerPermit {
            shared: self.shared.clone(),
            lane,
        })
    }

    #[cfg(test)]
    fn running(&self) -> (usize, usize) {
        self.shared.lock().map_or((0, 0), |state| {
            (state.running_interactive, state.running_background)
        })
    }
}

impl Drop for WorkerPermit {
    fn drop(&mut self) {
        let Ok(mut state) = self.shared.lock() else {
            return;
        };
        match self.lane {
            WorkerLane::Interactive => {
                state.running_interactive = state.running_interactive.saturating_sub(1)
            }
            WorkerLane::Background => {
                state.running_background = state.running_background.saturating_sub(1)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lanes_have_independent_caps_and_drop_releases_capacity() {
        let admission = EditorWorkerAdmission::new(WorkerPoolPolicy {
            interactive_lanes: 1,
            background_lanes: 1,
            max_background_queue: 0,
        });
        let interactive = admission
            .try_acquire(WorkerTaskKind::EditingBlockLayout)
            .unwrap();
        let background = admission.try_acquire(WorkerTaskKind::ImageDecode).unwrap();
        assert!(
            admission
                .try_acquire(WorkerTaskKind::CurrentViewportLayout)
                .is_none()
        );
        assert!(
            admission
                .try_acquire(WorkerTaskKind::RemoteTextShaping)
                .is_none()
        );
        assert_eq!(admission.running(), (1, 1));

        drop(interactive);
        assert!(
            admission
                .try_acquire(WorkerTaskKind::CurrentViewportLayout)
                .is_some()
        );
        drop(background);
    }
}
