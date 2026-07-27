use crate::{AgentSessionId, BlockId, DocumentId, TurnId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadBlockRecord {
    pub block_id: BlockId,
    pub content_version: u64,
    pub plain_text_preview: String,
    pub content_hash: [u8; 32],
    pub block_count_descendants: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCheckpoint {
    pub session_id: AgentSessionId,
    pub turn_id: TurnId,
    pub document_id: DocumentId,
    pub user_goal: String,
    pub user_constraints: Vec<String>,
    pub read_blocks: Vec<ReadBlockRecord>,
    pub document_revision: u64,
    pub structure_version: u64,
    pub committed_mutation_ids: Vec<uuid::Uuid>,
    pub pending_todos: Vec<String>,
    pub sequence: u64,
}

impl AgentCheckpoint {
    pub fn new(sid: AgentSessionId, tid: TurnId, did: DocumentId, goal: String) -> Self {
        Self {
            session_id: sid,
            turn_id: tid,
            document_id: did,
            user_goal: goal,
            user_constraints: vec![],
            read_blocks: vec![],
            document_revision: 0,
            structure_version: 0,
            committed_mutation_ids: vec![],
            pending_todos: vec![],
            sequence: 0,
        }
    }
    pub fn estimated_tokens(&self) -> u32 {
        256 + (self.read_blocks.len() as u32 * 96) + (self.pending_todos.len() as u32 * 32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn new_checkpoint_has_low_token_budget() {
        let c = AgentCheckpoint::new(
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            "test".into(),
        );
        assert!(c.estimated_tokens() < 1000);
        assert_eq!(c.sequence, 0);
    }
    #[test]
    fn estimated_tokens_grows_with_read_blocks() {
        let mut c = AgentCheckpoint::new(
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            "test".into(),
        );
        let before = c.estimated_tokens();
        c.read_blocks.push(ReadBlockRecord {
            block_id: uuid::Uuid::new_v4(),
            content_version: 1,
            plain_text_preview: "hello".into(),
            content_hash: [0; 32],
            block_count_descendants: None,
        });
        assert!(c.estimated_tokens() > before);
    }
}
