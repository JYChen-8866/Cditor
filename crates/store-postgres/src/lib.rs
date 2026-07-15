pub mod adapter;
pub mod demo_seed;
pub mod error;
pub mod migrations;
pub mod pool;
pub mod queue;
pub mod runtime;
pub mod stores;
pub mod types;

#[cfg(test)]
mod postgres_integration;

pub use adapter::PostgresDocumentStorage;
pub use demo_seed::{LargeDemoSeedOptions, LargeDemoSeedReport, ensure_large_mixed_demo_seeded};
pub use error::{PostgresStorageError, PostgresStorageResult};
pub use migrations::{INITIAL_SCHEMA_MIGRATION, INITIAL_SCHEMA_VERSION, run_migrations};
pub use pool::{PostgresPoolConfig, create_pg_pool, health_check};
pub use queue::persistence::{
    PersistenceQueueRow, PersistenceQueueState, PersistenceQueueTask, PersistenceTaskKind,
    PersistenceWorkerCommand, PostgresPersistenceQueue, WorkerProcessReport,
};
pub use runtime::block_on_postgres;
pub use stores::asset::{AssetRecord, BlockAssetRecord, PostgresAssetStore, StoredAssetRecord};
pub use stores::crash_recovery::{
    DirtyBlockRecoveryRecord, PostgresCrashRecoveryStore, RuntimeSnapshotLoadResult,
    RuntimeSnapshotLoadStatus, RuntimeSnapshotRecord, StartupRecoveryResult,
};
pub use stores::document::{PostgresDocumentIndexSnapshot, PostgresDocumentStore};
pub use stores::fts::{FtsSearchResult, FtsUpsertResult, PostgresFtsStore};
pub use stores::layout::PostgresLayoutCacheStore;
pub use stores::payload::{LoadBlockPayloadsResult, PostgresPayloadStore};
pub use stores::sync_outbox::{
    PostgresSyncOutboxStore, RemoteTombstoneRecord, SyncClientIdentity, SyncOutboxInsertResult,
    SyncOutboxRecord, SyncOutboxState, SyncStateRecord, pg_tombstone_block_entity_id,
};
pub use stores::transaction::{
    EditTransactionVersions, PostgresTransactionStore, StoredEditTransaction,
    pg_transaction_id_from_runtime,
};
pub use types::{DocumentRow, PgDocumentId, pg_block_id_from_runtime, pg_document_id_from_runtime};
