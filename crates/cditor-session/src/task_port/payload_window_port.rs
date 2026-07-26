use std::{ops::Range, time::Instant};

use cditor_editor_protocol::ProtocolError;
use cditor_runtime::content::payload_window::{
    PayloadWindowApplyDecision, PayloadWindowLoadRequest, PayloadWindowLoadResult,
};

use super::{
    EditorSessionHandle, SessionId, SessionTaskAdmission, SessionTaskCoordinator, SessionTaskKind,
    SessionTaskToken,
};

#[derive(Debug, Clone, PartialEq)]
pub enum PayloadWindowTaskSchedule {
    Dispatch {
        token: SessionTaskToken,
        request: PayloadWindowLoadRequest,
    },
    WakeAfter(std::time::Duration),
    WakeAlreadyScheduled,
    Busy,
    Idle,
}

#[derive(Debug, Default)]
pub(super) struct PayloadWindowTaskState {
    last_dispatched_at: Option<Instant>,
    pending_range: Option<Range<usize>>,
}

impl SessionTaskCoordinator {
    fn finish_payload(
        &mut self,
        session_id: SessionId,
        token: SessionTaskToken,
        request_generation: u64,
    ) -> bool {
        if token.session_id != session_id || token.kind != SessionTaskKind::PayloadWindow {
            return false;
        }
        let Some(active) = self.active.get(&SessionTaskKind::PayloadWindow).copied() else {
            return false;
        };
        if active.generation != token.generation || active.key != request_generation {
            return false;
        }
        self.active.remove(&SessionTaskKind::PayloadWindow);
        true
    }

    fn queue_latest_payload_range(&mut self, block_range: Range<usize>) {
        self.payload.pending_range = Some(block_range);
    }

    fn clear_pending_payload_range(&mut self) {
        self.payload.pending_range = None;
    }

    fn take_pending_payload_range(&mut self) -> Option<Range<usize>> {
        self.payload.pending_range.take()
    }

    fn pending_payload_range(&self) -> Option<Range<usize>> {
        self.payload.pending_range.clone()
    }

    fn mark_payload_dispatched(&mut self, now: Instant) {
        self.payload.last_dispatched_at = Some(now);
    }

    fn reset_payload(&mut self) -> Option<u64> {
        self.payload = PayloadWindowTaskState::default();
        self.active
            .remove(&SessionTaskKind::PayloadWindow)
            .map(|active| active.key)
    }

    #[cfg(test)]
    pub(super) fn payload_last_dispatched_at(&self) -> Option<Instant> {
        self.payload.last_dispatched_at
    }
}

impl EditorSessionHandle {
    pub fn schedule_payload_window_task(
        &self,
        block_range: Range<usize>,
        now: Instant,
    ) -> Result<PayloadWindowTaskSchedule, ProtocolError> {
        let mut session = self.try_session_mut()?;
        if session.tasks.is_active(SessionTaskKind::PayloadWindow) {
            session.tasks.queue_latest_payload_range(block_range);
            return Ok(PayloadWindowTaskSchedule::Busy);
        }

        // A render-time request is newer than anything retained while the lane
        // was busy. Visible payloads are never debounced: delaying a cold window
        // here directly extends the time that the viewport shows placeholders.
        session.tasks.clear_pending_payload_range();
        let Some(request) = session
            .runtime
            .plan_payload_window_load_if_needed(block_range)
        else {
            return Ok(PayloadWindowTaskSchedule::Idle);
        };
        let key = request.generation;
        let session_id = session.id;
        match session
            .tasks
            .begin(session_id, SessionTaskKind::PayloadWindow, key)
        {
            SessionTaskAdmission::Started(token) => {
                session.tasks.mark_payload_dispatched(now);
                Ok(PayloadWindowTaskSchedule::Dispatch { token, request })
            }
            SessionTaskAdmission::Duplicate | SessionTaskAdmission::Busy => {
                unreachable!("payload lane was checked while holding the session borrow")
            }
        }
    }

