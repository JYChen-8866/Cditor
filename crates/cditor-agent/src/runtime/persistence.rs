//! Persistence layer for agent sessions. Mirrors SiYuan's runtime persistence.
use std::fs;
use std::path::{Path, PathBuf};
use crate::protocol::messages::SessionEntry;
use crate::runtime::engine::AgentRuntimeTurn;
use crate::AgentSessionId;

#[derive(Debug, thiserror::Error)]
pub enum PersistenceError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("session not found: {0}")]
    NotFound(AgentSessionId),
    #[error("revision conflict: expected {expected}, found {found}")]
    Conflict { expected: i64, found: i64 },
}

pub trait PersistenceStore: Send + Sync {
    fn sessions_dir(&self) -> &Path;
    fn session_dir(&self, id: AgentSessionId) -> PathBuf {
        self.sessions_dir().join(id.to_string())
    }

    fn load_entries(&self, id: AgentSessionId) -> Result<Vec<SessionEntry>, PersistenceError> {
        let path = self.session_dir(id).join("session.json");
        if !path.exists() { return Ok(Vec::new()); }
        let data = fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&data)?)
    }

    fn save_entries(&self, id: AgentSessionId, entries: &[SessionEntry]) -> Result<(), PersistenceError> {
        let dir = self.session_dir(id);
        fs::create_dir_all(&dir)?;
        let path = dir.join("session.json");
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, serde_json::to_string_pretty(entries)?)?;
        fs::rename(&tmp, &path)?;
        Ok(())
    }

    fn load_runtime(&self, id: AgentSessionId) -> Result<Option<AgentRuntimeTurn>, PersistenceError> {
        let path = self.session_dir(id).join("runtime.json");
        if !path.exists() { return Ok(None); }
        let data = fs::read_to_string(&path)?;
        Ok(Some(serde_json::from_str(&data)?))
    }

    fn save_runtime(&self, id: AgentSessionId, turn: &AgentRuntimeTurn) -> Result<(), PersistenceError> {
        let dir = self.session_dir(id);
        fs::create_dir_all(&dir)?;
        let path = dir.join("runtime.json");
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, serde_json::to_string_pretty(turn)?)?;
        fs::rename(&tmp, &path)?;
        Ok(())
    }

    fn delete_session(&self, id: AgentSessionId) -> Result<(), PersistenceError> {
        let dir = self.session_dir(id);
        if dir.exists() { fs::remove_dir_all(&dir)?; }
        Ok(())
    }

    fn list_sessions(&self) -> Result<Vec<AgentSessionId>, PersistenceError> {
        let dir = self.sessions_dir();
        if !dir.exists() { return Ok(Vec::new()); }
        let mut ids = Vec::new();
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                if let Ok(id) = uuid::Uuid::parse_str(&entry.file_name().to_string_lossy()) {
                    ids.push(id);
                }
            }
        }
        Ok(ids)
    }
}

pub struct FsPersistenceStore { sessions_dir: PathBuf }
impl FsPersistenceStore {
    pub fn new(root: PathBuf) -> Self {
        Self { sessions_dir: root.join("agent").join("sessions") }
    }
}
impl PersistenceStore for FsPersistenceStore {
    fn sessions_dir(&self) -> &Path { &self.sessions_dir }
}

pub fn save_runtime_atomic(store: &dyn PersistenceStore, id: AgentSessionId, turn: &AgentRuntimeTurn, expected_revision: i64) -> Result<(), PersistenceError> {
    if let Some(prev) = store.load_runtime(id)? {
        if prev.base_revision != expected_revision {
            return Err(PersistenceError::Conflict { expected: expected_revision, found: prev.base_revision });
        }
    }
    store.save_runtime(id, turn)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn temp_store() -> FsPersistenceStore {
        let dir = std::env::temp_dir().join(format!("cditor-agent-test-{}", uuid::Uuid::new_v4()));
        FsPersistenceStore::new(dir)
    }

    #[test] fn save_and_load_entries() {
        let store = temp_store();
        let sid = AgentSessionId::new_v4();
        let entries = vec![SessionEntry { id: "e1".into(), role: "user".into(), content: "hello".into(), tool_calls: vec![], references: vec![], editor_context: None, created_at_ms: 1000 }];
        store.save_entries(sid, &entries).unwrap();
        assert_eq!(store.load_entries(sid).unwrap().len(), 1);
        store.delete_session(sid).unwrap();
    }
    #[test] fn save_and_load_runtime() {
        let store = temp_store();
        let sid = AgentSessionId::new_v4();
        let turn = AgentRuntimeTurn { turn_id: uuid::Uuid::new_v4(), mode: "append".into(), user_entry_id: None, base_revision: 0, state: "running".into(), updated_at_ms: 1000, user_content: None, draft_content: None, token_breakdown: std::collections::HashMap::new(), prompt_tokens: 0, completion_tokens: 0, last_prompt_tokens: 0, cached_tokens: 0, context_limit: 131072 };
        store.save_runtime(sid, &turn).unwrap();
        assert_eq!(store.load_runtime(sid).unwrap().unwrap().state, "running");
        store.delete_session(sid).unwrap();
    }
    #[test] fn atomic_revision_check() {
        let store = temp_store();
        let sid = AgentSessionId::new_v4();
        let t1 = AgentRuntimeTurn { turn_id: uuid::Uuid::new_v4(), mode: "append".into(), user_entry_id: None, base_revision: 1, state: "running".into(), updated_at_ms: 1000, user_content: None, draft_content: None, token_breakdown: std::collections::HashMap::new(), prompt_tokens: 0, completion_tokens: 0, last_prompt_tokens: 0, cached_tokens: 0, context_limit: 131072 };
        store.save_runtime(sid, &t1).unwrap();
        assert!(save_runtime_atomic(&store, sid, &t1, 999).is_err());
        store.delete_session(sid).unwrap();
    }
    #[test] fn list_sessions() {
        let store = temp_store();
        let sid = AgentSessionId::new_v4();
        let entries = vec![SessionEntry { id: "e1".into(), role: "user".into(), content: "test".into(), tool_calls: vec![], references: vec![], editor_context: None, created_at_ms: 1000 }];
        store.save_entries(sid, &entries).unwrap();
        let list = store.list_sessions().unwrap();
        assert!(list.contains(&sid));
        store.delete_session(sid).unwrap();
    }
}

