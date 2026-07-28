use std::time::Duration;
use web_time::Instant;

use cditor_core::rich_text::{BlockPayloadRecord, RichBlockKind};
use cditor_runtime::content::payload_window::{
    PayloadWindowApplyDecision, PayloadWindowLoadRequest, PayloadWindowLoadResult,
};

use super::tests::cold_window_runtime;
use super::*;
use crate::EditorSession;

fn loaded_result(request: PayloadWindowLoadRequest, text: &'static str) -> PayloadWindowLoadResult {
    let records = request
        .block_ids
        .iter()
        .map(|block_id| BlockPayloadRecord::rich_text(*block_id, RichBlockKind::Paragraph, text))
        .collect();
    PayloadWindowLoadResult::prepare(request, records, Vec::new())
}

fn dispatch(
    handle: &EditorSessionHandle,
    range: Range<usize>,
    now: Instant,
) -> (SessionTaskToken, PayloadWindowLoadRequest) {
    let PayloadWindowTaskSchedule::Dispatch { token, request } = handle
        .schedule_payload_window_task(range.clone(), now)
        .unwrap()
    else {
        panic!("range {range:?} must dispatch")
    };
    (token, request)
}

#[test]
fn busy_ranges_are_coalesced_latest_wins_without_advancing_runtime_generation() {
    let handle = EditorSession::new(cold_window_runtime(), false).into_handle();
    let start = Instant::now();
    let (_token, request) = dispatch(&handle, 64..96, start);
    let generation = request.generation;
    let loading_count = request.block_ids.len();

    assert_eq!(
        handle
            .schedule_payload_window_task(128..160, start + Duration::from_millis(1))
            .unwrap(),
        PayloadWindowTaskSchedule::Busy
    );
    assert_eq!(
        handle.pending_payload_window_range().unwrap(),
        Some(128..160)
    );
    assert_eq!(
        handle
            .schedule_payload_window_task(192..224, start + Duration::from_millis(2))
            .unwrap(),
        PayloadWindowTaskSchedule::Busy
    );
    assert_eq!(
        handle.pending_payload_window_range().unwrap(),
        Some(192..224)
    );

    let session = handle.try_session_mut().unwrap();
    assert_eq!(session.runtime.payload_window_generation(), generation);
    assert_eq!(session.runtime.pending_payload_load_count(), loading_count);
}

#[test]
fn successful_completion_exposes_and_immediately_dispatches_latest_pending_range() {
    let handle = EditorSession::new(cold_window_runtime(), false).into_handle();
    let start = Instant::now();
    let (token, request) = dispatch(&handle, 64..96, start);
    let first_generation = request.generation;
    assert_eq!(
        handle
            .schedule_payload_window_task(128..160, start + Duration::from_millis(1))
            .unwrap(),
        PayloadWindowTaskSchedule::Busy
    );
    assert_eq!(
        handle
            .schedule_payload_window_task(192..224, start + Duration::from_millis(2))
            .unwrap(),
        PayloadWindowTaskSchedule::Busy
    );

    let completion = handle
        .complete_payload_window_task_with_reschedule(token, Ok(loaded_result(request, "first")))
        .unwrap();
    assert_eq!(
        completion,
        Some((PayloadWindowApplyDecision::Applied, Some(192..224)))
    );

    let PayloadWindowTaskSchedule::Dispatch {
        request: latest, ..
    } = handle
        .schedule_pending_payload_window_task(start + Duration::from_millis(3))
        .unwrap()
    else {
        panic!("latest pending range must dispatch immediately")
    };
    assert_eq!(latest.block_range, 192..224);
    assert!(latest.generation > first_generation);
    assert_eq!(handle.pending_payload_window_range().unwrap(), None);
}

