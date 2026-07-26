use super::*;
use cditor_runtime::content::payload_window::{
    PayloadWindowApplyDecision, PayloadWindowLoadRequest, PayloadWindowLoadResult,
};

const PAYLOAD_PREFETCH_DEBOUNCE: Duration = Duration::from_millis(75);

impl SessionTaskCoordinator {
    fn prefetch_delay(&mut self, now: Instant) -> Option<Duration> {
        if self.prefetch_wake_scheduled {
            return Some(Duration::ZERO);
        }
        let last = self.prefetch_last_dispatched_at?;
        let elapsed = now.saturating_duration_since(last);
        if elapsed >= PAYLOAD_PREFETCH_DEBOUNCE {
            return None;
        }
        self.prefetch_wake_scheduled = true;
        Some(PAYLOAD_PREFETCH_DEBOUNCE - elapsed)
    }

    fn mark_prefetch_dispatched(&mut self, now: Instant) {
        self.prefetch_last_dispatched_at = Some(now);
        self.prefetch_wake_scheduled = false;
        self.prefetch_pending_range = None;
    }

    fn finish_prefetch(
        &mut self,
        session_id: SessionId,
        token: SessionTaskToken,
        request_generation: u64,
    ) -> bool {
        if token.session_id != session_id || token.kind != SessionTaskKind::PayloadPrefetch {
            return false;
        }
        let Some(active) = self.active.get(&SessionTaskKind::PayloadPrefetch).copied() else {
            return false;
        };
        if active.generation != token.generation || active.key != request_generation {
            return false;
        }
        self.active.remove(&SessionTaskKind::PayloadPrefetch);
        true
    }

    pub(super) fn reset_prefetch(&mut self) -> Option<u64> {
        self.prefetch_last_dispatched_at = None;
        self.prefetch_wake_scheduled = false;
        self.prefetch_pending_range = None;
        self.active
            .remove(&SessionTaskKind::PayloadPrefetch)
            .map(|active| active.key)
    }
}

impl EditorSessionHandle {
    pub fn schedule_payload_prefetch_task(
        &self,
        block_range: Range<usize>,
        now: Instant,
    ) -> Result<PayloadWindowTaskSchedule, ProtocolError> {
        let mut session = self.try_session_mut()?;
        if session.tasks.is_active(SessionTaskKind::PayloadPrefetch) {
            session.tasks.prefetch_pending_range = Some(block_range);
            return Ok(PayloadWindowTaskSchedule::Busy);
        }
        if let Some(delay) = session.tasks.prefetch_delay(now) {
            session.tasks.prefetch_pending_range = Some(block_range);
            return Ok(if delay.is_zero() {
                PayloadWindowTaskSchedule::WakeAlreadyScheduled
            } else {
                PayloadWindowTaskSchedule::WakeAfter(delay)
            });
        }

        let Some(request) = session
            .runtime
            .plan_payload_prefetch_load_if_needed(block_range)
        else {
            session.tasks.prefetch_pending_range = None;
            return Ok(PayloadWindowTaskSchedule::Idle);
        };
        let key = request.generation;
        let session_id = session.id;
        let SessionTaskAdmission::Started(token) =
            session
                .tasks
                .begin(session_id, SessionTaskKind::PayloadPrefetch, key)
        else {
            unreachable!("prefetch lane was checked while holding the session borrow")
        };
        session.tasks.mark_prefetch_dispatched(now);
        Ok(PayloadWindowTaskSchedule::Dispatch { token, request })
    }

    pub fn wake_payload_prefetch_task(&self) -> Result<(), ProtocolError> {
        self.try_session_mut()?.tasks.prefetch_wake_scheduled = false;
        Ok(())
    }

    pub fn pending_payload_prefetch_range(&self) -> Result<Option<Range<usize>>, ProtocolError> {
        Ok(self.try_session_mut()?.tasks.prefetch_pending_range.clone())
    }

    pub fn complete_payload_prefetch_task(
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
            .finish_prefetch(session_id, token, request_generation)
        {
            return Ok(None);
        }
        let decision = match result {
            Ok(result) => session.runtime.apply_payload_prefetch_result(result),
            Err((request, _)) => session.runtime.apply_payload_prefetch_load_error(request),
        };
        Ok(Some((
            decision,
            session.tasks.prefetch_pending_range.clone(),
        )))
    }
}

