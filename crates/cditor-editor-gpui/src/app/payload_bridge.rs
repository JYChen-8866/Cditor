use std::ops::Range;
use web_time::Instant;

use gpui::{AppContext, Context};

use cditor_runtime::content::payload_preparation::prepare_payload_records;
use cditor_runtime::content::payload_window::{PayloadWindowLoadRequest, PayloadWindowLoadResult};
use cditor_runtime::{MainThreadWorkKind, SelectionMaterializationRequest, WorkCost};
use cditor_session::{
    PayloadStorageRequest, PayloadWindowTaskSchedule, SessionTaskAdmission, SessionTaskKind,
    SessionTaskToken, run_payload_load,
};

use crate::editor_view::CditorV2View;
use crate::input::GuiInputCommand;

fn prefetch_payload_commit_cost(record_count: usize, missing_count: usize) -> WorkCost {
    let payload_count = record_count.saturating_add(missing_count);
    WorkCost {
        sync_ms: (0.15 + payload_count as f64 * 0.002).clamp(0.15, 2.0),
        async_results: 1,
        ..WorkCost::ZERO
    }
}

impl CditorV2View {
    pub(crate) fn schedule_storage_payload_window(
        &mut self,
        storage_request: PayloadStorageRequest,
        block_range: Range<usize>,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.ready_session().cloned() else {
            return;
        };
        let schedule = session.schedule_payload_window_task(block_range.clone(), Instant::now());
        match schedule {
            Ok(PayloadWindowTaskSchedule::Dispatch { token, request }) => {
                crate::diagnostics::payload_pipeline::trace_payload(
                    "schedule.dispatch",
                    format_args!(
                        "generation={} range={:?} ids={}",
                        request.generation,
                        request.block_range,
                        request.block_ids.len()
                    ),
                );
                self.load_storage_payload_window(storage_request, token, request, cx);
            }
            Ok(PayloadWindowTaskSchedule::WakeAfter(delay)) => {
                crate::diagnostics::payload_pipeline::trace_payload_state(
                    "schedule.debounce",
                    format_args!(
                        "range={block_range:?} delay_ms={:.2}",
                        delay.as_secs_f64() * 1000.0
                    ),
                );
                self.schedule_storage_payload_window_wake(delay, cx);
            }
            Ok(PayloadWindowTaskSchedule::WakeAlreadyScheduled) => {
                crate::diagnostics::payload_pipeline::trace_payload_state(
                    "schedule.wake-pending",
                    format_args!("range={block_range:?}"),
                );
            }
            Ok(PayloadWindowTaskSchedule::Busy) => {
                crate::diagnostics::payload_pipeline::trace_payload_state(
                    "schedule.busy",
                    format_args!("range={block_range:?}"),
                );
            }
            Ok(PayloadWindowTaskSchedule::Idle) => {
                crate::diagnostics::payload_pipeline::trace_payload_state(
                    "schedule.idle",
                    format_args!("range={block_range:?}"),
                );
            }
            Err(error) => {
                crate::diagnostics::payload_pipeline::trace_payload(
                    "schedule.error",
                    format_args!("range={block_range:?} error={error}"),
                );
            }
        }
    }

    pub(crate) fn schedule_storage_payload_prefetch(
        &mut self,
        storage_request: PayloadStorageRequest,
        block_range: Range<usize>,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.ready_session().cloned() else {
            return;
        };
        match session.schedule_payload_prefetch_task(block_range.clone(), Instant::now()) {
            Ok(PayloadWindowTaskSchedule::Dispatch { token, request }) => {
                crate::diagnostics::payload_pipeline::trace_payload_state(
                    "prefetch-schedule.dispatch",
                    format_args!(
                        "generation={} range={:?} ids={}",
                        request.generation,
                        request.block_range,
                        request.block_ids.len()
                    ),
                );
                self.load_storage_payload_prefetch(storage_request, token, request, cx);
            }
            Ok(PayloadWindowTaskSchedule::WakeAfter(delay)) => {
                crate::diagnostics::payload_pipeline::trace_payload_state(
                    "prefetch-schedule.debounce",
                    format_args!(
                        "range={block_range:?} delay_ms={:.2}",
                        delay.as_secs_f64() * 1000.0
                    ),
                );
                self.schedule_storage_payload_prefetch_wake(delay, cx);
            }
            Ok(PayloadWindowTaskSchedule::WakeAlreadyScheduled) => {
                crate::diagnostics::payload_pipeline::trace_payload_state(
                    "prefetch-schedule.wake-pending",
                    format_args!("range={block_range:?}"),
                );
            }
            Ok(PayloadWindowTaskSchedule::Busy) => {
                crate::diagnostics::payload_pipeline::trace_payload_state(
                    "prefetch-schedule.busy",
                    format_args!("range={block_range:?}"),
                );
            }
            Ok(PayloadWindowTaskSchedule::Idle) => {
                crate::diagnostics::payload_pipeline::trace_payload_state(
                    "prefetch-schedule.resident",
                    format_args!("range={block_range:?}"),
                );
            }
            Err(error) => {
                crate::diagnostics::payload_pipeline::trace_payload(
                    "prefetch-schedule.error",
                    format_args!("range={block_range:?} error={error}"),
                );
            }
        }
    }

