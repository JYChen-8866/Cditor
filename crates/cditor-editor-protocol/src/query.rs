use cditor_core::ids::{BlockId, DocumentId};
use serde::{Deserialize, Serialize};

use crate::command::{CommandId, CommandQueryState};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CommandQuery {
    State { command_id: CommandId },
    DocumentSummary,
    BlockExists { block_id: BlockId },
    Diagnostics { include_expensive: bool },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum QueryResult {
    CommandState(CommandQueryState),
    DocumentSummary(DocumentSummary),
    BlockExists(bool),
    Diagnostics(DiagnosticsSnapshot),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentSummary {
    pub document_id: DocumentId,
    pub revision: u64,
    pub block_count: usize,
    pub readonly: bool,
    pub dirty: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticsSnapshot {
    pub resident_blocks: usize,
    pub projected_blocks: usize,
    pub pending_tasks: usize,
    pub undo_bytes: usize,
    pub stale_results_discarded: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_wire_shape_is_tagged_and_roundtrips() {
        let query = CommandQuery::BlockExists { block_id: 42 };
        let value = serde_json::to_value(&query).unwrap();
        assert_eq!(value["type"], "block_exists");
        assert_eq!(
            serde_json::from_value::<CommandQuery>(value).unwrap(),
            query
        );
    }
}