    /// Schedules the latest viewport range retained while a payload task was busy.
    ///
    /// Hosts that consume [`Self::complete_payload_window_task_with_reschedule`] can call
    /// this immediately after completion instead of waiting for another render.
    pub fn schedule_pending_payload_window_task(
        &self,
        now: Instant,
    ) -> Result<PayloadWindowTaskSchedule, ProtocolError> {
        let mut session = self.try_session_mut()?;
        if session.tasks.is_active(SessionTaskKind::PayloadWindow) {
            return Ok(PayloadWindowTaskSchedule::Busy);
        }
        let Some(block_range) = session.tasks.take_pending_payload_range() else {
            return Ok(PayloadWindowTaskSchedule::Idle);
        };
        let Some(request) = session
            .runtime
            .plan_payload_window_load_if_needed(block_range)
        else {
            return Ok(PayloadWindowTaskSchedule::Idle);
        };
        let key = request.generation;
        let session_id = session.id;
        let SessionTaskAdmission::Started(token) =
            session
                .tasks
                .begin(session_id, SessionTaskKind::PayloadWindow, key)
        else {
            unreachable!("payload lane was checked while holding the session borrow")
        };
        session.tasks.mark_payload_dispatched(now);
        Ok(PayloadWindowTaskSchedule::Dispatch { token, request })
    }

    pub fn wake_payload_window_task(&self) -> Result<(), ProtocolError> {
        // Kept for host compatibility. Visible payload scheduling no longer uses
        // a timer; prefetch throttling belongs to its independent lane.
        let _ = self.try_session_mut()?;
        Ok(())
    }

    pub fn pending_payload_window_range(&self) -> Result<Option<Range<usize>>, ProtocolError> {
        Ok(self.try_session_mut()?.tasks.pending_payload_range())
    }

    pub fn reset_payload_window_tasks(&self) -> Result<(), ProtocolError> {
        let mut session = self.try_session_mut()?;
        if let Some(generation) = session.tasks.reset_payload() {
            session.runtime.cancel_payload_window_load(generation);
        }
        if let Some(generation) = session.tasks.reset_prefetch() {
            session.runtime.cancel_payload_window_load(generation);
        }
        Ok(())
    }

    pub fn complete_payload_window_task(
        &self,
        token: SessionTaskToken,
        result: Result<PayloadWindowLoadResult, (PayloadWindowLoadRequest, String)>,
    ) -> Result<Option<PayloadWindowApplyDecision>, ProtocolError> {
        Ok(self
            .complete_payload_window_task_with_reschedule(token, result)?
            .map(|(decision, _pending_range)| decision))
    }

    /// Completes a payload load and exposes the latest range queued while it ran.
    ///
    /// `None` means the token/request pair was stale or mismatched and no runtime
    /// payload state was mutated. The nested range is a latest-wins reschedule
    /// hint; it remains queued until a host schedules it or a newer render request
    /// supersedes it.
    pub fn complete_payload_window_task_with_reschedule(
        &self,
        token: SessionTaskToken,
        result: Result<PayloadWindowLoadResult, (PayloadWindowLoadRequest, String)>,
    ) -> Result<Option<(PayloadWindowApplyDecision, Option<Range<usize>>)>, ProtocolError> {
        let mut session = self.try_session_mut()?;
        let session_id = session.id;
        let request_generation = match &result {
            Ok(result) => result.request.generation,
            Err((request, _)) => request.generation,
        };
        if !session
            .tasks
            .finish_payload(session_id, token, request_generation)
        {
            return Ok(None);
        }
        let decision = match result {
            Ok(result) => session.runtime.apply_payload_window_result(result),
            Err((request, message)) => session
                .runtime
                .apply_payload_window_load_error(request, message),
        };
        let pending_range = session.tasks.pending_payload_range();
        Ok(Some((decision, pending_range)))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use cditor_core::rich_text::{BlockPayloadRecord, RichBlockKind};
    use cditor_runtime::DocumentRuntime;

    use super::*;
    use crate::{EditorSession, task_port::tests::cold_window_runtime};

    #[test]
    fn payload_reset_cancels_an_in_flight_generation() {
        let handle = EditorSession::new(DocumentRuntime::empty(), false).into_handle();
        let SessionTaskAdmission::Started(token) = handle
            .begin_session_task(SessionTaskKind::PayloadWindow, 41)
            .unwrap()
        else {
            panic!("payload task must start")
        };

        handle.reset_payload_window_tasks().unwrap();

        assert!(!handle.complete_session_task(token).unwrap());
        assert!(matches!(
            handle
                .begin_session_task(SessionTaskKind::PayloadWindow, 42)
                .unwrap(),
            SessionTaskAdmission::Started(_)
        ));
    }

    #[test]
    fn payload_reset_releases_runtime_loading_markers_before_discarding_late_result() {
        let handle = EditorSession::new(cold_window_runtime(), false).into_handle();
        let start = Instant::now();
        let PayloadWindowTaskSchedule::Dispatch { token, request } =
            handle.schedule_payload_window_task(64..96, start).unwrap()
        else {
            panic!("remote window must dispatch")
        };
        let first_generation = request.generation;
        assert!(
            handle
                .try_session_mut()
                .unwrap()
                .runtime
                .pending_payload_load_count()
                > 0
        );

        handle.reset_payload_window_tasks().unwrap();

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
                .complete_payload_window_task(
                    token,
                    Ok(PayloadWindowLoadResult::prepare(
                        request,
                        Vec::new(),
                        Vec::new(),
                    )),
                )
                .unwrap(),
            None
        );
        let PayloadWindowTaskSchedule::Dispatch { request, .. } = handle
            .schedule_payload_window_task(64..96, start + Duration::from_millis(1))
            .unwrap()
        else {
            panic!("cancelled range must be dispatchable again")
        };
        assert!(request.generation > first_generation);
        assert!(!request.block_ids.is_empty());
    }