    pub(crate) fn schedule_selection_materialization(
        &mut self,
        command: GuiInputCommand,
        request: SelectionMaterializationRequest,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(session) = self.ready_session().cloned() else {
            return false;
        };
        let Some(storage_request) = session.payload_storage_request().ok().flatten() else {
            return false;
        };
        let task_key = request.document_id
            ^ request.structure_version.rotate_left(13)
            ^ request.payload_window_generation.rotate_left(29);
        let token = match session
            .begin_session_task(SessionTaskKind::SelectionMaterialization, task_key)
            .ok()
        {
            Some(SessionTaskAdmission::Started(token)) => token,
            Some(SessionTaskAdmission::Duplicate | SessionTaskAdmission::Busy) => return true,
            None => return false,
        };
        let load_request = request.clone();
        let load_task = cx.background_spawn(async move {
            run_payload_load(
                storage_request,
                &load_request.block_ids,
                token.timeout(),
                "selection payload materialization",
            )
            .map(|loaded| {
                (
                    prepare_payload_records(loaded.records),
                    loaded.missing_block_ids,
                )
            })
        });
        cx.spawn(async move |view, cx| {
            let result = load_task.await;
            let _ = view.update(cx, |view, cx| {
                let block_id = request.block_ids.first().copied();
                view.enqueue_main_thread_apply(
                    MainThreadWorkKind::WindowSwap,
                    request.payload_window_generation,
                    block_id,
                    WorkCost {
                        sync_ms: 0.2,
                        async_results: 1,
                        ..WorkCost::ZERO
                    },
                    move |view, cx| {
                        let Some(session) = view.ready_session().cloned() else {
                            return;
                        };
                        if !session.complete_session_task(token).unwrap_or(false) {
                            return;
                        }
                        let should_replay = match result {
                            Ok((records, missing_block_ids)) if missing_block_ids.is_empty() => {
                                session
                                    .apply_selection_materialization_result(
                                        &request,
                                        records,
                                        &missing_block_ids,
                                    )
                                    .is_ok_and(|snapshot| snapshot.replay_ready)
                            }
                            Ok((records, missing_block_ids)) => {
                                let _ = session.apply_selection_materialization_result(
                                    &request,
                                    records,
                                    &missing_block_ids,
                                );
                                false
                            }
                            Err(_) => false,
                        };
                        if should_replay {
                            view.apply_input_command(command, cx);
                        }
                        view.schedule_persistent_payload_cache_trim(cx);
                        cx.notify();
                    },
                    cx,
                );
            });
        })
        .detach();
        true
    }