#[test]
fn failed_completion_releases_lane_and_reschedules_latest_pending_range() {
    let handle = EditorSession::new(cold_window_runtime(), false).into_handle();
    let start = Instant::now();
    let (token, request) = dispatch(&handle, 64..96, start);
    assert_eq!(
        handle
            .schedule_payload_window_task(160..192, start + Duration::from_millis(1))
            .unwrap(),
        PayloadWindowTaskSchedule::Busy
    );

    let completion = handle
        .complete_payload_window_task_with_reschedule(
            token,
            Err((request, "sqlite read failed".to_owned())),
        )
        .unwrap();
    assert_eq!(
        completion,
        Some((PayloadWindowApplyDecision::Applied, Some(160..192)))
    );
    assert!(matches!(
        handle
            .schedule_pending_payload_window_task(start + Duration::from_millis(2))
            .unwrap(),
        PayloadWindowTaskSchedule::Dispatch { request, .. }
            if request.block_range == (160..192)
    ));
}

#[test]
fn reset_clears_active_loading_generation_and_latest_pending_range() {
    let handle = EditorSession::new(cold_window_runtime(), false).into_handle();
    let start = Instant::now();
    let (token, request) = dispatch(&handle, 64..96, start);
    assert_eq!(
        handle
            .schedule_payload_window_task(160..192, start + Duration::from_millis(1))
            .unwrap(),
        PayloadWindowTaskSchedule::Busy
    );

    handle.reset_payload_window_tasks().unwrap();

    assert_eq!(handle.pending_payload_window_range().unwrap(), None);
    assert_eq!(
        handle
            .try_session_mut()
            .unwrap()
            .runtime
            .pending_payload_load_count(),
        0
    );
    assert_eq!(
        handle
            .complete_payload_window_task(token, Ok(loaded_result(request, "late")))
            .unwrap(),
        None
    );
    assert_eq!(
        handle
            .schedule_pending_payload_window_task(start + Duration::from_millis(2))
            .unwrap(),
        PayloadWindowTaskSchedule::Idle
    );
}

#[test]
fn payload_completion_rejects_wrong_request_generation_without_releasing_lane() {
    let handle = EditorSession::new(cold_window_runtime(), false).into_handle();
    let start = Instant::now();
    let (token, request) = dispatch(&handle, 64..96, start);
    let mut wrong_request = request.clone();
    wrong_request.generation = wrong_request.generation.saturating_add(1);

    assert_eq!(
        handle
            .complete_payload_window_task(token, Ok(loaded_result(wrong_request, "wrong")))
            .unwrap(),
        None
    );
    assert_eq!(
        handle
            .schedule_payload_window_task(160..192, start + Duration::from_millis(1))
            .unwrap(),
        PayloadWindowTaskSchedule::Busy
    );
    assert_eq!(
        handle
            .complete_payload_window_task(token, Ok(loaded_result(request, "right")))
            .unwrap(),
        Some(PayloadWindowApplyDecision::Applied)
    );
}

#[test]
fn payload_completion_rejects_a_non_payload_token_without_consuming_either_task() {
    let handle = EditorSession::new(cold_window_runtime(), false).into_handle();
    let start = Instant::now();
    let (payload_token, request) = dispatch(&handle, 64..96, start);
    let SessionTaskAdmission::Started(history_token) = handle
        .begin_session_task(SessionTaskKind::HistoryHydration, 7)
        .unwrap()
    else {
        panic!("history task must start")
    };

    assert_eq!(
        handle
            .complete_payload_window_task(
                history_token,
                Ok(loaded_result(request.clone(), "wrong token")),
            )
            .unwrap(),
        None
    );
    assert!(handle.complete_session_task(history_token).unwrap());
    assert_eq!(
        handle
            .complete_payload_window_task(payload_token, Ok(loaded_result(request, "right")))
            .unwrap(),
        Some(PayloadWindowApplyDecision::Applied)
    );
}

#[test]
fn idle_planning_does_not_advance_last_dispatch_time() {
    let handle = EditorSession::new(cditor_runtime::DocumentRuntime::empty(), false).into_handle();
    assert_eq!(
        handle
            .schedule_payload_window_task(0..0, Instant::now())
            .unwrap(),
        PayloadWindowTaskSchedule::Idle
    );
    assert_eq!(
        handle
            .try_session_mut()
            .unwrap()
            .tasks
            .payload_last_dispatched_at(),
        None
    );
}
