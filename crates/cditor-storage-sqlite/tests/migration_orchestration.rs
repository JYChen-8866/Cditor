use std::borrow::Cow;
use std::path::Path;

use cditor_storage_sqlite::{
    MigrationCancellation, SqliteDocumentStorage, SqliteMigrationManager, SqliteMigrationStage,
    SqliteStorageOptions,
};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};
use tempfile::TempDir;
use uuid::Uuid;

const UNKNOWN_KIND_JSON: &str = r#"{"Plugin":{"vendor":"future.plugin","kind":9001}}"#;
const UNKNOWN_PAYLOAD_JSON: &str =
    r#"{ "opaque" : [1, 2, 3], "future_field":"\u4f60\u597d", "nested":{"z":1,"a":2} }"#;
const UNKNOWN_ATTRS_JSON: &str = r#"{"color":null,"future_attr":{"raw":true}}"#;

async fn raw_pool(path: &Path, create: bool) -> SqlitePool {
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(path)
                .create_if_missing(create)
                .foreign_keys(true),
        )
        .await
        .expect("open raw sqlite pool")
}

async fn create_v1_fixture(path: &Path) {
    let all =
        sqlx::migrate::Migrator::new(Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations"))
            .await
            .expect("load migrations");
    let v1 = all
        .iter()
        .find(|migration| migration.version == 1)
        .expect("v1 migration")
        .clone();
    let migrator = sqlx::migrate::Migrator {
        migrations: Cow::Owned(vec![v1]),
        ignore_missing: false,
        locking: true,
        no_tx: false,
    };
    let pool = raw_pool(path, true).await;
    migrator.run(&pool).await.expect("apply v1");

    let workspace_id = Uuid::from_u128(1);
    let document_id = Uuid::from_u128(7);
    let block_id = Uuid::from_u128(11);
    sqlx::query(
        "INSERT INTO workspaces (id, name, created_at, updated_at) VALUES (?, 'Legacy', 10, 10)",
    )
    .bind(workspace_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO documents (id, workspace_id, title, structure_version, content_version, layout_version, schema_version, created_at, updated_at) VALUES (?, ?, 'Fixture', 4, 9, 2, 1, 10, 20)",
    )
    .bind(document_id)
    .bind(workspace_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO blocks (id, document_id, parent_id, sort_key, depth, kind_tag, flags, content_version, structure_version, estimated_height, measured_height, width_bucket, layout_version, layout_dirty, updated_at) VALUES (?, ?, NULL, '0001', 0, 9001, 0, 9, 4, 24, 25, 80, 2, 0, 20)",
    )
    .bind(block_id)
    .bind(document_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO block_attrs (document_id, block_id, attrs_json, updated_at) VALUES (?, ?, ?, 20)",
    )
    .bind(document_id)
    .bind(block_id)
    .bind(UNKNOWN_ATTRS_JSON)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO block_payloads (block_id, document_id, kind_json, payload_json, plain_text, content_version, byte_len, updated_at) VALUES (?, ?, ?, ?, 'fallback', 9, ?, 20)",
    )
    .bind(block_id)
    .bind(document_id)
    .bind(UNKNOWN_KIND_JSON)
    .bind(UNKNOWN_PAYLOAD_JSON)
    .bind(i64::try_from(UNKNOWN_PAYLOAD_JSON.len()).unwrap())
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO page_layout (document_id, structure_version, layout_key_hash, page_policy_version, page_index, height, updated_at) VALUES (?, 4, 'legacy-layout', 1, 0, 720, 20)",
    )
    .bind(document_id)
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;
}

async fn migration_version(path: &Path) -> i64 {
    let pool = raw_pool(path, false).await;
    let version = sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(MAX(version), 0) FROM _sqlx_migrations WHERE success = 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    pool.close().await;
    version
}

async fn raw_fixture_values(path: &Path) -> (String, String, String) {
    let pool = raw_pool(path, false).await;
    let row = sqlx::query(
        "SELECT p.kind_json, p.payload_json, a.attrs_json FROM block_payloads p JOIN block_attrs a ON a.document_id = p.document_id AND a.block_id = p.block_id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let values = (
        row.get::<String, _>(0),
        row.get::<String, _>(1),
        row.get::<String, _>(2),
    );
    pool.close().await;
    values
}

