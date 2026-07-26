#![cfg_attr(test, allow(dead_code, unused_imports))]

mod adapter;
mod error;
mod migrations;
mod pool;
mod provider;
#[cfg_attr(not(test), allow(dead_code))]
mod queue;
#[cfg_attr(not(test), allow(dead_code))]
mod stores;
#[cfg_attr(not(test), allow(dead_code))]
mod types;

#[cfg(test)]
mod postgres_integration;

pub(crate) use adapter::PostgresDocumentStorage;
pub use error::{PostgresStorageError, PostgresStorageResult};
pub use migrations::{INITIAL_SCHEMA_MIGRATION, INITIAL_SCHEMA_VERSION, run_migrations};
pub use pool::{PostgresPoolConfig, create_pg_pool, health_check};
pub use provider::PostgresStorageProvider;
#[cfg(test)]
pub(crate) use queue::persistence::{
    PersistenceQueueRow, PersistenceQueueState, PersistenceQueueTask, PersistenceTaskKind,
    PersistenceWorkerCommand, PostgresPersistenceQueue, WorkerProcessReport,
};
#[cfg(test)]
pub(crate) use stores::asset::{
    AssetRecord, BlockAssetRecord, PostgresAssetStore, StoredAssetRecord,
};
#[cfg(test)]
pub(crate) use stores::crash_recovery::{
    DirtyBlockRecoveryRecord, PostgresCrashRecoveryStore, RuntimeSnapshotLoadResult,
    RuntimeSnapshotLoadStatus, RuntimeSnapshotRecord, StartupRecoveryResult,
};
#[cfg(test)]
pub(crate) use stores::document::PostgresDocumentIndexSnapshot;
pub(crate) use stores::document::PostgresDocumentStore;
#[cfg(test)]
pub(crate) use stores::fts::{FtsSearchResult, FtsUpsertResult, PostgresFtsStore};
pub(crate) use stores::layout::PostgresLayoutCacheStore;
#[cfg(test)]
pub(crate) use stores::payload::LoadBlockPayloadsResult;
pub(crate) use stores::payload::PostgresPayloadStore;
#[cfg(test)]
pub(crate) use stores::sync_outbox::{
    PostgresSyncOutboxStore, RemoteTombstoneRecord, SyncClientIdentity, SyncOutboxInsertResult,
    SyncOutboxRecord, SyncOutboxState, SyncStateRecord, pg_tombstone_block_entity_id,
};
pub(crate) use stores::transaction::{EditTransactionVersions, PostgresTransactionStore};
#[cfg(test)]
pub(crate) use stores::transaction::{StoredEditTransaction, pg_transaction_id_from_runtime};
#[cfg(test)]
pub(crate) use types::{
    DocumentRow, PgDocumentId, pg_block_id_from_runtime, pg_document_id_from_runtime,
};
