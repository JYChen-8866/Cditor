use std::collections::VecDeque;
use std::ops::Range;

use crate::scheduling::main_thread_budget::{
    FrameBudgetState, InteractionMode, MainThreadBudget, WorkCost,
};
use cditor_core::ids::BlockId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutTaskLane {
    High,
    Normal,
    Idle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutTaskKind {
    EditingBlock,
    CompositionBlock,
    CurrentViewport,
    Overscan,
    EntityCreate,
    MeasureApply,
    HeightCorrection,
    RemoteHeightRefinement,
    PrefetchMeasure,
}

impl LayoutTaskKind {
    pub const fn default_lane(self) -> LayoutTaskLane {
        match self {
            Self::EditingBlock | Self::CompositionBlock | Self::CurrentViewport => {
                LayoutTaskLane::High
            }
            Self::Overscan | Self::EntityCreate | Self::MeasureApply | Self::HeightCorrection => {
                LayoutTaskLane::Normal
            }
            Self::RemoteHeightRefinement | Self::PrefetchMeasure => LayoutTaskLane::Idle,
        }
    }

    pub const fn is_remote_convergence(self) -> bool {
        matches!(self, Self::RemoteHeightRefinement | Self::PrefetchMeasure)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutTask {
    pub id: u64,
    pub block_id: Option<BlockId>,
    pub page_range: Option<Range<usize>>,
    pub kind: LayoutTaskKind,
    pub lane: LayoutTaskLane,
    pub generation: u64,
    pub cost: WorkCost,
}

impl LayoutTask {
    pub fn new(id: u64, kind: LayoutTaskKind, block_id: Option<BlockId>, cost: WorkCost) -> Self {
        Self {
            id,
            block_id,
            page_range: None,
            kind,
            lane: kind.default_lane(),
            generation: 0,
            cost,
        }
    }

    pub fn with_lane(mut self, lane: LayoutTaskLane) -> Self {
        self.lane = lane;
        self
    }

    pub fn with_page_range(mut self, page_range: Range<usize>) -> Self {
        self.page_range = Some(page_range);
        self
    }

    pub fn with_generation(mut self, generation: u64) -> Self {
        self.generation = generation;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutSchedulerConfig {
    pub max_entity_create_per_frame: usize,
    pub max_measure_apply_per_frame: usize,
    pub max_height_corrections_per_frame: usize,
    pub max_background_queue: usize,
}

impl Default for LayoutSchedulerConfig {
    fn default() -> Self {
        Self {
            max_entity_create_per_frame: 24,
            max_measure_apply_per_frame: 64,
            max_height_corrections_per_frame: 32,
            max_background_queue: 512,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScheduleDecision {
    Enqueued(LayoutTaskLane),
    DroppedByBackpressure,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutTaskOutcome {
    Ran(u64),
    Deferred(u64),
    DroppedByBackpressure(u64),
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutFrameResult {
    pub ran: Vec<LayoutTask>,
    pub outcomes: Vec<LayoutTaskOutcome>,
    pub budget_exhausted: bool,
    pub debug_overlay: LayoutSchedulerDebugOverlay,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutSchedulerDebugOverlay {
    pub interaction_mode: InteractionMode,
    pub pending_high: usize,
    pub pending_normal: usize,
    pub pending_idle: usize,
    pub ran_this_frame: usize,
    pub deferred_this_frame: usize,
    pub backpressure_dropped: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutScheduler {
    high_priority: VecDeque<LayoutTask>,
    normal_priority: VecDeque<LayoutTask>,
    idle_priority: VecDeque<LayoutTask>,
    pub config: LayoutSchedulerConfig,
    pub interaction_mode: InteractionMode,
    backpressure_dropped: usize,
}

impl LayoutScheduler {
    pub fn new(config: LayoutSchedulerConfig, interaction_mode: InteractionMode) -> Self {
        Self {
            high_priority: VecDeque::new(),
            normal_priority: VecDeque::new(),
            idle_priority: VecDeque::new(),
            config,
            interaction_mode,
            backpressure_dropped: 0,
        }
    }

    pub fn schedule(&mut self, task: LayoutTask) -> ScheduleDecision {
        let lane = task.lane;
        if lane == LayoutTaskLane::Idle
            && self.idle_priority.len() >= self.config.max_background_queue
        {
            self.backpressure_dropped = self.backpressure_dropped.saturating_add(1);
            return ScheduleDecision::DroppedByBackpressure;
        }
        match lane {
            LayoutTaskLane::High => self.high_priority.push_back(task),
            LayoutTaskLane::Normal => self.normal_priority.push_back(task),
            LayoutTaskLane::Idle => self.idle_priority.push_back(task),
        }
        ScheduleDecision::Enqueued(lane)
    }

    pub fn pending_high(&self) -> usize {
        self.high_priority.len()
    }

    pub fn pending_normal(&self) -> usize {
        self.normal_priority.len()
    }

    pub fn pending_idle(&self) -> usize {
        self.idle_priority.len()
    }

    pub fn run_frame(&mut self, budget: MainThreadBudget) -> LayoutFrameResult {
        let mut frame_budget = self
            .apply_scheduler_caps(budget)
            .for_mode(self.interaction_mode);
        let mut ran = Vec::new();
        let mut outcomes = Vec::new();
        let mut deferred_this_frame = 0;
        let mut budget_exhausted = false;

        for lane in [
            LayoutTaskLane::High,
            LayoutTaskLane::Normal,
            LayoutTaskLane::Idle,
        ] {
            if lane == LayoutTaskLane::Idle && self.interaction_mode != InteractionMode::Idle {
                deferred_this_frame += self.idle_priority.len();
                continue;
            }

            loop {
                let Some(task) = self.pop_front(lane) else {
                    break;
                };

                if self.should_defer_for_interaction(&task) {
                    let id = task.id;
                    self.push_front(lane, task);
                    outcomes.push(LayoutTaskOutcome::Deferred(id));
                    deferred_this_frame += 1;
                    break;
                }

                if can_run_layout_task(&frame_budget, &task) {
                    frame_budget.consume_layout_task(&task);
                    outcomes.push(LayoutTaskOutcome::Ran(task.id));
                    ran.push(task);
                } else {
                    let id = task.id;
                    self.push_front(lane, task);
                    outcomes.push(LayoutTaskOutcome::Deferred(id));
                    deferred_this_frame += 1;
                    budget_exhausted = true;
                    break;
                }
            }

            if budget_exhausted {
                break;
            }
        }

        let debug_overlay = LayoutSchedulerDebugOverlay {
            interaction_mode: self.interaction_mode,
            pending_high: self.pending_high(),
            pending_normal: self.pending_normal(),
            pending_idle: self.pending_idle(),
            ran_this_frame: ran.len(),
            deferred_this_frame,
            backpressure_dropped: self.backpressure_dropped,
        };

        LayoutFrameResult {
            ran,
            outcomes,
            budget_exhausted,
            debug_overlay,
        }
    }

    fn apply_scheduler_caps(&self, mut budget: MainThreadBudget) -> MainThreadBudget {
        budget.entity_create_budget = budget
            .entity_create_budget
            .min(self.config.max_entity_create_per_frame);
        budget.measure_apply_budget = budget
            .measure_apply_budget
            .min(self.config.max_measure_apply_per_frame);
        budget.height_correction_budget = budget
            .height_correction_budget
            .min(self.config.max_height_corrections_per_frame);
        budget
    }

    fn should_defer_for_interaction(&self, task: &LayoutTask) -> bool {
        if task.kind.is_remote_convergence() && self.interaction_mode != InteractionMode::Idle {
            return true;
        }
        if self.interaction_mode == InteractionMode::ScrollbarDragging
            && matches!(
                task.kind,
                LayoutTaskKind::Overscan | LayoutTaskKind::PrefetchMeasure
            )
        {
            return true;
        }
        false
    }

    fn pop_front(&mut self, lane: LayoutTaskLane) -> Option<LayoutTask> {
        match lane {
            LayoutTaskLane::High => self.high_priority.pop_front(),
            LayoutTaskLane::Normal => self.normal_priority.pop_front(),
            LayoutTaskLane::Idle => self.idle_priority.pop_front(),
        }
    }

    fn push_front(&mut self, lane: LayoutTaskLane, task: LayoutTask) {
        match lane {
            LayoutTaskLane::High => self.high_priority.push_front(task),
            LayoutTaskLane::Normal => self.normal_priority.push_front(task),
            LayoutTaskLane::Idle => self.idle_priority.push_front(task),
        }
    }
}

impl Default for LayoutScheduler {
    fn default() -> Self {
        Self::new(LayoutSchedulerConfig::default(), InteractionMode::Idle)
    }
}

trait LayoutFrameBudgetExt {
    fn can_run_layout_task(&self, task: &LayoutTask) -> bool;
    fn consume_layout_task(&mut self, task: &LayoutTask);
}

impl LayoutFrameBudgetExt for FrameBudgetState {
    fn can_run_layout_task(&self, task: &LayoutTask) -> bool {
        self.consumed.sync_ms + task.cost.sync_ms <= self.budget.layout_budget_ms
            && self.consumed.entity_creates + task.cost.entity_creates
                <= self.budget.entity_create_budget
            && self.consumed.measure_applies + task.cost.measure_applies
                <= self.budget.measure_apply_budget
            && self.consumed.height_corrections + task.cost.height_corrections
                <= self.budget.height_correction_budget
            && self.consumed.window_diff_items + task.cost.window_diff_items
                <= self.budget.window_diff_budget
    }

    fn consume_layout_task(&mut self, task: &LayoutTask) {
        self.consumed.sync_ms += task.cost.sync_ms;
        self.consumed.entity_creates += task.cost.entity_creates;
        self.consumed.measure_applies += task.cost.measure_applies;
        self.consumed.height_corrections += task.cost.height_corrections;
        self.consumed.window_diff_items += task.cost.window_diff_items;
    }
}

fn can_run_layout_task(frame_budget: &FrameBudgetState, task: &LayoutTask) -> bool {
    frame_budget.can_run_layout_task(task)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheduler_priority_keeps_editing_block_before_overscan_and_idle() {
        let mut scheduler = LayoutScheduler::default();
        scheduler.schedule(LayoutTask::new(
            1,
            LayoutTaskKind::RemoteHeightRefinement,
            Some(99),
            WorkCost::async_measure(),
        ));
        scheduler.schedule(LayoutTask::new(
            2,
            LayoutTaskKind::Overscan,
            Some(10),
            WorkCost::async_measure(),
        ));
        scheduler.schedule(LayoutTask::new(
            3,
            LayoutTaskKind::EditingBlock,
            Some(42),
            WorkCost::sync_ms(0.2),
        ));

        let result = scheduler.run_frame(MainThreadBudget::default());

        assert_eq!(
            result.ran.iter().map(|task| task.id).collect::<Vec<_>>(),
            vec![3, 2, 1]
        );
    }

    #[test]
    fn frame_budget_exhaustion_stops_measure_apply_work() {
        let mut scheduler = LayoutScheduler::new(
            LayoutSchedulerConfig {
                max_measure_apply_per_frame: 4,
                ..LayoutSchedulerConfig::default()
            },
            InteractionMode::Idle,
        );
        for id in 0..20 {
            scheduler.schedule(LayoutTask::new(
                id,
                LayoutTaskKind::MeasureApply,
                Some(id),
                WorkCost::async_measure(),
            ));
        }

        let result = scheduler.run_frame(MainThreadBudget::default());

        assert_eq!(result.ran.len(), 4);
        assert!(result.budget_exhausted);
        assert_eq!(scheduler.pending_normal(), 16);
    }

    #[test]
    fn wheel_scrolling_does_not_measure_1000_blocks_in_one_frame() {
        let mut scheduler = LayoutScheduler::new(
            LayoutSchedulerConfig::default(),
            InteractionMode::WheelScrolling,
        );
        for id in 0..1_000 {
            scheduler.schedule(LayoutTask::new(
                id,
                LayoutTaskKind::MeasureApply,
                Some(id),
                WorkCost::async_measure(),
            ));
        }

        let result = scheduler.run_frame(MainThreadBudget::default());

        assert!(result.ran.len() <= 8);
        assert!(scheduler.pending_normal() >= 992);
    }

    #[test]
    fn remote_convergence_runs_only_when_idle() {
        let mut scheduler =
            LayoutScheduler::new(LayoutSchedulerConfig::default(), InteractionMode::Typing);
        scheduler.schedule(LayoutTask::new(
            1,
            LayoutTaskKind::RemoteHeightRefinement,
            Some(50),
            WorkCost::async_measure(),
        ));

        let typing_result = scheduler.run_frame(MainThreadBudget::default());
        assert!(typing_result.ran.is_empty());
        assert_eq!(scheduler.pending_idle(), 1);

        scheduler.interaction_mode = InteractionMode::Idle;
        let idle_result = scheduler.run_frame(MainThreadBudget::default());
        assert_eq!(idle_result.ran.len(), 1);
    }

    #[test]
    fn backpressure_drops_remote_background_queue_overflow() {
        let mut scheduler = LayoutScheduler::new(
            LayoutSchedulerConfig {
                max_background_queue: 2,
                ..LayoutSchedulerConfig::default()
            },
            InteractionMode::Idle,
        );

        assert_eq!(
            scheduler.schedule(LayoutTask::new(
                1,
                LayoutTaskKind::RemoteHeightRefinement,
                Some(1),
                WorkCost::async_measure()
            )),
            ScheduleDecision::Enqueued(LayoutTaskLane::Idle)
        );
        assert_eq!(
            scheduler.schedule(LayoutTask::new(
                2,
                LayoutTaskKind::PrefetchMeasure,
                Some(2),
                WorkCost::async_measure()
            )),
            ScheduleDecision::Enqueued(LayoutTaskLane::Idle)
        );
        assert_eq!(
            scheduler.schedule(LayoutTask::new(
                3,
                LayoutTaskKind::RemoteHeightRefinement,
                Some(3),
                WorkCost::async_measure()
            )),
            ScheduleDecision::DroppedByBackpressure
        );

        let overlay = scheduler
            .run_frame(MainThreadBudget::default())
            .debug_overlay;
        assert_eq!(overlay.backpressure_dropped, 1);
    }

    #[test]
    fn debug_overlay_reports_pending_layout_tasks() {
        let mut scheduler = LayoutScheduler::new(
            LayoutSchedulerConfig::default(),
            InteractionMode::WheelScrolling,
        );
        scheduler.schedule(LayoutTask::new(
            1,
            LayoutTaskKind::EditingBlock,
            Some(1),
            WorkCost::sync_ms(0.1),
        ));
        scheduler.schedule(LayoutTask::new(
            2,
            LayoutTaskKind::MeasureApply,
            Some(2),
            WorkCost::async_measure(),
        ));
        scheduler.schedule(LayoutTask::new(
            3,
            LayoutTaskKind::RemoteHeightRefinement,
            Some(3),
            WorkCost::async_measure(),
        ));

        let overlay = scheduler
            .run_frame(MainThreadBudget::default())
            .debug_overlay;

        assert_eq!(overlay.interaction_mode, InteractionMode::WheelScrolling);
        assert_eq!(overlay.ran_this_frame, 2);
        assert_eq!(overlay.pending_idle, 1);
    }
}
