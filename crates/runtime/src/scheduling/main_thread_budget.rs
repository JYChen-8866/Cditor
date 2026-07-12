use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

use cditor_core::ids::BlockId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractionMode {
    Idle,
    Typing,
    Composing,
    WheelScrolling,
    ScrollbarDragging,
    Selecting,
    Pasting,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MainThreadBudget {
    pub input_reserved_ms: f64,
    pub paint_reserved_ms: f64,
    pub layout_budget_ms: f64,
    pub entity_create_budget: usize,
    pub measure_apply_budget: usize,
    pub window_diff_budget: usize,
    pub height_correction_budget: usize,
    pub async_result_budget: usize,
}

impl Default for MainThreadBudget {
    fn default() -> Self {
        Self {
            input_reserved_ms: 4.0,
            paint_reserved_ms: 4.0,
            layout_budget_ms: 6.0,
            entity_create_budget: 24,
            measure_apply_budget: 64,
            window_diff_budget: 128,
            height_correction_budget: 32,
            async_result_budget: 64,
        }
    }
}

impl MainThreadBudget {
    pub fn for_mode(self, mode: InteractionMode) -> FrameBudgetState {
        let mut budget = self;
        match mode {
            InteractionMode::Typing | InteractionMode::Composing => {
                budget.layout_budget_ms =
                    (budget.layout_budget_ms - budget.input_reserved_ms).max(1.0);
                budget.async_result_budget = budget.async_result_budget.min(4);
                budget.measure_apply_budget = budget.measure_apply_budget.min(4);
                budget.height_correction_budget = budget.height_correction_budget.min(2);
            }
            InteractionMode::WheelScrolling => {
                budget.window_diff_budget = budget.window_diff_budget.min(32);
                budget.entity_create_budget = budget.entity_create_budget.min(8);
                budget.measure_apply_budget = budget.measure_apply_budget.min(8);
                budget.height_correction_budget = budget.height_correction_budget.min(4);
                budget.async_result_budget = budget.async_result_budget.min(8);
            }
            InteractionMode::ScrollbarDragging => {
                budget.window_diff_budget = budget.window_diff_budget.min(8);
                budget.entity_create_budget = budget.entity_create_budget.min(4);
                budget.measure_apply_budget = budget.measure_apply_budget.min(4);
                budget.height_correction_budget = budget.height_correction_budget.min(2);
                budget.async_result_budget = budget.async_result_budget.min(4);
            }
            InteractionMode::Idle | InteractionMode::Selecting | InteractionMode::Pasting => {}
        }
        FrameBudgetState::new(budget, mode)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MainThreadWorkKind {
    CompositionCaret,
    KeyInput,
    VisibleSelection,
    WheelScroll,
    CurrentWindowMeasure,
    WindowSwap,
    AsyncMeasureApply,
    Prefetch,
    PersistenceCallback,
    FtsUpdate,
    RemoteHeightRefinement,
    ImageDecodeApply,
}

impl MainThreadWorkKind {
    pub const fn priority(self) -> u8 {
        match self {
            Self::CompositionCaret => 100,
            Self::KeyInput => 90,
            Self::VisibleSelection => 80,
            Self::WheelScroll => 70,
            Self::CurrentWindowMeasure => 60,
            Self::WindowSwap => 50,
            Self::AsyncMeasureApply => 40,
            Self::Prefetch => 30,
            Self::PersistenceCallback => 20,
            Self::FtsUpdate => 10,
            Self::RemoteHeightRefinement => 5,
            Self::ImageDecodeApply => 35,
        }
    }

    pub const fn is_drop_stale(self) -> bool {
        matches!(
            self,
            Self::RemoteHeightRefinement
                | Self::Prefetch
                | Self::FtsUpdate
                | Self::ImageDecodeApply
        )
    }

    pub const fn is_background(self) -> bool {
        matches!(
            self,
            Self::Prefetch
                | Self::PersistenceCallback
                | Self::FtsUpdate
                | Self::RemoteHeightRefinement
                | Self::ImageDecodeApply
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorkCost {
    pub sync_ms: f64,
    pub entity_creates: usize,
    pub measure_applies: usize,
    pub window_diff_items: usize,
    pub height_corrections: usize,
    pub async_results: usize,
}

impl WorkCost {
    pub const ZERO: Self = Self {
        sync_ms: 0.0,
        entity_creates: 0,
        measure_applies: 0,
        window_diff_items: 0,
        height_corrections: 0,
        async_results: 0,
    };

    pub const fn sync_ms(sync_ms: f64) -> Self {
        Self {
            sync_ms,
            ..Self::ZERO
        }
    }

    pub const fn async_measure() -> Self {
        Self {
            sync_ms: 0.08,
            measure_applies: 1,
            async_results: 1,
            ..Self::ZERO
        }
    }

    pub const fn image_decode_apply() -> Self {
        Self {
            sync_ms: 0.15,
            measure_applies: 1,
            async_results: 1,
            ..Self::ZERO
        }
    }

    pub const fn fts_update() -> Self {
        Self {
            sync_ms: 0.05,
            async_results: 1,
            ..Self::ZERO
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MainThreadTask {
    pub id: u64,
    pub kind: MainThreadWorkKind,
    pub generation: u64,
    pub block_id: Option<BlockId>,
    pub cost: WorkCost,
    pub enqueued_order: u64,
}

impl MainThreadTask {
    pub fn new(
        id: u64,
        kind: MainThreadWorkKind,
        generation: u64,
        block_id: Option<BlockId>,
        cost: WorkCost,
    ) -> Self {
        Self {
            id,
            kind,
            generation,
            block_id,
            cost,
            enqueued_order: 0,
        }
    }

    fn coalesce_key(&self) -> Option<(MainThreadWorkKind, BlockId)> {
        self.block_id.map(|block_id| (self.kind, block_id))
    }
}

impl Eq for MainThreadTask {}

impl Ord for MainThreadTask {
    fn cmp(&self, other: &Self) -> Ordering {
        self.kind
            .priority()
            .cmp(&other.kind.priority())
            .then_with(|| other.enqueued_order.cmp(&self.enqueued_order))
    }
}

impl PartialOrd for MainThreadTask {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FrameBudgetState {
    pub budget: MainThreadBudget,
    pub mode: InteractionMode,
    pub consumed: WorkCost,
}

impl FrameBudgetState {
    fn new(budget: MainThreadBudget, mode: InteractionMode) -> Self {
        Self {
            budget,
            mode,
            consumed: WorkCost::ZERO,
        }
    }

    pub fn can_run(&self, task: &MainThreadTask) -> bool {
        self.consumed.sync_ms + task.cost.sync_ms <= self.budget.layout_budget_ms
            && self.consumed.entity_creates + task.cost.entity_creates
                <= self.budget.entity_create_budget
            && self.consumed.measure_applies + task.cost.measure_applies
                <= self.budget.measure_apply_budget
            && self.consumed.window_diff_items + task.cost.window_diff_items
                <= self.budget.window_diff_budget
            && self.consumed.height_corrections + task.cost.height_corrections
                <= self.budget.height_correction_budget
            && self.consumed.async_results + task.cost.async_results
                <= self.budget.async_result_budget
    }

    pub fn consume(&mut self, task: &MainThreadTask) {
        self.consumed.sync_ms += task.cost.sync_ms;
        self.consumed.entity_creates += task.cost.entity_creates;
        self.consumed.measure_applies += task.cost.measure_applies;
        self.consumed.window_diff_items += task.cost.window_diff_items;
        self.consumed.height_corrections += task.cost.height_corrections;
        self.consumed.async_results += task.cost.async_results;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueueDecision {
    Enqueued,
    CoalescedNewer,
    DroppedStale,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskOutcome {
    Applied(u64),
    Deferred(u64),
    DroppedStale(u64),
}

#[derive(Debug, Clone, PartialEq)]
pub struct FrameRunResult {
    pub applied: Vec<MainThreadTask>,
    pub outcomes: Vec<TaskOutcome>,
    pub remaining_queue_len: usize,
    pub consumed: WorkCost,
}

#[derive(Debug, Default)]
pub struct MainThreadBudgetArbiter {
    heap: BinaryHeap<MainThreadTask>,
    latest_generation: HashMap<(MainThreadWorkKind, BlockId), u64>,
    next_order: u64,
}

impl MainThreadBudgetArbiter {
    pub fn enqueue_async_result(&mut self, mut task: MainThreadTask) -> QueueDecision {
        task.enqueued_order = self.next_order;
        self.next_order = self.next_order.saturating_add(1);

        if let Some(key) = task.coalesce_key() {
            let latest = self.latest_generation.get(&key).copied().unwrap_or(0);
            if task.generation < latest && task.kind.is_drop_stale() {
                return QueueDecision::DroppedStale;
            }
            if task.generation >= latest {
                self.latest_generation.insert(key, task.generation);
                self.heap.push(task);
                return if latest == 0 {
                    QueueDecision::Enqueued
                } else {
                    QueueDecision::CoalescedNewer
                };
            }
        }

        self.heap.push(task);
        QueueDecision::Enqueued
    }

    pub fn queue_len(&self) -> usize {
        self.heap.len()
    }

    pub fn run_frame(&mut self, budget: MainThreadBudget, mode: InteractionMode) -> FrameRunResult {
        let mut frame = budget.for_mode(mode);
        let mut deferred = Vec::new();
        let mut applied = Vec::new();
        let mut outcomes = Vec::new();

        while let Some(task) = self.heap.pop() {
            if self.is_stale(&task) {
                outcomes.push(TaskOutcome::DroppedStale(task.id));
                continue;
            }

            if self.should_protect_input_frame(mode, &task) {
                outcomes.push(TaskOutcome::Deferred(task.id));
                deferred.push(task);
                continue;
            }

            if frame.can_run(&task) {
                frame.consume(&task);
                outcomes.push(TaskOutcome::Applied(task.id));
                applied.push(task);
            } else {
                outcomes.push(TaskOutcome::Deferred(task.id));
                deferred.push(task);
                break;
            }
        }

        for task in deferred {
            self.heap.push(task);
        }

        FrameRunResult {
            applied,
            outcomes,
            remaining_queue_len: self.heap.len(),
            consumed: frame.consumed,
        }
    }

    fn is_stale(&self, task: &MainThreadTask) -> bool {
        let Some(key) = task.coalesce_key() else {
            return false;
        };
        task.kind.is_drop_stale()
            && self
                .latest_generation
                .get(&key)
                .is_some_and(|latest| task.generation < *latest)
    }

    fn should_protect_input_frame(&self, mode: InteractionMode, task: &MainThreadTask) -> bool {
        matches!(mode, InteractionMode::Typing | InteractionMode::Composing)
            && task.kind.is_background()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_frame_defers_background_fts_but_allows_key_input() {
        let mut arbiter = MainThreadBudgetArbiter::default();
        arbiter.enqueue_async_result(MainThreadTask::new(
            1,
            MainThreadWorkKind::FtsUpdate,
            1,
            Some(42),
            WorkCost::fts_update(),
        ));
        arbiter.enqueue_async_result(MainThreadTask::new(
            2,
            MainThreadWorkKind::KeyInput,
            1,
            Some(42),
            WorkCost::sync_ms(0.5),
        ));

        let result = arbiter.run_frame(MainThreadBudget::default(), InteractionMode::Typing);

        assert_eq!(
            result
                .applied
                .iter()
                .map(|task| task.id)
                .collect::<Vec<_>>(),
            vec![2]
        );
        assert!(result.outcomes.contains(&TaskOutcome::Deferred(1)));
        assert_eq!(result.remaining_queue_len, 1);
    }

    #[test]
    fn async_results_are_queued_and_limited_per_frame() {
        let mut arbiter = MainThreadBudgetArbiter::default();
        for id in 0..1_000 {
            arbiter.enqueue_async_result(MainThreadTask::new(
                id,
                MainThreadWorkKind::AsyncMeasureApply,
                1,
                Some(id as BlockId),
                WorkCost::async_measure(),
            ));
        }
        let budget = MainThreadBudget {
            async_result_budget: 16,
            measure_apply_budget: 16,
            ..MainThreadBudget::default()
        };

        let result = arbiter.run_frame(budget, InteractionMode::Idle);

        assert_eq!(result.applied.len(), 16);
        assert_eq!(result.consumed.async_results, 16);
        assert_eq!(result.remaining_queue_len, 984);
    }

    #[test]
    fn wheel_scrolling_limits_window_diff_entity_create_measure_and_corrections() {
        let mut arbiter = MainThreadBudgetArbiter::default();
        for id in 0..20 {
            arbiter.enqueue_async_result(MainThreadTask::new(
                id,
                MainThreadWorkKind::ImageDecodeApply,
                1,
                Some(id as BlockId),
                WorkCost::image_decode_apply(),
            ));
        }

        let result =
            arbiter.run_frame(MainThreadBudget::default(), InteractionMode::WheelScrolling);

        assert!(result.applied.len() <= 8);
        assert!(result.consumed.measure_applies <= 8);
        assert!(result.remaining_queue_len >= 12);
    }

    #[test]
    fn remote_refinement_coalesces_and_drops_stale_generation() {
        let mut arbiter = MainThreadBudgetArbiter::default();
        assert_eq!(
            arbiter.enqueue_async_result(MainThreadTask::new(
                1,
                MainThreadWorkKind::RemoteHeightRefinement,
                1,
                Some(42),
                WorkCost::async_measure(),
            )),
            QueueDecision::Enqueued
        );
        assert_eq!(
            arbiter.enqueue_async_result(MainThreadTask::new(
                2,
                MainThreadWorkKind::RemoteHeightRefinement,
                3,
                Some(42),
                WorkCost::async_measure(),
            )),
            QueueDecision::CoalescedNewer
        );
        assert_eq!(
            arbiter.enqueue_async_result(MainThreadTask::new(
                3,
                MainThreadWorkKind::RemoteHeightRefinement,
                2,
                Some(42),
                WorkCost::async_measure(),
            )),
            QueueDecision::DroppedStale
        );

        let result = arbiter.run_frame(MainThreadBudget::default(), InteractionMode::Idle);

        assert!(result.outcomes.contains(&TaskOutcome::DroppedStale(1)));
        assert!(result.outcomes.contains(&TaskOutcome::Applied(2)));
    }

    #[test]
    fn composition_frame_preserves_input_reserved_budget_from_remote_results() {
        let mut arbiter = MainThreadBudgetArbiter::default();
        arbiter.enqueue_async_result(MainThreadTask::new(
            1,
            MainThreadWorkKind::CompositionCaret,
            1,
            Some(42),
            WorkCost::sync_ms(0.2),
        ));
        arbiter.enqueue_async_result(MainThreadTask::new(
            2,
            MainThreadWorkKind::RemoteHeightRefinement,
            1,
            Some(99),
            WorkCost::async_measure(),
        ));

        let result = arbiter.run_frame(MainThreadBudget::default(), InteractionMode::Composing);

        assert_eq!(
            result
                .applied
                .iter()
                .map(|task| task.id)
                .collect::<Vec<_>>(),
            vec![1]
        );
        assert!(result.outcomes.contains(&TaskOutcome::Deferred(2)));
    }
}
