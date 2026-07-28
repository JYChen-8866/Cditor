use std::collections::HashMap;
use std::time::Duration;
use web_time::Instant;

use cditor_core::ids::BlockId;
use cditor_runtime::{
    FrameBudgetState, InteractionMode, MainThreadBudget, MainThreadBudgetArbiter, MainThreadTask,
    MainThreadWorkKind, QueueDecision, TaskOutcome, WorkCost,
};
use gpui::Context;

use crate::editor_view::CditorV2View;

pub(crate) const EDITOR_FRAME_DEADLINE: Duration = Duration::from_micros(16_667);
const INPUT_FRAME_PROTECTION: Duration = Duration::from_millis(120);

pub(crate) struct MainThreadApplyRequest {
    pub(crate) kind: MainThreadWorkKind,
    pub(crate) generation: u64,
    pub(crate) block_id: Option<BlockId>,
    pub(crate) cost: WorkCost,
}

type ApplyCallback = Box<dyn FnOnce(&mut CditorV2View, &mut Context<CditorV2View>) + 'static>;
type CancelCallback = Box<dyn FnOnce() + 'static>;

struct PendingCallbacks {
    apply: ApplyCallback,
    cancel: Option<CancelCallback>,
}

impl PendingCallbacks {
    fn cancel(self) {
        if let Some(cancel) = self.cancel {
            cancel();
        }
    }
}

pub(crate) struct ReadyMainThreadApply {
    task: MainThreadTask,
    callback: ApplyCallback,
    cancel: Option<CancelCallback>,
}

impl ReadyMainThreadApply {
    fn run(self, view: &mut CditorV2View, cx: &mut Context<CditorV2View>) {
        drop(self.cancel);
        (self.callback)(view, cx);
    }

