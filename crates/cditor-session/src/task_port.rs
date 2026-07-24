use std::{
    collections::HashMap,
    ops::Range,
    time::{Duration, Instant},
};

use cditor_editor_protocol::ProtocolError;
use cditor_runtime::content::payload_window::{
    PayloadWindowApplyDecision, PayloadWindowLoadRequest, PayloadWindowLoadResult,
};

use crate::{EditorSessionHandle, SessionId};

const PAYLOAD_DEBOUNCE: Duration = Duration::from_millis(75);
const STORAGE_TIMEOUT: Duration = Duration::from_secs(15);
const HISTORY_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SessionTaskKind {
    PayloadWindow,
    SelectionMaterialization,
    HistoryHydration,
    UndoSpill,
    UndoCleanup,
    PersistenceSave,
    StorageFlush,
    AiStream,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionTaskLane {
    Interactive,
    Visible,
    Background,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionTaskToken {
    session_id: SessionId,
    kind: SessionTaskKind,
    generation: u64,
    timeout: Duration,
    lane: SessionTaskLane,
}

impl SessionTaskToken {
    pub const fn timeout(self) -> Duration {
        self.timeout
    }

    pub const fn lane(self) -> SessionTaskLane {
        self.lane
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionTaskAdmission {
    Started(SessionTaskToken),
    Duplicate,
    Busy,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PayloadWindowTaskSchedule {
    Dispatch {
        token: SessionTaskToken,
        request: PayloadWindowLoadRequest,
    },
    WakeAfter(Duration),
    WakeAlreadyScheduled,
    Idle,
}

#[derive(Debug, Clone, Copy)]
struct ActiveTask {
    key: u64,
    generation: u64,
}

#[derive(Debug, Default)]
pub(crate) struct SessionTaskCoordinator {
    next_generation: u64,
    active: HashMap<SessionTaskKind, ActiveTask>,
    payload_last_dispatched_at: Option<Instant>,
    payload_wake_scheduled: bool,
}

impl SessionTaskCoordinator {
    fn begin(
        &mut self,
        session_id: SessionId,
        kind: SessionTaskKind,
        key: u64,
    ) -> SessionTaskAdmission {
        if let Some(active) = self.active.get(&kind) {
            return if active.key == key {
                SessionTaskAdmission::Duplicate
            } else {
                SessionTaskAdmission::Busy
            };
        }
        self.next_generation = self.next_generation.wrapping_add(1).max(1);
        let generation = self.next_generation;
        self.active.insert(kind, ActiveTask { key, generation });
        let (lane, timeout) = task_policy(kind);
        SessionTaskAdmission::Started(SessionTaskToken {
            session_id,
            kind,
            generation,
            timeout,
            lane,
        })
    }

    fn finish(&mut self, session_id: SessionId, token: SessionTaskToken) -> bool {
        if token.session_id != session_id {
            return false;
        }
        let current = self.active.get(&token.kind).copied();
        if current.is_none_or(|active| active.generation != token.generation) {
            return false;
        }
        self.active.remove(&token.kind);
        true
    }

    fn payload_schedule(&mut self, now: Instant) -> Result<(), Duration> {
        let Some(last) = self.payload_last_dispatched_at else {
            self.payload_last_dispatched_at = Some(now);
            return Ok(());
        };
        let elapsed = now.saturating_duration_since(last);
        if elapsed >= PAYLOAD_DEBOUNCE {
            self.payload_last_dispatched_at = Some(now);
            self.payload_wake_scheduled = false;
            return Ok(());
        }
        if self.payload_wake_scheduled {
            return Err(Duration::ZERO);
        }
        self.payload_wake_scheduled = true;
        Err(PAYLOAD_DEBOUNCE - elapsed)
    }

    fn wake_payload(&mut self) {
        self.payload_wake_scheduled = false;
    }

    fn reset_payload(&mut self) {
        self.payload_last_dispatched_at = None;
        self.payload_wake_scheduled = false;
        self.active.remove(&SessionTaskKind::PayloadWindow);
    }

    pub(crate) fn cancel_kind(&mut self, kind: SessionTaskKind) {
        self.active.remove(&kind);
    }
}

fn task_policy(kind: SessionTaskKind) -> (SessionTaskLane, Duration) {
    match kind {
        SessionTaskKind::PayloadWindow | SessionTaskKind::SelectionMaterialization => {
            (SessionTaskLane::Visible, STORAGE_TIMEOUT)
        }
        SessionTaskKind::HistoryHydration => (SessionTaskLane::Interactive, HISTORY_TIMEOUT),
        SessionTaskKind::AiStream => (SessionTaskLane::Interactive, Duration::from_secs(120)),
        SessionTaskKind::UndoSpill
        | SessionTaskKind::UndoCleanup
        | SessionTaskKind::PersistenceSave
        | SessionTaskKind::StorageFlush => (SessionTaskLane::Background, HISTORY_TIMEOUT),
    }
}

impl EditorSessionHandle {
    pub fn schedule_payload_window_task(
        &self,
        block_range: Range<usize>,
        now: Instant,
    ) -> Result<PayloadWindowTaskSchedule, ProtocolError> {
        let mut session = self.try_session_mut()?;
        match session.tasks.payload_schedule(now) {
            Err(delay) if delay.is_zero() => {
                return Ok(PayloadWindowTaskSchedule::WakeAlreadyScheduled);
            }
            Err(delay) => return Ok(PayloadWindowTaskSchedule::WakeAfter(delay)),
            Ok(()) => {}
        }
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
                Ok(PayloadWindowTaskSchedule::Dispatch { token, request })
            }
            SessionTaskAdmission::Duplicate | SessionTaskAdmission::Busy => {
                Ok(PayloadWindowTaskSchedule::Idle)
            }
        }
    }

    pub fn wake_payload_window_task(&self) -> Result<(), ProtocolError> {
        self.try_session_mut()?.tasks.wake_payload();
        Ok(())
    }

    pub fn reset_payload_window_tasks(&self) -> Result<(), ProtocolError> {
        self.try_session_mut()?.tasks.reset_payload();
        Ok(())
    }

    pub fn complete_payload_window_task(
        &self,
        token: SessionTaskToken,
        result: Result<PayloadWindowLoadResult, (PayloadWindowLoadRequest, String)>,
    ) -> Result<Option<PayloadWindowApplyDecision>, ProtocolError> {
        let mut session = self.try_session_mut()?;
        let session_id = session.id;
        if !session.tasks.finish(session_id, token) {
            return Ok(None);
        }
        Ok(Some(match result {
            Ok(result) => session.runtime.apply_payload_window_result(result),
            Err((request, message)) => session
                .runtime
                .apply_payload_window_load_error(request, message),
        }))
    }

    pub fn begin_session_task(
        &self,
        kind: SessionTaskKind,
        key: u64,
    ) -> Result<SessionTaskAdmission, ProtocolError> {
        let mut session = self.try_session_mut()?;
        let session_id = session.id;
        Ok(session.tasks.begin(session_id, kind, key))
    }

    pub fn complete_session_task(&self, token: SessionTaskToken) -> Result<bool, ProtocolError> {
        let mut session = self.try_session_mut()?;
        let session_id = session.id;
        Ok(session.tasks.finish(session_id, token))
    }

    pub fn replace_session_task(
        &self,
        kind: SessionTaskKind,
        key: u64,
    ) -> Result<SessionTaskToken, ProtocolError> {
        let mut session = self.try_session_mut()?;
        session.tasks.cancel_kind(kind);
        let session_id = session.id;
        let SessionTaskAdmission::Started(token) = session.tasks.begin(session_id, kind, key)
        else {
            unreachable!("cancelled task slot must accept replacement")
        };
        Ok(token)
    }
}

#[cfg(test)]
mod tests {
    use cditor_runtime::DocumentRuntime;

    use super::*;
    use crate::EditorSession;

    #[test]
    fn task_slots_dedupe_reject_conflicts_and_discard_stale_completion() {
        let handle = EditorSession::new(DocumentRuntime::empty(), false).into_handle();
        let SessionTaskAdmission::Started(first) = handle
            .begin_session_task(SessionTaskKind::HistoryHydration, 7)
            .unwrap()
        else {
            panic!("first task must start")
        };
        assert_eq!(first.lane(), SessionTaskLane::Interactive);
        assert_eq!(
            handle
                .begin_session_task(SessionTaskKind::HistoryHydration, 7)
                .unwrap(),
            SessionTaskAdmission::Duplicate
        );
        assert_eq!(
            handle
                .begin_session_task(SessionTaskKind::HistoryHydration, 8)
                .unwrap(),
            SessionTaskAdmission::Busy
        );
        assert!(handle.complete_session_task(first).unwrap());
        assert!(!handle.complete_session_task(first).unwrap());

        let SessionTaskAdmission::Started(session_bound) = handle
            .begin_session_task(SessionTaskKind::UndoCleanup, 1)
            .unwrap()
        else {
            panic!("cleanup task must start")
        };
        let other = EditorSession::new(DocumentRuntime::empty(), false).into_handle();
        assert!(!other.complete_session_task(session_bound).unwrap());
        assert!(handle.complete_session_task(session_bound).unwrap());
    }

    #[test]
    fn replacement_cancels_old_generation_and_policies_are_explicit() {
        let handle = EditorSession::new(DocumentRuntime::empty(), false).into_handle();
        let first = handle
            .replace_session_task(SessionTaskKind::AiStream, 1)
            .unwrap();
        let replacement = handle
            .replace_session_task(SessionTaskKind::AiStream, 2)
            .unwrap();

        assert_eq!(replacement.lane(), SessionTaskLane::Interactive);
        assert_eq!(replacement.timeout(), Duration::from_secs(120));
        assert!(!handle.complete_session_task(first).unwrap());
        assert!(handle.complete_session_task(replacement).unwrap());

        let SessionTaskAdmission::Started(background) = handle
            .begin_session_task(SessionTaskKind::PersistenceSave, 1)
            .unwrap()
        else {
            panic!("save task must start")
        };
        assert_eq!(background.lane(), SessionTaskLane::Background);
        assert_eq!(background.timeout(), HISTORY_TIMEOUT);
    }

    #[test]
    fn every_background_operation_has_an_explicit_lane_and_timeout() {
        let expected = [
            (
                SessionTaskKind::PayloadWindow,
                SessionTaskLane::Visible,
                STORAGE_TIMEOUT,
            ),
            (
                SessionTaskKind::SelectionMaterialization,
                SessionTaskLane::Visible,
                STORAGE_TIMEOUT,
            ),
            (
                SessionTaskKind::HistoryHydration,
                SessionTaskLane::Interactive,
                HISTORY_TIMEOUT,
            ),
            (
                SessionTaskKind::UndoSpill,
                SessionTaskLane::Background,
                HISTORY_TIMEOUT,
            ),
            (
                SessionTaskKind::UndoCleanup,
                SessionTaskLane::Background,
                HISTORY_TIMEOUT,
            ),
            (
                SessionTaskKind::PersistenceSave,
                SessionTaskLane::Background,
                HISTORY_TIMEOUT,
            ),
            (
                SessionTaskKind::StorageFlush,
                SessionTaskLane::Background,
                HISTORY_TIMEOUT,
            ),
            (
                SessionTaskKind::AiStream,
                SessionTaskLane::Interactive,
                Duration::from_secs(120),
            ),
        ];

        for (kind, lane, timeout) in expected {
            assert_eq!(task_policy(kind), (lane, timeout), "policy for {kind:?}");
            assert!(!timeout.is_zero(), "timeout for {kind:?}");
        }
    }

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
    fn payload_debounce_is_owned_by_session() {
        let handle = EditorSession::new(DocumentRuntime::large_mixed_demo(), false).into_handle();
        let start = Instant::now();
        assert!(matches!(
            handle.schedule_payload_window_task(0..64, start).unwrap(),
            PayloadWindowTaskSchedule::Idle | PayloadWindowTaskSchedule::Dispatch { .. }
        ));
        assert!(matches!(
            handle
                .schedule_payload_window_task(64..128, start + Duration::from_millis(25))
                .unwrap(),
            PayloadWindowTaskSchedule::WakeAfter(_)
        ));
    }
}
