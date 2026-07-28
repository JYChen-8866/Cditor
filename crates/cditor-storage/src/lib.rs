pub mod backend;
pub mod checkpoint;
pub mod error;
pub mod height_write_debounce;
pub mod layout_cache;
pub mod optimistic_persistence;
pub mod page_layout_snapshot;
pub mod traits;
pub mod version;

pub use backend::{
    DocumentLoadProgress, DocumentLoadStage, DocumentStorage, EmergencyLogAppendOutcome,
    EmergencyLogEntry, LoadDocumentRequest, LoadedDocument, LoadedPayloadBatch, StorageBackendKind,
    StorageCapabilities, StorageDocumentMetadata, StorageSaveBatch, StorageSaveOutcome,
};
pub use checkpoint::{
    MATERIALIZED_CHECKPOINT_FORMAT, MATERIALIZED_CHECKPOINT_VERSION, MaterializedCheckpoint,
    MaterializedDocumentState, MaterializedRebuildPlan,
};
pub use error::{StorageError, StorageResult};
pub use page_layout_snapshot::{StoragePageLayoutPage, StoragePageLayoutSnapshot};
pub use version::DOCUMENT_INDEX_VISIBLE_VERSION;