#[cfg(test)]
mod tests {
    use cditor_core::rich_text::{BlockPayloadRecord, RichBlockKind};

    use super::*;
    use crate::{EditorSession, task_port::tests::cold_window_runtime};

    #[test]
    fn prefetch_is_debounced_without_delaying_the_visible_lane() {
        let handle = EditorSession::new(cold_window_runtime(), false).into_handle();
        let start = Instant::now();
        let PayloadWindowTaskSchedule::Dispatch {
            token: prefetch_token,
            request: prefetch_request,
        } = handle
            .schedule_payload_prefetch_task(64..96, start)
            .unwrap()
        else {
            panic!("first prefetch must dispatch")
        };

        let PayloadWindowTaskSchedule::Dispatch { request, .. } = handle
            .schedule_payload_window_task(160..192, start + Duration::from_millis(1))
            .unwrap()
        else {
            panic!("visible work must dispatch while prefetch is active")
        };
        assert_eq!(request.block_range, 160..192);

        handle
            .complete_payload_prefetch_task(
                prefetch_token,
                Ok(PayloadWindowLoadResult::prepare(
                    prefetch_request.clone(),
                    prefetch_request
                        .block_ids
                        .iter()
                        .map(|block_id| {
                            BlockPayloadRecord::rich_text(
                                *block_id,
                                RichBlockKind::Paragraph,
                                "prefetched",
                            )
                        })
                        .collect(),
                    Vec::new(),
                )),
            )
            .unwrap();
        assert!(matches!(
            handle
                .schedule_payload_prefetch_task(200..224, start + Duration::from_millis(2))
                .unwrap(),
            PayloadWindowTaskSchedule::WakeAfter(_)
        ));
    }

    #[test]
    fn prefetch_busy_state_coalesces_to_the_latest_range() {
        let handle = EditorSession::new(cold_window_runtime(), false).into_handle();
        let start = Instant::now();
        assert!(matches!(
            handle
                .schedule_payload_prefetch_task(64..96, start)
                .unwrap(),
            PayloadWindowTaskSchedule::Dispatch { .. }
        ));

        assert_eq!(
            handle
                .schedule_payload_prefetch_task(128..160, start + Duration::from_millis(1))
                .unwrap(),
            PayloadWindowTaskSchedule::Busy
        );
        assert_eq!(
            handle
                .schedule_payload_prefetch_task(192..224, start + Duration::from_millis(2))
                .unwrap(),
            PayloadWindowTaskSchedule::Busy
        );
        assert_eq!(
            handle.pending_payload_prefetch_range().unwrap(),
            Some(192..224)
        );
    }

    #[test]
    fn prefetch_missing_releases_ownership_without_spending_visible_retries() {
        let handle = EditorSession::new(cold_window_runtime(), false).into_handle();
        let start = Instant::now();
        let PayloadWindowTaskSchedule::Dispatch {
            token,
            request: prefetch_request,
        } = handle
            .schedule_payload_prefetch_task(64..66, start)
            .unwrap()
        else {
            panic!("cold prefetch range must dispatch")
        };
        let loaded_id = prefetch_request.block_ids[0];
        let missing_id = prefetch_request.block_ids[1];

        let completed = handle
            .complete_payload_prefetch_task(
                token,
                Ok(PayloadWindowLoadResult::prepare(
                    prefetch_request,
                    vec![BlockPayloadRecord::rich_text(
                        loaded_id,
                        RichBlockKind::Paragraph,
                        "prefetched",
                    )],
                    vec![missing_id],
                )),
            )
            .unwrap();
        assert!(matches!(
            completed,
            Some((PayloadWindowApplyDecision::Applied, _))
        ));

        for attempt in 1..=3 {
            let PayloadWindowTaskSchedule::Dispatch { token, request } = handle
                .schedule_payload_window_task(64..66, start + Duration::from_millis(100 + attempt))
                .unwrap()
            else {
                panic!("visible retry {attempt} must still dispatch")
            };
            assert_eq!(request.block_ids, vec![missing_id]);
            handle
                .complete_payload_window_task(
                    token,
                    Err((request, format!("visible attempt {attempt}"))),
                )
                .unwrap();
        }

        assert_eq!(
            handle
                .schedule_payload_window_task(64..66, start + Duration::from_millis(200))
                .unwrap(),
            PayloadWindowTaskSchedule::Idle
        );
    }
}
