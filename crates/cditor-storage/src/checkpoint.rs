use cditor_core::document::BlockIndexRecord;
use cditor_core::ids::{BlockId, DocumentId};
use cditor_core::rich_text::{BlockAttrs, BlockPayloadRecord};

use crate::{EmergencyLogEntry, StorageDocumentMetadata};

pub const MATERIALIZED_CHECKPOINT_FORMAT: &str = "cditor-materialized-checkpoint";
pub const MATERIALIZED_CHECKPOINT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq)]
pub struct MaterializedDocumentState {
    pub metadata: StorageDocumentMetadata,
    pub records: Vec<BlockIndexRecord>,
    pub block_attrs: Vec<(BlockId, BlockAttrs)>,
    pub payloads: Vec<BlockPayloadRecord>,
}

impl MaterializedDocumentState {
    pub fn document_id(&self) -> DocumentId {
        self.metadata.document_id
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MaterializedCheckpoint {
    pub format: String,
    pub version: u32,
    pub journal_sequence: u64,
    pub checksum: u64,
    pub state: MaterializedDocumentState,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MaterializedRebuildPlan {
    pub checkpoint: MaterializedCheckpoint,
    pub operations: Vec<EmergencyLogEntry>,
}