    pub(crate) fn load_storage_payload_window(
        &mut self,
        storage_request: PayloadStorageRequest,
        token: SessionTaskToken,
        request: PayloadWindowLoadRequest,
        cx: &mut Context<Self>,
    ) {
        let failed_request = request.clone();
        let generation = request.generation;
        let block_range = request.block_range.clone();
        let requested_count = request.block_ids.len();
        crate::diagnostics::payload_pipeline::trace_payload(
            "visible-query.start",
            format_args!("generation={generation} range={block_range:?} ids={requested_count}"),
        );
        let load_task = cx.background_spawn(async move {
            let query_started = Instant::now();
            let loaded = run_payload_load(
                storage_request,
                &request.block_ids,
                token.timeout(),
                "storage viewport payload load",
            );
            let query_elapsed = query_started.elapsed();
            let prepare_started = Instant::now();
            let result = loaded.map(|loaded| {
                PayloadWindowLoadResult::prepare(request, loaded.records, loaded.missing_block_ids)
            });
            (query_elapsed, prepare_started.elapsed(), result)
        });
        cx.spawn(async move |view, cx| match load_task.await {
            (query_elapsed, prepare_elapsed, Ok(result)) => {
                let generation = result.request.generation;
                let block_range = result.request.block_range.clone();
                let record_count = result.records.len();
                let missing_count = result.missing_block_ids.len();
                crate::diagnostics::payload_pipeline::trace_payload(
                    "visible-query.complete",
                    format_args!(
                        "generation={generation} range={block_range:?} elapsed_ms={:.2} records={record_count} missing={missing_count}",
                        query_elapsed.as_secs_f64() * 1000.0
                    ),
                );
                crate::diagnostics::payload_pipeline::trace_payload(
                    "visible-prepare.complete",
                    format_args!(
                        "generation={generation} range={block_range:?} elapsed_ms={:.2} records={record_count}",
                        prepare_elapsed.as_secs_f64() * 1000.0
                    ),
                );
                let _ = view.update(cx, |view, cx| {
                    let apply_started = Instant::now();
                    let Some(session) = view.ready_session().cloned() else {
                        return;
                    };
                    // Visible completion is a viewport-liveness commit: validate
                    // ownership and move normalized records into the resident map.
                    // Text/table hydration, shaping and cache maintenance remain
                    // lazy or idle-budgeted, so this callback can notify at once.
                    let applied = session
                        .complete_payload_window_task_with_reschedule(token, Ok(result));
                    let pending_range = applied
                        .as_ref()
                        .ok()
                        .and_then(|completion| completion.as_ref())
                        .and_then(|(_, pending_range)| pending_range.clone());
                    crate::diagnostics::payload_pipeline::trace_payload(
                        "visible-commit.complete",
                        format_args!(
                            "generation={generation} range={block_range:?} apply_ms={:.2} result={applied:?}",
                            apply_started.elapsed().as_secs_f64() * 1000.0
                        ),
                    );
                    if let Some(pending_range) = pending_range
                        && let Ok(Some(storage_request)) = session.payload_storage_request()
                    {
                        view.schedule_storage_payload_window(storage_request, pending_range, cx);
                    }
                    view.schedule_persistent_payload_cache_trim(cx);
                    cx.notify();
                });
            }
            (query_elapsed, _prepare_elapsed, Err(message)) => {
                let generation = failed_request.generation;
                let block_range = failed_request.block_range.clone();
                crate::diagnostics::payload_pipeline::trace_payload(
                    "visible-query.error",
                    format_args!(
                        "generation={generation} range={block_range:?} elapsed_ms={:.2} error={message}",
                        query_elapsed.as_secs_f64() * 1000.0
                    ),
                );
                let _ = view.update(cx, |view, cx| {
                    let apply_started = Instant::now();
                    let Some(session) = view.ready_session().cloned() else {
                        return;
                    };
                    let applied = session.complete_payload_window_task_with_reschedule(
                        token,
                        Err((failed_request, message)),
                    );
                    let pending_range = applied
                        .as_ref()
                        .ok()
                        .and_then(|completion| completion.as_ref())
                        .and_then(|(_, pending_range)| pending_range.clone());
                    crate::diagnostics::payload_pipeline::trace_payload(
                        "visible-commit.complete",
                        format_args!(
                            "generation={generation} range={block_range:?} apply_ms={:.2} result={applied:?} error=true",
                            apply_started.elapsed().as_secs_f64() * 1000.0
                        ),
                    );
                    if let Some(pending_range) = pending_range
                        && let Ok(Some(storage_request)) = session.payload_storage_request()
                    {
                        view.schedule_storage_payload_window(storage_request, pending_range, cx);
                    }
                    cx.notify();
                });
            }
        })
        .detach();
    }

