use crate::{AgentSessionId, TurnId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnStatus {
    Pending,
    Executing,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TurnTokenUsage {
    pub prompt_tokens: u64,
    pub cached_tokens: u64,
    pub completion_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTurn {
    pub id: TurnId,
    pub session_id: AgentSessionId,
    pub sequence: u64,
    pub status: TurnStatus,
    pub started_at_ms: Option<u64>,
    pub finished_at_ms: Option<u64>,
    pub tool_call_count: usize,
    pub token_usage: TurnTokenUsage,
}
impl AgentTurn {
    pub fn new(id: TurnId, sid: AgentSessionId, seq: u64) -> Self {
        Self {
            id,
            session_id: sid,
            sequence: seq,
            status: TurnStatus::Pending,
            started_at_ms: None,
            finished_at_ms: None,
            tool_call_count: 0,
            token_usage: TurnTokenUsage::default(),
        }
    }
    pub fn start(&mut self, now: u64) {
        self.status = TurnStatus::Executing;
        self.started_at_ms = Some(now);
    }
    pub fn complete(&mut self, now: u64) {
        self.status = TurnStatus::Completed;
        self.finished_at_ms = Some(now);
    }
    pub fn fail(&mut self, now: u64) {
        self.status = TurnStatus::Failed;
        self.finished_at_ms = Some(now);
    }
}
#[cfg(test)]
mod t {
    use super::*;
    #[test]
    fn turn_lifecycle() {
        let mut t = AgentTurn::new(uuid::Uuid::new_v4(), uuid::Uuid::new_v4(), 0);
        assert_eq!(t.status, TurnStatus::Pending);
        t.start(1000);
        assert_eq!(t.status, TurnStatus::Executing);
        t.complete(5000);
        assert_eq!(t.status, TurnStatus::Completed);
    }
}
