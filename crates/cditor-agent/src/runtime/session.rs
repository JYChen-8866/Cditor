use super::turn::AgentTurn;
use crate::protocol::checkpoint::AgentCheckpoint;
use crate::tools::mutation::PreparedMutationId;
use crate::{AgentErrorCode, AgentSessionId, TurnId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentSessionState {
    Idle,
    Active(TurnId),
    AwaitingConfirmation(PreparedMutationId),
    Cancelling(TurnId),
    Cancelled,
    Failed(AgentErrorCode),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSessionConfig {
    pub model_context_limit_tokens: u64,
    pub turn_deadline_secs: i64,
    pub max_turns: usize,
}
impl Default for AgentSessionConfig {
    fn default() -> Self {
        Self {
            model_context_limit_tokens: 131072,
            turn_deadline_secs: 300,
            max_turns: 20,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSession {
    pub id: AgentSessionId,
    pub config: AgentSessionConfig,
    pub state: AgentSessionState,
    pub turns: Vec<AgentTurn>,
    pub checkpoints: Vec<AgentCheckpoint>,
    pub created_at_ms: u64,
}
impl AgentSession {
    pub fn new(id: AgentSessionId, config: AgentSessionConfig, now: u64) -> Self {
        Self {
            id,
            config,
            state: AgentSessionState::Idle,
            turns: Vec::new(),
            checkpoints: Vec::new(),
            created_at_ms: now,
        }
    }
    pub fn next_sequence(&self) -> u64 {
        self.turns.len() as u64
    }
    pub fn is_active(&self) -> bool {
        matches!(
            self.state,
            AgentSessionState::Active(_) | AgentSessionState::AwaitingConfirmation(_)
        )
    }
    pub fn transition_to_active(&mut self, tid: TurnId) {
        self.state = AgentSessionState::Active(tid);
    }
    pub fn transition_to_awaiting(&mut self, pid: PreparedMutationId) {
        self.state = AgentSessionState::AwaitingConfirmation(pid);
    }
    pub fn transition_to_idle(&mut self) {
        self.state = AgentSessionState::Idle;
    }
    pub fn record_turn(&mut self, t: AgentTurn) {
        self.turns.push(t);
    }
    pub fn record_checkpoint(&mut self, c: AgentCheckpoint) {
        self.checkpoints.push(c);
    }
}
#[cfg(test)]
mod t {
    use super::*;
    #[test]
    fn session_lifecycle() {
        let mut s = AgentSession::new(uuid::Uuid::new_v4(), AgentSessionConfig::default(), 1000);
        assert_eq!(s.state, AgentSessionState::Idle);
        s.transition_to_active(uuid::Uuid::new_v4());
        assert!(s.is_active());
    }
}