#[tokio::test]
async fn open_runs_backup_dry_run_validation_and_preserves_unknown_bytes() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("legacy.db");
    create_v1_fixture(&path).await;

    let store = SqliteDocumentStorage::open(SqliteStorageOptions::file(&path))
        .await
        .expect("migrate legacy database");
    let report = store
        .last_migration_report()
        .expect("migration report must remain inspectable");
    assert_eq!(report.plan.source_version, 1);
    assert_eq!(report.plan.target_version, 4);
    assert_eq!(
        report
            .plan
            .pending
            .iter()
            .map(|migration| migration.version)
            .collect::<Vec<_>>(),
        vec![2, 3, 4]
    );
    assert!(report.backup_path.is_file());
    assert_eq!(report.before.checksums, report.dry_run.checksums);
    assert_eq!(report.before.checksums, report.after.checksums);
    let backup_path = report.backup_path.clone();
    store.pool().close().await;

    assert_eq!(migration_version(&path).await, 4);
    assert_eq!(
        raw_fixture_values(&path).await,
        (
            UNKNOWN_KIND_JSON.to_owned(),
            UNKNOWN_PAYLOAD_JSON.to_owned(),
            UNKNOWN_ATTRS_JSON.to_owned()
        )
    );

    SqliteMigrationManager::rollback(&path, &backup_path)
        .await
        .expect("restore verified backup");
    assert_eq!(migration_version(&path).await, 1);
    assert_eq!(
        raw_fixture_values(&path).await,
        (
            UNKNOWN_KIND_JSON.to_owned(),
            UNKNOWN_PAYLOAD_JSON.to_owned(),
            UNKNOWN_ATTRS_JSON.to_owned()
        )
    );
}

#[tokio::test]
async fn progress_reports_each_safe_resume_boundary() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("progress.db");
    create_v1_fixture(&path).await;
    let options = SqliteStorageOptions::file(&path);
    let mut events = Vec::new();

    let report = SqliteMigrationManager::migrate_with(
        &options,
        &MigrationCancellation::default(),
        |event| events.push(event),
    )
    .await
    .expect("migrate")
    .expect("report");

    for stage in [SqliteMigrationStage::DryRun, SqliteMigrationStage::Applying] {
        assert_eq!(
            events
                .iter()
                .filter(|event| event.stage == stage && event.migration_version.is_some())
                .map(|event| event.migration_version.unwrap())
                .collect::<Vec<_>>(),
            vec![2, 3, 4]
        );
    }
    assert!(events.iter().any(|event| {
        event.stage == SqliteMigrationStage::Completed && event.completed_units == event.total_units
    }));
    assert_eq!(report.before.checksums, report.after.checksums);
}

#[tokio::test]
async fn cancellation_before_backup_leaves_source_untouched() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("cancel.db");
    create_v1_fixture(&path).await;
    let cancellation = MigrationCancellation::default();
    cancellation.cancel();

    let error = SqliteMigrationManager::migrate_with(
        &SqliteStorageOptions::file(&path),
        &cancellation,
        |_| {},
    )
    .await
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("cancelled at Preflight boundary")
    );
    assert_eq!(migration_version(&path).await, 1);
    assert_eq!(
        std::fs::read_dir(dir.path()).unwrap().count(),
        1,
        "cancel before backup must not leave artifacts"
    );
}

#[tokio::test]
async fn cancellation_between_formal_migrations_automatically_restores_backup() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("rollback.db");
    create_v1_fixture(&path).await;
    let cancellation = MigrationCancellation::default();
    let cancel_from_progress = cancellation.clone();
    let mut events = Vec::new();

    let error = SqliteMigrationManager::migrate_with(
        &SqliteStorageOptions::file(&path),
        &cancellation,
        |event| {
            if event.stage == SqliteMigrationStage::Applying && event.migration_version == Some(2) {
                cancel_from_progress.cancel();
            }
            events.push(event);
        },
    )
    .await
    .unwrap_err();

    assert!(error.to_string().contains("cancelled at Applying boundary"));
    assert!(events.iter().any(|event| {
        event.stage == SqliteMigrationStage::RollingBack && event.completed_units == 1
    }));
    assert_eq!(migration_version(&path).await, 1);
    assert_eq!(
        raw_fixture_values(&path).await,
        (
            UNKNOWN_KIND_JSON.to_owned(),
            UNKNOWN_PAYLOAD_JSON.to_owned(),
            UNKNOWN_ATTRS_JSON.to_owned()
        )
    );
}