    fn cancel(self) {
        if let Some(cancel) = self.cancel {
            cancel();
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct SchedulerLaneDepths {
    pub(crate) realtime: usize,
    pub(crate) interactive: usize,
    pub(crate) visible: usize,
    pub(crate) prefetch: usize,
    pub(crate) background: usize,
}

#[derive(Default)]
pub(crate) struct EditorMainThreadScheduler {
    arbiter: MainThreadBudgetArbiter,
    callbacks: HashMap<u64, PendingCallbacks>,
    tasks: HashMap<u64, MainThreadTask>,
    latest_generation: HashMap<(MainThreadWorkKind, BlockId), u64>,
    next_task_id: u64,
    wake_scheduled: bool,
    pump_scheduled: bool,
    frame_budget: Option<FrameBudgetState>,
}

impl EditorMainThreadScheduler {
    pub(crate) fn enqueue(
        &mut self,
        kind: MainThreadWorkKind,
        generation: u64,
        block_id: Option<BlockId>,
        cost: WorkCost,
        callback: ApplyCallback,
    ) -> QueueDecision {
        self.enqueue_with_cancel(kind, generation, block_id, cost, callback, None)
    }

    fn enqueue_with_cancel(
        &mut self,
        kind: MainThreadWorkKind,
        generation: u64,
        block_id: Option<BlockId>,
        cost: WorkCost,
        callback: ApplyCallback,
        cancel: Option<CancelCallback>,
    ) -> QueueDecision {
        let id = self.next_task_id;
        self.next_task_id = self.next_task_id.wrapping_add(1);
        let task = MainThreadTask::new(id, kind, generation, block_id, cost);
        if kind.is_drop_stale()
            && let Some(block_id) = block_id
        {
            self.latest_generation
                .entry((kind, block_id))
                .and_modify(|latest| *latest = (*latest).max(generation))
                .or_insert(generation);
        }
        let decision = self.arbiter.enqueue_async_result(task.clone());
        if decision != QueueDecision::DroppedStale {
            self.tasks.insert(id, task);
            self.callbacks.insert(
                id,
                PendingCallbacks {
                    apply: callback,
                    cancel,
                },
            );
        } else if let Some(cancel) = cancel {
            cancel();
        }
        decision
    }

    fn take_ready(
        &mut self,
        mode: InteractionMode,
        budget: MainThreadBudget,
    ) -> Vec<ReadyMainThreadApply> {
        let mut frame_budget = budget.for_mode(mode);
        let result = self.arbiter.run_frame_with_budget(&mut frame_budget);
        self.frame_budget = Some(frame_budget);
        for outcome in &result.outcomes {
            if let TaskOutcome::DroppedStale(id) = outcome {
                self.tasks.remove(id);
                if let Some(callbacks) = self.callbacks.remove(id) {
                    callbacks.cancel();
                }
            }
        }
        result
            .applied
            .into_iter()
            .filter_map(|task| {
                self.tasks.remove(&task.id);
                self.callbacks
                    .remove(&task.id)
                    .map(|callbacks| ReadyMainThreadApply {
                        task,
                        callback: callbacks.apply,
                        cancel: callbacks.cancel,
                    })
            })
            .collect()
    }

    pub(crate) fn try_admit_inline(&mut self, kind: MainThreadWorkKind, cost: WorkCost) -> bool {
        let Some(frame) = self.frame_budget.as_mut() else {
            return false;
        };
        if matches!(
            frame.mode,
            InteractionMode::Typing | InteractionMode::Composing
        ) && kind.is_background()
        {
            return false;
        }
        let task = MainThreadTask::new(0, kind, 0, None, cost);
        if !frame.can_run(&task) {
            return false;
        }
        frame.consume(&task);
        true
    }

    fn requeue(&mut self, apply: ReadyMainThreadApply) {
        let id = apply.task.id;
        let decision = self.arbiter.enqueue_async_result(apply.task.clone());
        if decision != QueueDecision::DroppedStale {
            self.tasks.insert(id, apply.task);
            self.callbacks.insert(
                id,
                PendingCallbacks {
                    apply: apply.callback,
                    cancel: apply.cancel,
                },
            );
        }
    }

    pub(crate) fn pending_len(&self) -> usize {
        self.callbacks.len()
    }

    pub(crate) fn has_pending(
        &self,
        kind: MainThreadWorkKind,
        block_id: BlockId,
        generation: u64,
    ) -> bool {
        self.tasks.values().any(|task| {
            task.kind == kind
                && task.block_id == Some(block_id)
                && task.generation == generation
                && self.callbacks.contains_key(&task.id)
        })
    }

    fn is_stale(&self, task: &MainThreadTask) -> bool {
        task.kind.is_drop_stale()
            && task.block_id.is_some_and(|block_id| {
                self.latest_generation
                    .get(&(task.kind, block_id))
                    .is_some_and(|latest| task.generation < *latest)
            })
    }

    pub(crate) fn lane_depths(&self) -> SchedulerLaneDepths {
        let mut depths = SchedulerLaneDepths::default();
        for task in self.tasks.values() {
            match lane_for_kind(task.kind) {
                SchedulerLane::Realtime => depths.realtime += 1,
                SchedulerLane::Interactive => depths.interactive += 1,
                SchedulerLane::Visible => depths.visible += 1,
                SchedulerLane::Prefetch => depths.prefetch += 1,
                SchedulerLane::Background => depths.background += 1,
            }
        }
        depths
    }

    pub(crate) fn clear(&mut self) {
        self.arbiter = MainThreadBudgetArbiter::default();
        for (_, callbacks) in self.callbacks.drain() {
            callbacks.cancel();
        }
        self.tasks.clear();
        self.latest_generation.clear();
        self.wake_scheduled = false;
        self.pump_scheduled = false;
        self.frame_budget = None;
    }

    fn schedule_wake(&mut self) -> bool {
        if self.wake_scheduled {
            false
        } else {
            self.wake_scheduled = true;
            true
        }
    }

    fn finish_wake(&mut self) {
        self.wake_scheduled = false;
    }

    fn schedule_pump(&mut self) -> bool {
        if self.pump_scheduled {
            false
        } else {
            self.pump_scheduled = true;
            true
        }
    }

    fn finish_pump(&mut self) {
        self.pump_scheduled = false;
    }

    fn pending_wake_delay(&self) -> Duration {
        let needs_next_frame = self.tasks.values().any(|task| {
            matches!(
                lane_for_kind(task.kind),
                SchedulerLane::Realtime | SchedulerLane::Interactive | SchedulerLane::Visible
            )
        });
        if needs_next_frame {
            EDITOR_FRAME_DEADLINE
        } else {
            INPUT_FRAME_PROTECTION
        }
    }
}

impl CditorV2View {
    pub(crate) fn enqueue_main_thread_apply(
        &mut self,
        kind: MainThreadWorkKind,
        generation: u64,
        block_id: Option<BlockId>,
        cost: WorkCost,
        callback: impl FnOnce(&mut Self, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> QueueDecision {
        let decision = self.scheduling.main_thread.enqueue(
            kind,
            generation,
            block_id,
            cost,
            Box::new(callback),
        );
        if decision != QueueDecision::DroppedStale {
            cx.notify();
            self.schedule_main_thread_pump(cx);
        }
        decision
    }

    pub(crate) fn enqueue_main_thread_apply_with_cancel(
        &mut self,
        request: MainThreadApplyRequest,
        callback: impl FnOnce(&mut Self, &mut Context<Self>) + 'static,
        cancel: impl FnOnce() + 'static,
        cx: &mut Context<Self>,
    ) -> QueueDecision {
        let decision = self.scheduling.main_thread.enqueue_with_cancel(
            request.kind,
            request.generation,
            request.block_id,
            request.cost,
            Box::new(callback),
            Some(Box::new(cancel)),
        );
        if decision != QueueDecision::DroppedStale {
            cx.notify();
            self.schedule_main_thread_pump(cx);
        }
        decision
    }

    fn schedule_main_thread_pump(&mut self, cx: &mut Context<Self>) {
        if !self.scheduling.main_thread.schedule_pump() {
            return;
        }
        let pump = cx.background_executor().timer(Duration::ZERO);
        cx.spawn(async move |view, cx| {
            pump.await;
            let _ = view.update(cx, |view, cx| {
                view.scheduling.main_thread.finish_pump();
                view.run_main_thread_applies(Instant::now(), cx);
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn run_main_thread_applies(
        &mut self,
        frame_started: Instant,
        cx: &mut Context<Self>,
    ) {
        let mode = self.main_thread_interaction_mode();
        let ready = self
            .scheduling
            .main_thread
            .take_ready(mode, MainThreadBudget::default());
        let deadline = frame_started + EDITOR_FRAME_DEADLINE;
        let mut ready = ready.into_iter();
        while let Some(apply) = ready.next() {
            if self.scheduling.main_thread.is_stale(&apply.task) {
                apply.cancel();
                continue;
            }
            if Instant::now() >= deadline {
                self.scheduling.main_thread.requeue(apply);
                for deferred in ready {
                    self.scheduling.main_thread.requeue(deferred);
                }
                cx.notify();
                break;
            }
            apply.run(self, cx);
        }
        if self.scheduling.main_thread.pending_len() > 0
            && self.scheduling.main_thread.schedule_wake()
        {
            let wake_delay = self.scheduling.main_thread.pending_wake_delay();
            let wake = cx.background_executor().timer(wake_delay);
            cx.spawn(async move |view, cx| {
                wake.await;
                let _ = view.update(cx, |view, cx| {
                    view.scheduling.main_thread.finish_wake();
                    view.run_main_thread_applies(Instant::now(), cx);
                    cx.notify();
                });
            })
            .detach();
        }
    }

    pub(crate) fn main_thread_interaction_mode(&self) -> InteractionMode {
        interaction_mode_for_signals(
            self.ready_session()
                .and_then(|session| session.input_context().ok())
                .is_some_and(|context| context.composition.is_some()),
            self.interaction.scrollbar_drag.is_some(),
            self.interaction.text_drag_selection.is_some(),
            self.interaction.scroll_accumulator.interaction_state
                != cditor_viewport::scroll::ScrollInteractionState::Idle,
            self.interaction
                .last_input_at
                .is_some_and(|at| at.elapsed() <= INPUT_FRAME_PROTECTION),
        )
    }
}

const fn interaction_mode_for_signals(
    composing: bool,
    scrollbar_dragging: bool,
    selecting: bool,
    wheel_scrolling: bool,
    recently_input: bool,
) -> InteractionMode {
    if composing {
        InteractionMode::Composing
    } else if scrollbar_dragging {
        InteractionMode::ScrollbarDragging
    } else if selecting {
        InteractionMode::Selecting
    } else if wheel_scrolling {
        InteractionMode::WheelScrolling
    } else if recently_input {
        InteractionMode::Typing
    } else {
        InteractionMode::Idle
    }
}

#[derive(Clone, Copy)]
enum SchedulerLane {
    Realtime,
    Interactive,
    Visible,
    Prefetch,
    Background,
}

const fn lane_for_kind(kind: MainThreadWorkKind) -> SchedulerLane {
    match kind {
        MainThreadWorkKind::CompositionCaret
        | MainThreadWorkKind::KeyInput
        | MainThreadWorkKind::EditingTextShape => SchedulerLane::Realtime,
        MainThreadWorkKind::VisibleSelection
        | MainThreadWorkKind::WheelScroll
        | MainThreadWorkKind::CurrentWindowMeasure => SchedulerLane::Interactive,
        MainThreadWorkKind::WindowSwap
        | MainThreadWorkKind::AsyncMeasureApply
        | MainThreadWorkKind::ImageDecodeApply => SchedulerLane::Visible,
        MainThreadWorkKind::Prefetch => SchedulerLane::Prefetch,
        MainThreadWorkKind::PersistenceCallback
        | MainThreadWorkKind::FtsUpdate
        | MainThreadWorkKind::RemoteHeightRefinement => SchedulerLane::Background,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn callback() -> ApplyCallback {
        Box::new(|_, _| {})
    }

    #[test]
    fn lane_depths_report_real_pending_work() {
        let mut scheduler = EditorMainThreadScheduler::default();
        scheduler.enqueue(
            MainThreadWorkKind::CompositionCaret,
            1,
            Some(1),
            WorkCost::ZERO,
            callback(),
        );
        scheduler.enqueue(
            MainThreadWorkKind::WindowSwap,
            1,
            None,
            WorkCost::ZERO,
            callback(),
        );
        scheduler.enqueue(
            MainThreadWorkKind::Prefetch,
            1,
            Some(2),
            WorkCost::ZERO,
            callback(),
        );

        assert_eq!(
            scheduler.lane_depths(),
            SchedulerLaneDepths {
                realtime: 1,
                visible: 1,
                prefetch: 1,
                ..SchedulerLaneDepths::default()
            }
        );
    }

    #[test]
    fn newer_stale_droppable_generation_replaces_older_callback() {
        let mut scheduler = EditorMainThreadScheduler::default();
        scheduler.enqueue(
            MainThreadWorkKind::ImageDecodeApply,
            1,
            Some(7),
            WorkCost::image_decode_apply(),
            callback(),
        );
        scheduler.enqueue(
            MainThreadWorkKind::ImageDecodeApply,
            2,
            Some(7),
            WorkCost::image_decode_apply(),
            callback(),
        );

        let ready = scheduler.take_ready(InteractionMode::Idle, MainThreadBudget::default());

        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].task.generation, 2);
        assert_eq!(scheduler.pending_len(), 0);
    }

    #[test]
    fn typing_defers_background_callbacks() {
        let mut scheduler = EditorMainThreadScheduler::default();
        scheduler.enqueue(
            MainThreadWorkKind::ImageDecodeApply,
            1,
            Some(7),
            WorkCost::image_decode_apply(),
            callback(),
        );

        assert!(
            scheduler
                .take_ready(InteractionMode::Typing, MainThreadBudget::default())
                .is_empty()
        );
        assert_eq!(scheduler.pending_len(), 1);
    }

    #[test]
    fn interaction_mode_uses_transient_input_instead_of_focused_editing_session() {
        assert_eq!(
            interaction_mode_for_signals(false, false, false, false, false),
            InteractionMode::Idle
        );
        assert_eq!(
            interaction_mode_for_signals(false, false, false, false, true),
            InteractionMode::Typing
        );
        assert_eq!(
            interaction_mode_for_signals(true, true, true, true, true),
            InteractionMode::Composing
        );
    }

    #[test]
    fn deferred_queue_schedules_only_one_timer_wake() {
        let mut scheduler = EditorMainThreadScheduler::default();

        assert!(scheduler.schedule_wake());
        assert!(!scheduler.schedule_wake());
        scheduler.finish_wake();
        assert!(scheduler.schedule_wake());
    }

    #[test]
    fn queue_schedules_only_one_foreground_pump() {
        let mut scheduler = EditorMainThreadScheduler::default();

        assert!(scheduler.schedule_pump());
        assert!(!scheduler.schedule_pump());
        scheduler.finish_pump();
        assert!(scheduler.schedule_pump());
    }

    #[test]
    fn visible_work_continues_next_frame_while_background_work_stays_idle_batched() {
        let mut scheduler = EditorMainThreadScheduler::default();
        scheduler.enqueue(
            MainThreadWorkKind::CurrentWindowMeasure,
            1,
            Some(7),
            WorkCost::async_measure(),
            callback(),
        );
        assert_eq!(scheduler.pending_wake_delay(), EDITOR_FRAME_DEADLINE);

        scheduler.clear();
        scheduler.enqueue(
            MainThreadWorkKind::PersistenceCallback,
            1,
            None,
            WorkCost::sync_ms(0.1),
            callback(),
        );
        assert_eq!(scheduler.pending_wake_delay(), INPUT_FRAME_PROTECTION);
    }

    #[test]
    fn stale_and_cleared_callbacks_run_cancellation_cleanup_once() {
        let cancellations = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut scheduler = EditorMainThreadScheduler::default();
        for generation in [1, 2] {
            let cancellations = cancellations.clone();
            scheduler.enqueue_with_cancel(
                MainThreadWorkKind::ImageDecodeApply,
                generation,
                Some(9),
                WorkCost::image_decode_apply(),
                callback(),
                Some(Box::new(move || {
                    cancellations.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                })),
            );
        }

        let ready = scheduler.take_ready(InteractionMode::Idle, MainThreadBudget::default());
        assert_eq!(ready.len(), 1);
        assert_eq!(cancellations.load(std::sync::atomic::Ordering::Relaxed), 1);
        scheduler.requeue(ready.into_iter().next().unwrap());
        scheduler.clear();
        assert_eq!(cancellations.load(std::sync::atomic::Ordering::Relaxed), 2);
    }

    #[test]
    fn selected_task_is_rechecked_when_generation_advances_in_the_same_frame() {
        let mut scheduler = EditorMainThreadScheduler::default();
        scheduler.enqueue(
            MainThreadWorkKind::AsyncMeasureApply,
            4,
            Some(12),
            WorkCost::async_measure(),
            callback(),
        );
        let ready = scheduler.take_ready(InteractionMode::Idle, MainThreadBudget::default());
        scheduler.enqueue(
            MainThreadWorkKind::AsyncMeasureApply,
            5,
            Some(12),
            WorkCost::async_measure(),
            callback(),
        );

        assert!(scheduler.is_stale(&ready[0].task));
    }

    #[test]
    fn inline_work_consumes_the_budget_left_by_queued_callbacks() {
        let mut scheduler = EditorMainThreadScheduler::default();
        scheduler.enqueue(
            MainThreadWorkKind::CurrentWindowMeasure,
            1,
            Some(12),
            WorkCost::sync_ms(5.0),
            callback(),
        );
        let ready = scheduler.take_ready(InteractionMode::Idle, MainThreadBudget::default());

        assert_eq!(ready.len(), 1);
        assert!(!scheduler.try_admit_inline(
            MainThreadWorkKind::CurrentWindowMeasure,
            WorkCost::sync_ms(1.5),
        ));
        assert!(scheduler.try_admit_inline(
            MainThreadWorkKind::CurrentWindowMeasure,
            WorkCost::sync_ms(1.0),
        ));
    }
}
