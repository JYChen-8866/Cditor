//! Persistent message model — mirrors SiYuan AgentMessage / SessionEntry.
use crate::{BlockId, DocumentId, JsonValue};
use serde::{Deserialize, Serialize};

/// A block or document reference passed by the user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentReference {
    pub id: String,
    pub title: String,
    pub ref_type: String,
    pub url: Option<String>,
}

/// Editor context snapshot — only IDs, no content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentEditorContext {
    pub active_doc_id: Option<DocumentId>,
    pub active_doc_title: Option<String>,
    pub notebook_id: Option<String>,
    pub focused_block_id: Option<BlockId>,
    pub selected_block_ids: Vec<BlockId>,
    pub visible_block_ids: Vec<BlockId>,
}

/// A single tool call within a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentToolCallEntry {
    pub id: String,
    pub name: String,
    pub arguments: JsonValue,
    pub state: String,
    pub result: Option<String>,
}

/// A message in the agent conversation (checkpoint-compatible).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    pub role: String,
    pub content: String,
    pub tool_calls: Vec<AgentToolCallEntry>,
    pub entry_id: Option<String>,
    pub references: Vec<AgentReference>,
    pub editor_context: Option<AgentEditorContext>,
    pub thinking_content: Option<String>,
}

/// A user step within a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEntryStep {
    pub id: String,
    pub name: String,
    pub content: String,
    pub references: Vec<AgentReference>,
    pub editor_context: Option<AgentEditorContext>,
}

/// A persisted entry in the agent session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEntry {
    pub id: String,
    pub role: String,
    pub content: String,
    pub tool_calls: Vec<AgentToolCallEntry>,
    pub references: Vec<AgentReference>,
    pub editor_context: Option<AgentEditorContext>,
    pub created_at_ms: u64,
}

// ── Converters (mirroring agent.go entriesToAgentMessages / agentMessagesToEntries) ──

pub fn entries_to_agent_messages(entries: &[SessionEntry]) -> Vec<AgentMessage> {
    entries
        .iter()
        .map(|e| AgentMessage {
            role: e.role.clone(),
            content: e.content.clone(),
            tool_calls: e.tool_calls.clone(),
            entry_id: Some(e.id.clone()),
            references: e.references.clone(),
            editor_context: e.editor_context.clone(),
            thinking_content: None,
        })
        .collect()
}

pub fn agent_messages_to_entries(msgs: &[AgentMessage]) -> Vec<SessionEntry> {
    msgs.iter()
        .map(|m| SessionEntry {
            id: m.entry_id.clone().unwrap_or_default(),
            role: m.role.clone(),
            content: m.content.clone(),
            tool_calls: m.tool_calls.clone(),
            references: m.references.clone(),
            editor_context: m.editor_context.clone(),
            created_at_ms: 0,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip_messages_to_entries() {
        let msg = AgentMessage {
            role: "user".into(),
            content: "hello".into(),
            tool_calls: vec![],
            entry_id: Some("e1".into()),
            references: vec![],
            editor_context: None,
            thinking_content: None,
        };
        let entries = agent_messages_to_entries(&[msg]);
        let back = entries_to_agent_messages(&entries);
        assert_eq!(back[0].content, "hello");
    }
    #[test]
    fn editor_context_includes_ids_only() {
        let ctx = AgentEditorContext {
            active_doc_id: Some(uuid::Uuid::new_v4()),
            active_doc_title: Some("Doc".into()),
            notebook_id: None,
            focused_block_id: None,
            selected_block_ids: vec![],
            visible_block_ids: vec![],
        };
        let json = serde_json::to_string(&ctx).unwrap();
        assert!(!json.contains("content"));
    }
}