// ── Runtime turn lifecycle (Section 4) ───────────────────────────

/// Begin a runtime turn with file lock + revision check.
/// Returns Ok(()) if the turn can proceed, Err if conflict or locked.
pub fn begin_runtime_turn(
    store: &dyn PersistenceStore,
    id: AgentSessionId,
    turn: &AgentRuntimeTurn,
) -> Result<(), PersistenceError> {
    // Check for uncommitted turn
    if let Some(existing) = store.load_runtime(id)? {
        if !is_turn_terminal(&existing) {
            return Err(PersistenceError::Conflict {
                expected: 0,
                found: existing.base_revision,
            });
        }
    }
    store.save_runtime(id, turn)
}

/// Finalize a turn: mark completed and write to disk.
pub fn save_runtime_turn(
    store: &dyn PersistenceStore,
    id: AgentSessionId,
    turn: &AgentRuntimeTurn,
) -> Result<(), PersistenceError> {
    store.save_runtime(id, turn)
}

/// Check if a session has an uncommitted turn.
pub fn has_uncommitted_turn(store: &dyn PersistenceStore, id: AgentSessionId) -> bool {
    store
        .load_runtime(id)
        .ok()
        .flatten()
        .map(|t| !is_turn_terminal(&t))
        .unwrap_or(false)
}

/// Get the turn ID of a recoverable (interrupted) turn.
pub fn recoverable_turn_id(
    store: &dyn PersistenceStore,
    id: AgentSessionId,
) -> Option<String> {
    store.load_runtime(id).ok().flatten().and_then(|t| {
        if t.state == "interrupted" {
            Some(t.turn_id.to_string())
        } else {
            None
        }
    })
}

/// Mark a runtime turn as committed (terminal state).
pub fn mark_runtime_committed(
    store: &dyn PersistenceStore,
    id: AgentSessionId,
    turn_id: uuid::Uuid,
) -> Result<(), PersistenceError> {
    if let Some(mut turn) = store.load_runtime(id)? {
        if turn.turn_id == turn_id {
            turn.state = "finished".into();
            store.save_runtime(id, &turn)?;
        }
    }
    Ok(())
}

/// Apply runtime turn data to a session (merge entries).
pub fn apply_runtime_to_session(
    store: &dyn PersistenceStore,
    id: AgentSessionId,
    entries: &[SessionEntry],
) -> Result<(), PersistenceError> {
    let mut existing = store.load_entries(id)?;
    for e in entries {
        if !existing.iter().any(|ex| ex.id == e.id) {
            existing.push(e.clone());
        }
    }
    store.save_entries(id, &existing)
}

/// Finalize an orphaned turn: set it to "interrupted".
pub fn finalize_orphaned_turn(
    store: &dyn PersistenceStore,
    id: AgentSessionId,
) -> Result<(), PersistenceError> {
    if let Some(mut turn) = store.load_runtime(id)? {
        if !is_turn_terminal(&turn) {
            turn.state = "interrupted".into();
            store.save_runtime(id, &turn)?;
        }
    }
    Ok(())
}

fn is_turn_terminal(turn: &AgentRuntimeTurn) -> bool {
    turn.state == "finished" || turn.state == "interrupted"
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;

    fn test_store() -> FsPersistenceStore {
        let dir = std::env::temp_dir().join(format!("cditor-agent-lifecycle-{}", uuid::Uuid::new_v4()));
        FsPersistenceStore::new(dir)
    }

    #[test]
    fn begin_and_finalize_turn() {
        let store = test_store();
        let sid = AgentSessionId::new_v4();
        let turn = AgentRuntimeTurn {
            turn_id: uuid::Uuid::new_v4(), mode: "append".into(), user_entry_id: None,
            base_revision: 0, state: "running".into(), updated_at_ms: 1000,
            user_content: None, draft_content: None, token_breakdown: Default::default(),
            prompt_tokens: 0, completion_tokens: 0, last_prompt_tokens: 0,
            cached_tokens: 0, context_limit: 131072,
        };
        begin_runtime_turn(&store, sid, &turn).unwrap();
        assert!(has_uncommitted_turn(&store, sid));
        
        finalize_orphaned_turn(&store, sid).unwrap();
        assert!(!has_uncommitted_turn(&store, sid));
        store.delete_session(sid).unwrap();
    }

    #[test]
    fn recoverable_turn_id_found() {
        let store = test_store();
        let sid = AgentSessionId::new_v4();
        let tid = uuid::Uuid::new_v4();
        let turn = AgentRuntimeTurn {
            turn_id: tid, mode: "append".into(), user_entry_id: None,
            base_revision: 0, state: "interrupted".into(), updated_at_ms: 1000,
            user_content: None, draft_content: None, token_breakdown: Default::default(),
            prompt_tokens: 0, completion_tokens: 0, last_prompt_tokens: 0,
            cached_tokens: 0, context_limit: 131072,
        };
        store.save_runtime(sid, &turn).unwrap();
        assert_eq!(recoverable_turn_id(&store, sid), Some(tid.to_string()));
        store.delete_session(sid).unwrap();
    }
}
