pub mod backend;
pub mod cache_recovery;
pub mod error;
pub mod height_write_debounce;
pub mod layout_cache;
pub mod optimistic_persistence;
pub mod page_layout_snapshot;
pub mod runtime;
pub mod traits;
pub mod version;

pub use backend::{
    DocumentStorage, LoadDocumentRequest, LoadedDocument, LoadedPayloadBatch, StorageBackendKind,
    StorageCapabilities, StorageDocumentMetadata, StorageSaveBatch, StorageSaveOutcome,
    StorageSession,
};
pub use error::{StorageError, StorageResult};
pub use page_layout_snapshot::{StoragePageLayoutPage, StoragePageLayoutSnapshot};
pub use runtime::block_on_storage;
pub use version::DOCUMENT_INDEX_VISIBLE_VERSION;
