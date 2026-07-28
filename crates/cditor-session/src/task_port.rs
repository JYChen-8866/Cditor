use std::{collections::HashMap, ops::Range, time::Duration};
use web_time::Instant;

use cditor_editor_protocol::ProtocolError;

use crate::{EditorSessionHandle, SessionId};

pub use self::payload_window_port::PayloadWindowTaskSchedule;
use self::payload_window_port::PayloadWindowTaskState;

const STORAGE_TIMEOUT: Duration = Duration::from_secs(15);
const HISTORY_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SessionTaskKind {
    PayloadWindow,
    PayloadPrefetch,
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

#[derive(Debug, Clone, Copy)]
struct ActiveTask {
    key: u64,
    generation: u64,
}

#[derive(Debug, Default)]
pub(crate) struct SessionTaskCoordinator {
    next_generation: u64,
    active: HashMap<SessionTaskKind, ActiveTask>,
    payload: PayloadWindowTaskState,
    prefetch_last_dispatched_at: Option<Instant>,
    prefetch_wake_scheduled: bool,
    prefetch_pending_range: Option<Range<usize>>,
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

    fn is_active(&self, kind: SessionTaskKind) -> bool {
        self.active.contains_key(&kind)
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
        SessionTaskKind::PayloadPrefetch => (SessionTaskLane::Background, STORAGE_TIMEOUT),
        SessionTaskKind::HistoryHydration => (SessionTaskLane::Interactive, HISTORY_TIMEOUT),
        SessionTaskKind::AiStream => (SessionTaskLane::Interactive, Duration::from_secs(120)),
        SessionTaskKind::UndoSpill
        | SessionTaskKind::UndoCleanup
        | SessionTaskKind::PersistenceSave
        | SessionTaskKind::StorageFlush => (SessionTaskLane::Background, HISTORY_TIMEOUT),
    }
}

impl EditorSessionHandle {
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
    use cditor_core::{
        document::BlockIndexRecord,
        rich_text::{BlockPayloadRecord, RichBlockKind, kind_tag_for_rich_block_kind},
    };
    use cditor_runtime::DocumentRuntime;
    use cditor_runtime::document_runtime::{
        DocumentRuntimeColdStartData, DocumentRuntimeIndexSource,
    };

    use super::*;
    use crate::EditorSession;

    pub(super) fn cold_window_runtime() -> DocumentRuntime {
        let records = (1..=256)
            .map(|block_id| {
                BlockIndexRecord::new(
                    block_id,
                    None,
                    0,
                    kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph),
                    0,
                )
            })
            .collect();
        let initial_payloads = (1..=16)
            .map(|block_id| {
                BlockPayloadRecord::rich_text(block_id, RichBlockKind::Paragraph, "initial")
            })
            .collect();
        DocumentRuntime::from_cold_start_data(
            DocumentRuntimeColdStartData {
                document_id: 1,
                document_title: "payload scheduling race".to_owned(),
                structure_version: 1,
                records,
                block_attrs: Vec::new(),
                initial_payloads,
                initial_payload_window_end: 16,
                index_source: DocumentRuntimeIndexSource::Blocks,
                layout_cache_hits: 0,
            },
            720.0,
        )
        .unwrap()
        .0
    }

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
                SessionTaskKind::PayloadPrefetch,
                SessionTaskLane::Background,
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
}

#[cfg(test)]
#[path = "task_port_state_tests.rs"]
mod state_tests;

#[path = "task_port/payload_prefetch_port.rs"]
mod payload_prefetch_port;

#[path = "task_port/payload_window_port.rs"]
mod payload_window_port;
