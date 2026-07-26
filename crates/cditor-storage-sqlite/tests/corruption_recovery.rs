use std::io::{Seek, SeekFrom, Write};

use cditor_storage::{DocumentStorage, StorageError};
use cditor_storage_sqlite::{
    SqliteDocumentStorage, SqliteRecoveryCopy, SqliteRecoveryCopyStatus, SqliteStorageOptions,
};
use cditor_test_support::seed_mixed_storage_document;
use tempfile::TempDir;

const DOCUMENT_ID: u64 = 7;

async fn create_source(dir: &TempDir) -> std::path::PathBuf {
    let path = dir.path().join("document.db");
    let store = SqliteDocumentStorage::open(SqliteStorageOptions::file(&path))
        .await
        .unwrap();
    seed_mixed_storage_document(&store, DOCUMENT_ID, 4)
        .await
        .unwrap();
    store.flush().await.unwrap();
    store.pool().close().await;
    path
}

#[tokio::test]
async fn clean_recovery_copy_is_readable_and_rejects_mutation() {
    let dir = TempDir::new().unwrap();
    let source = create_source(&dir).await;
    let recovery_dir = dir.path().join("recovery");
    let recovery = SqliteRecoveryCopy::create(&source, &recovery_dir)
        .await
        .unwrap();

    assert_eq!(recovery.status(), &SqliteRecoveryCopyStatus::Readable);
    assert!(recovery.path().starts_with(&recovery_dir));
    let state = recovery
        .load_materialized_document(DOCUMENT_ID)
        .await
        .unwrap();
    assert_eq!(state.metadata.document_id, DOCUMENT_ID);
    assert_eq!(state.records.len(), 4);
    assert_eq!(state.payloads.len(), 4);

    let pool = recovery.pool().expect("readable recovery pool");
    let query_only: i64 = sqlx::query_scalar("PRAGMA query_only")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(query_only, 1);
    assert!(
        sqlx::query("UPDATE documents SET title = 'mutated' WHERE id = id")
            .execute(pool)
            .await
            .is_err()
    );

    let original = SqliteDocumentStorage::open(
        SqliteStorageOptions::file(&source).create_if_missing(false),
    )
    .await
    .unwrap();
    let title: String = sqlx::query_scalar("SELECT title FROM documents LIMIT 1")
        .fetch_one(original.pool())
        .await
        .unwrap();
    assert_ne!(title, "mutated");
}

#[tokio::test]
async fn logically_corrupt_payload_is_reported_as_corrupt_data() {
    let dir = TempDir::new().unwrap();
    let source = create_source(&dir).await;
    let store = SqliteDocumentStorage::open(
        SqliteStorageOptions::file(&source).create_if_missing(false),
    )
    .await
    .unwrap();
    sqlx::query("UPDATE block_payloads SET payload_json = '{'")
        .execute(store.pool())
        .await
        .unwrap();
    store.pool().close().await;

    let recovery = SqliteRecoveryCopy::create(&source, &dir.path().join("recovery"))
        .await
        .unwrap();
    assert_eq!(recovery.status(), &SqliteRecoveryCopyStatus::Readable);
    let error = recovery
        .load_materialized_document(DOCUMENT_ID)
        .await
        .unwrap_err();
    assert!(matches!(error, StorageError::CorruptData(_)));
}

#[tokio::test]
async fn physically_corrupt_source_is_preserved_as_an_unreadable_artifact() {
    let dir = TempDir::new().unwrap();
    let source = create_source(&dir).await;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .open(&source)
        .unwrap();
    file.seek(SeekFrom::Start(0)).unwrap();
    file.write_all(b"not-a-sqlite-db!").unwrap();
    file.sync_all().unwrap();
    let damaged_source = std::fs::read(&source).unwrap();

    let recovery = SqliteRecoveryCopy::create(&source, &dir.path().join("recovery"))
        .await
        .unwrap();
    assert!(recovery.path().is_file());
    assert!(matches!(
        recovery.status(),
        SqliteRecoveryCopyStatus::Unreadable { .. }
            | SqliteRecoveryCopyStatus::IntegrityCheckFailed { .. }
    ));
    assert_eq!(std::fs::read(&source).unwrap(), damaged_source);
}

#[tokio::test]
async fn recovery_requires_an_existing_source_and_never_overwrites_an_old_copy() {
    let dir = TempDir::new().unwrap();
    let missing = dir.path().join("missing.db");
    let error = SqliteRecoveryCopy::create(&missing, &dir.path().join("recovery"))
        .await
        .unwrap_err();
    assert!(matches!(error, StorageError::NotFound { .. }));

    let source = create_source(&dir).await;
    let first = SqliteRecoveryCopy::create(&source, &dir.path().join("recovery"))
        .await
        .unwrap();
    let first_path = first.path().to_owned();
    let first_bytes = std::fs::read(&first_path).unwrap();
    let second = SqliteRecoveryCopy::create(&source, &dir.path().join("recovery"))
        .await
        .unwrap();
    assert_ne!(first_path, second.path());
    assert_eq!(std::fs::read(first_path).unwrap(), first_bytes);
}
