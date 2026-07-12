use std::collections::{HashMap, VecDeque};

use cditor_core::ids::BlockId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkerPoolPolicy {
    pub interactive_lanes: usize,
    pub background_lanes: usize,
    pub max_background_queue: usize,
}

impl Default for WorkerPoolPolicy {
    fn default() -> Self {
        Self {
            interactive_lanes: 2,
            background_lanes: 2,
            max_background_queue: 512,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WorkerTaskKind {
    EditingBlockLayout,
    CompositionBlockLayout,
    CurrentViewportLayout,
    ViewportMeasure,
    RemoteHeightRefinement,
    RemoteTextShaping,
    ImageDecode,
    FtsIndex,
    ThumbnailDecode,
}

impl WorkerTaskKind {
    pub const fn default_lane(self) -> WorkerLane {
        match self {
            Self::EditingBlockLayout
            | Self::CompositionBlockLayout
            | Self::CurrentViewportLayout
            | Self::ViewportMeasure => WorkerLane::Interactive,
            Self::RemoteHeightRefinement
            | Self::RemoteTextShaping
            | Self::ImageDecode
            | Self::FtsIndex
            | Self::ThumbnailDecode => WorkerLane::Background,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WorkerLane {
    Interactive,
    Background,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerTask {
    pub id: u64,
    pub kind: WorkerTaskKind,
    pub lane: WorkerLane,
    pub block_id: Option<BlockId>,
    pub generation: u64,
}

impl WorkerTask {
    pub fn new(id: u64, kind: WorkerTaskKind, block_id: Option<BlockId>, generation: u64) -> Self {
        Self {
            id,
            kind,
            lane: kind.default_lane(),
            block_id,
            generation,
        }
    }

    pub fn with_lane(mut self, lane: WorkerLane) -> Self {
        self.lane = lane;
        self
    }

    fn coalesce_key(&self) -> Option<(WorkerTaskKind, BlockId)> {
        self.block_id.map(|block_id| (self.kind, block_id))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerEnqueueDecision {
    Enqueued(WorkerLane),
    CoalescedNewer,
    DroppedStaleGeneration,
    DroppedQueueFull,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerDispatchBatch {
    pub interactive: Vec<WorkerTask>,
    pub background: Vec<WorkerTask>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerPoolDebugOverlay {
    pub pending_interactive: usize,
    pub pending_background: usize,
    pub interactive_lanes: usize,
    pub background_lanes: usize,
    pub dropped_background: usize,
    pub coalesced_background: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerPoolScheduler {
    policy: WorkerPoolPolicy,
    interactive_queue: VecDeque<WorkerTask>,
    background_queue: VecDeque<WorkerTask>,
    latest_background_generation: HashMap<(WorkerTaskKind, BlockId), u64>,
    dropped_background: usize,
    coalesced_background: usize,
}

impl WorkerPoolScheduler {
    pub fn new(policy: WorkerPoolPolicy) -> Self {
        Self {
            policy,
            interactive_queue: VecDeque::new(),
            background_queue: VecDeque::new(),
            latest_background_generation: HashMap::new(),
            dropped_background: 0,
            coalesced_background: 0,
        }
    }

    pub fn policy(&self) -> WorkerPoolPolicy {
        self.policy
    }

    pub fn enqueue(&mut self, task: WorkerTask) -> WorkerEnqueueDecision {
        match task.lane {
            WorkerLane::Interactive => {
                self.interactive_queue.push_back(task);
                WorkerEnqueueDecision::Enqueued(WorkerLane::Interactive)
            }
            WorkerLane::Background => self.enqueue_background(task),
        }
    }

    fn enqueue_background(&mut self, task: WorkerTask) -> WorkerEnqueueDecision {
        if let Some(key) = task.coalesce_key() {
            let latest = self
                .latest_background_generation
                .get(&key)
                .copied()
                .unwrap_or(0);
            if task.generation < latest {
                self.dropped_background = self.dropped_background.saturating_add(1);
                return WorkerEnqueueDecision::DroppedStaleGeneration;
            }
            if task.generation > latest && latest != 0 {
                self.latest_background_generation
                    .insert(key, task.generation);
                self.background_queue.push_back(task);
                self.coalesced_background = self.coalesced_background.saturating_add(1);
                self.drop_stale_background_entries();
                return WorkerEnqueueDecision::CoalescedNewer;
            }
            self.latest_background_generation
                .insert(key, task.generation);
        }

        if self.background_queue.len() >= self.policy.max_background_queue {
            if self.try_drop_oldest_stale_or_low_value_background() {
                self.background_queue.push_back(task);
                return WorkerEnqueueDecision::Enqueued(WorkerLane::Background);
            }
            self.dropped_background = self.dropped_background.saturating_add(1);
            return WorkerEnqueueDecision::DroppedQueueFull;
        }

        self.background_queue.push_back(task);
        WorkerEnqueueDecision::Enqueued(WorkerLane::Background)
    }

    pub fn dispatch_next(&mut self) -> WorkerDispatchBatch {
        let interactive = pop_n(&mut self.interactive_queue, self.policy.interactive_lanes);
        let background = pop_n(&mut self.background_queue, self.policy.background_lanes);
        WorkerDispatchBatch {
            interactive,
            background,
        }
    }

    pub fn pending_interactive(&self) -> usize {
        self.interactive_queue.len()
    }

    pub fn pending_background(&self) -> usize {
        self.background_queue.len()
    }

    pub fn debug_overlay(&self) -> WorkerPoolDebugOverlay {
        WorkerPoolDebugOverlay {
            pending_interactive: self.pending_interactive(),
            pending_background: self.pending_background(),
            interactive_lanes: self.policy.interactive_lanes,
            background_lanes: self.policy.background_lanes,
            dropped_background: self.dropped_background,
            coalesced_background: self.coalesced_background,
        }
    }

    fn drop_stale_background_entries(&mut self) {
        let mut retained = VecDeque::with_capacity(self.background_queue.len());
        while let Some(task) = self.background_queue.pop_front() {
            let stale = task.coalesce_key().is_some_and(|key| {
                self.latest_background_generation
                    .get(&key)
                    .is_some_and(|latest| task.generation < *latest)
            });
            if stale {
                self.dropped_background = self.dropped_background.saturating_add(1);
            } else {
                retained.push_back(task);
            }
        }
        self.background_queue = retained;
    }

    fn try_drop_oldest_stale_or_low_value_background(&mut self) -> bool {
        self.drop_stale_background_entries();
        if self.background_queue.len() < self.policy.max_background_queue {
            return true;
        }
        if let Some(index) = self.background_queue.iter().position(|task| {
            matches!(
                task.kind,
                WorkerTaskKind::RemoteHeightRefinement | WorkerTaskKind::ThumbnailDecode
            )
        }) {
            self.background_queue.remove(index);
            self.dropped_background = self.dropped_background.saturating_add(1);
            return true;
        }
        false
    }
}

impl Default for WorkerPoolScheduler {
    fn default() -> Self {
        Self::new(WorkerPoolPolicy::default())
    }
}

fn pop_n(queue: &mut VecDeque<WorkerTask>, n: usize) -> Vec<WorkerTask> {
    let mut tasks = Vec::with_capacity(n.min(queue.len()));
    for _ in 0..n {
        let Some(task) = queue.pop_front() else {
            break;
        };
        tasks.push(task);
    }
    tasks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editing_block_and_current_viewport_use_interactive_lanes() {
        let mut scheduler = WorkerPoolScheduler::default();

        assert_eq!(
            scheduler.enqueue(WorkerTask::new(
                1,
                WorkerTaskKind::EditingBlockLayout,
                Some(42),
                1
            )),
            WorkerEnqueueDecision::Enqueued(WorkerLane::Interactive)
        );
        assert_eq!(
            scheduler.enqueue(WorkerTask::new(
                2,
                WorkerTaskKind::CurrentViewportLayout,
                Some(7),
                1
            )),
            WorkerEnqueueDecision::Enqueued(WorkerLane::Interactive)
        );

        let batch = scheduler.dispatch_next();
        assert_eq!(
            batch
                .interactive
                .iter()
                .map(|task| task.id)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert!(batch.background.is_empty());
    }

    #[test]
    fn image_decode_fts_and_remote_refinement_use_background_lanes() {
        let mut scheduler = WorkerPoolScheduler::default();
        scheduler.enqueue(WorkerTask::new(1, WorkerTaskKind::ImageDecode, Some(1), 1));
        scheduler.enqueue(WorkerTask::new(2, WorkerTaskKind::FtsIndex, Some(2), 1));
        scheduler.enqueue(WorkerTask::new(
            3,
            WorkerTaskKind::RemoteHeightRefinement,
            Some(3),
            1,
        ));

        assert_eq!(scheduler.pending_background(), 3);
        assert_eq!(scheduler.pending_interactive(), 0);
        let batch = scheduler.dispatch_next();
        assert_eq!(batch.background.len(), 2);
        assert_eq!(scheduler.pending_background(), 1);
    }

    #[test]
    fn worker_pool_saturation_does_not_block_interactive_tasks() {
        let mut scheduler = WorkerPoolScheduler::new(WorkerPoolPolicy {
            interactive_lanes: 1,
            background_lanes: 1,
            max_background_queue: 10,
        });
        for id in 0..10 {
            scheduler.enqueue(WorkerTask::new(
                id,
                WorkerTaskKind::RemoteTextShaping,
                Some(id),
                1,
            ));
        }
        scheduler.enqueue(WorkerTask::new(
            99,
            WorkerTaskKind::EditingBlockLayout,
            Some(42),
            1,
        ));

        let batch = scheduler.dispatch_next();

        assert_eq!(batch.interactive.len(), 1);
        assert_eq!(batch.interactive[0].id, 99);
        assert_eq!(batch.background.len(), 1);
    }

    #[test]
    fn typing_while_background_indexing_keeps_editing_layout_first() {
        let mut scheduler = WorkerPoolScheduler::new(WorkerPoolPolicy {
            interactive_lanes: 2,
            background_lanes: 1,
            max_background_queue: 100,
        });
        for id in 0..100 {
            scheduler.enqueue(WorkerTask::new(id, WorkerTaskKind::FtsIndex, Some(id), 1));
        }
        scheduler.enqueue(WorkerTask::new(
            10_000,
            WorkerTaskKind::EditingBlockLayout,
            Some(42),
            7,
        ));

        let batch = scheduler.dispatch_next();

        assert_eq!(batch.interactive[0].id, 10_000);
        assert_eq!(batch.background.len(), 1);
        assert!(scheduler.pending_background() > 0);
    }

    #[test]
    fn background_queue_full_drops_old_generation_and_keeps_latest() {
        let mut scheduler = WorkerPoolScheduler::new(WorkerPoolPolicy {
            interactive_lanes: 1,
            background_lanes: 1,
            max_background_queue: 2,
        });
        scheduler.enqueue(WorkerTask::new(
            1,
            WorkerTaskKind::RemoteHeightRefinement,
            Some(42),
            1,
        ));
        assert_eq!(
            scheduler.enqueue(WorkerTask::new(
                2,
                WorkerTaskKind::RemoteHeightRefinement,
                Some(42),
                2
            )),
            WorkerEnqueueDecision::CoalescedNewer
        );
        assert_eq!(
            scheduler.enqueue(WorkerTask::new(
                3,
                WorkerTaskKind::RemoteHeightRefinement,
                Some(42),
                1
            )),
            WorkerEnqueueDecision::DroppedStaleGeneration
        );
        scheduler.enqueue(WorkerTask::new(
            4,
            WorkerTaskKind::RemoteHeightRefinement,
            Some(99),
            1,
        ));

        assert!(scheduler.pending_background() <= 2);
        let overlay = scheduler.debug_overlay();
        assert!(overlay.dropped_background >= 1);
        assert_eq!(overlay.coalesced_background, 1);
    }

    #[test]
    fn debug_overlay_reports_worker_pool_queues() {
        let mut scheduler = WorkerPoolScheduler::new(WorkerPoolPolicy {
            interactive_lanes: 3,
            background_lanes: 4,
            max_background_queue: 8,
        });
        scheduler.enqueue(WorkerTask::new(
            1,
            WorkerTaskKind::EditingBlockLayout,
            Some(1),
            1,
        ));
        scheduler.enqueue(WorkerTask::new(2, WorkerTaskKind::ImageDecode, Some(2), 1));

        let overlay = scheduler.debug_overlay();

        assert_eq!(overlay.pending_interactive, 1);
        assert_eq!(overlay.pending_background, 1);
        assert_eq!(overlay.interactive_lanes, 3);
        assert_eq!(overlay.background_lanes, 4);
    }
}