    #[test]
    fn visible_payload_requests_are_not_debounced() {
        let handle = EditorSession::new(cold_window_runtime(), false).into_handle();
        let start = Instant::now();
        let PayloadWindowTaskSchedule::Dispatch { token, request } =
            handle.schedule_payload_window_task(64..96, start).unwrap()
        else {
            panic!("first cold window must dispatch")
        };
        let records = request
            .block_ids
            .iter()
            .map(|block_id| {
                BlockPayloadRecord::rich_text(*block_id, RichBlockKind::Paragraph, "loaded")
            })
            .collect();
        handle
            .complete_payload_window_task(
                token,
                Ok(PayloadWindowLoadResult::prepare(
                    request,
                    records,
                    Vec::new(),
                )),
            )
            .unwrap();

        assert!(matches!(
            handle
                .schedule_payload_window_task(160..192, start + Duration::from_millis(1))
                .unwrap(),
            PayloadWindowTaskSchedule::Dispatch { .. }
        ));
    }

    #[test]
    fn in_flight_payload_task_does_not_create_an_undispatched_loading_generation() {
        let handle = EditorSession::new(cold_window_runtime(), false).into_handle();
        let start = Instant::now();
        let PayloadWindowTaskSchedule::Dispatch {
            token,
            request: first_request,
        } = handle.schedule_payload_window_task(64..96, start).unwrap()
        else {
            panic!("first remote window must dispatch")
        };
        let first_generation = first_request.generation;

        assert_eq!(
            handle
                .schedule_payload_window_task(160..192, start + Duration::from_millis(1))
                .unwrap(),
            PayloadWindowTaskSchedule::Busy
        );
        assert_eq!(
            handle
                .try_session_mut()
                .unwrap()
                .runtime
                .payload_window_generation(),
            first_generation,
            "a busy lane must not leave an undispatched loading generation"
        );

        let first_records = first_request
            .block_ids
            .iter()
            .map(|block_id| {
                BlockPayloadRecord::rich_text(*block_id, RichBlockKind::Paragraph, "first")
            })
            .collect();
        assert_eq!(
            handle
                .complete_payload_window_task(
                    token,
                    Ok(PayloadWindowLoadResult::prepare(
                        first_request,
                        first_records,
                        Vec::new(),
                    )),
                )
                .unwrap(),
            Some(PayloadWindowApplyDecision::Applied)
        );

        let PayloadWindowTaskSchedule::Dispatch {
            request: latest_request,
            ..
        } = handle
            .schedule_payload_window_task(160..192, start + Duration::from_millis(2))
            .unwrap()
        else {
            panic!("latest viewport must dispatch after the lane is released")
        };
        assert_eq!(latest_request.block_range, 160..192);
        assert!(latest_request.generation > first_generation);
    }
}
