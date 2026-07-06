pub mod asset_store;
pub mod crash_recovery;
pub mod demo_seed;
pub mod document_store;
pub mod error;
pub mod fts_store;
pub mod layout_store;
pub mod migrations;
pub mod payload_store;
pub mod persistence_queue;
pub mod pool;
pub mod runtime;
pub mod sync_outbox;
pub mod transaction_store;
pub mod types;

#[cfg(test)]
mod postgres_integration;

pub use asset_store::{AssetRecord, BlockAssetRecord, PostgresAssetStore, StoredAssetRecord};
pub use crash_recovery::{
    DirtyBlockRecoveryRecord, PostgresCrashRecoveryStore, RuntimeSnapshotLoadResult,
    RuntimeSnapshotLoadStatus, RuntimeSnapshotRecord, StartupRecoveryResult,
};
pub use demo_seed::{LargeDemoSeedOptions, LargeDemoSeedReport, ensure_large_mixed_demo_seeded};
pub use document_store::{PostgresDocumentIndexSnapshot, PostgresDocumentStore};
pub use error::{PostgresStorageError, PostgresStorageResult};
pub use fts_store::{FtsSearchResult, FtsUpsertResult, PostgresFtsStore};
pub use layout_store::PostgresLayoutCacheStore;
pub use migrations::{INITIAL_SCHEMA_MIGRATION, INITIAL_SCHEMA_VERSION, run_migrations};
pub use payload_store::{LoadBlockPayloadsResult, PostgresPayloadStore};
pub use persistence_queue::{
    PersistenceQueueRow, PersistenceQueueState, PersistenceQueueTask, PersistenceTaskKind,
    PersistenceWorkerCommand, PostgresPersistenceQueue, WorkerProcessReport,
};
pub use pool::{PostgresPoolConfig, create_pg_pool, health_check};
pub use runtime::block_on_postgres;
pub use sync_outbox::{
    PostgresSyncOutboxStore, RemoteTombstoneRecord, SyncClientIdentity, SyncOutboxInsertResult,
    SyncOutboxRecord, SyncOutboxState, SyncStateRecord, pg_tombstone_block_entity_id,
};
pub use transaction_store::{
    EditTransactionVersions, PostgresTransactionStore, StoredEditTransaction,
    pg_transaction_id_from_runtime,
};
pub use types::{DocumentRow, PgDocumentId, pg_block_id_from_runtime, pg_document_id_from_runtime};