    pub(crate) fn load_storage_payload_prefetch(
        &mut self,
        storage_request: PayloadStorageRequest,
        token: SessionTaskToken,
        request: PayloadWindowLoadRequest,
        cx: &mut Context<Self>,
    ) {
        let failed_request = request.clone();
        let generation = request.generation;
        let block_range = request.block_range.clone();
        let requested_count = request.block_ids.len();
        crate::diagnostics::payload_pipeline::trace_payload_state(
            "prefetch-query.start",
            format_args!("generation={generation} range={block_range:?} ids={requested_count}"),
        );
        let load_task = cx.background_spawn(async move {
            let query_started = Instant::now();
            let loaded = run_payload_load(
                storage_request,
                &request.block_ids,
                token.timeout(),
                "storage payload prefetch",
            );
            let query_elapsed = query_started.elapsed();
            let prepare_started = Instant::now();
            let result = loaded.map(|loaded| {
                PayloadWindowLoadResult::prepare(request, loaded.records, loaded.missing_block_ids)
            });
            (query_elapsed, prepare_started.elapsed(), result)
        });
        cx.spawn(async move |view, cx| match load_task.await {
            (query_elapsed, prepare_elapsed, Ok(result)) => {
                let record_count = result.records.len();
                let missing_count = result.missing_block_ids.len();
                crate::diagnostics::payload_pipeline::trace_payload_state(
                    "prefetch-query.complete",
                    format_args!(
                        "generation={generation} range={block_range:?} elapsed_ms={:.2} records={record_count} missing={missing_count}",
                        query_elapsed.as_secs_f64() * 1000.0
                    ),
                );
                crate::diagnostics::payload_pipeline::trace_payload_state(
                    "prefetch-prepare.complete",
                    format_args!(
                        "generation={generation} range={block_range:?} elapsed_ms={:.2} records={record_count}",
                        prepare_elapsed.as_secs_f64() * 1000.0
                    ),
                );
                let _ = view.update(cx, |view, cx| {
                    view.enqueue_main_thread_apply(
                        MainThreadWorkKind::Prefetch,
                        generation,
                        None,
                        prefetch_payload_commit_cost(record_count, missing_count),
                        move |view, cx| {
                            let applied = view.ready_session().map(|session| {
                                session.complete_payload_prefetch_task(token, Ok(result))
                            });
                            crate::diagnostics::payload_pipeline::trace_payload_state(
                                "prefetch-commit.complete",
                                format_args!(
                                    "generation={generation} range={block_range:?} result={applied:?}"
                                ),
                            );
                            view.schedule_persistent_payload_cache_trim(cx);
                            cx.notify();
                        },
                        cx,
                    );
                });
            }
            (query_elapsed, _prepare_elapsed, Err(message)) => {
                crate::diagnostics::payload_pipeline::trace_payload(
                    "prefetch-query.error",
                    format_args!(
                        "generation={generation} range={block_range:?} elapsed_ms={:.2} error={message}",
                        query_elapsed.as_secs_f64() * 1000.0
                    ),
                );
                let _ = view.update(cx, |view, cx| {
                    view.enqueue_main_thread_apply(
                        MainThreadWorkKind::Prefetch,
                        generation,
                        None,
                        prefetch_payload_commit_cost(0, requested_count),
                        move |view, cx| {
                            let applied = view.ready_session().map(|session| {
                                session.complete_payload_prefetch_task(
                                    token,
                                    Err((failed_request, message)),
                                )
                            });
                            crate::diagnostics::payload_pipeline::trace_payload_state(
                                "prefetch-commit.complete",
                                format_args!(
                                    "generation={generation} range={block_range:?} result={applied:?} error=true"
                                ),
                            );
                            cx.notify();
                        },
                        cx,
                    );
                });
            }
        })
        .detach();
    }

    pub(crate) fn schedule_storage_payload_window_wake(
        &mut self,
        delay: std::time::Duration,
        cx: &mut Context<Self>,
    ) {
        let wake = cx.background_executor().timer(delay);
        cx.spawn(async move |view, cx| {
            wake.await;
            let _ = view.update(cx, |view, cx| {
                if let Some(session) = view.ready_session() {
                    let _ = session.wake_payload_window_task();
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn schedule_storage_payload_prefetch_wake(
        &mut self,
        delay: std::time::Duration,
        cx: &mut Context<Self>,
    ) {
        let wake = cx.background_executor().timer(delay);
        cx.spawn(async move |view, cx| {
            wake.await;
            let _ = view.update(cx, |view, cx| {
                if let Some(session) = view.ready_session() {
                    let _ = session.wake_payload_prefetch_task();
                }
                cx.notify();
            });
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefetch_payload_commit_cost_is_bounded_and_counts_one_async_result() {
        let empty = prefetch_payload_commit_cost(0, 0);
        let large = prefetch_payload_commit_cost(10_000, 500);

        assert_eq!(empty.sync_ms, 0.15);
        assert_eq!(empty.async_results, 1);
        assert_eq!(large.sync_ms, 2.0);
        assert_eq!(large.async_results, 1);
    }
}
