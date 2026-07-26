#[cfg(test)]
extern crate self as cditor_storage_sqlite;

mod asset_manifest;
mod bootstrap;
mod checkpoint;
mod codec;
mod config;
mod error;
mod fault_injection;
mod ids;
mod journal;
mod layout;
mod migration;
mod page_layout;
mod payload;
mod provider;
mod query_index;
mod recovery;
mod snapshot;
mod storage;
mod undo_blob;
mod util;
mod writer;

pub use config::{SqliteDurability, SqliteStorageOptions};
pub use journal::{
    InboxEntry, JournalEntry, OutboxEntry, OutboxState, StartupRecovery, SyncAckCursor,
};
pub use migration::{
    MigrationCancellation, SqliteMigrationChecksums, SqliteMigrationDescriptor,
    SqliteMigrationManager, SqliteMigrationPlan, SqliteMigrationProgress, SqliteMigrationReport,
    SqliteMigrationStage, SqliteMigrationValidation,
};
pub use provider::SqliteStorageProvider;
pub use recovery::{SqliteRecoveryCopy, SqliteRecoveryCopyStatus};
pub use storage::SqliteDocumentStorage;

#[cfg(test)]
mod integration_tests {
    mod asset_manifest {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/asset_manifest.rs"
        ));
    }

    mod checkpoint_rebuild {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/checkpoint_rebuild.rs"
        ));
    }

    mod corruption_recovery {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/corruption_recovery.rs"
        ));
    }

    mod fault_injection {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fault_injection.rs"
        ));
    }

    mod journal_recovery {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/journal_recovery.rs"
        ));
    }

    mod local_query_index {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/local_query_index.rs"
        ));
    }

    mod migration_orchestration {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/migration_orchestration.rs"
        ));
    }

    mod shared_contract {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/shared_contract.rs"
        ));
    }

    mod storage_contract {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/storage_contract.rs"
        ));
    }

    mod undo_blob {
        include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/undo_blob.rs"));
    }
}
