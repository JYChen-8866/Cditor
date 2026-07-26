//! SQLite schema migration orchestration（P1-013 / P7-013）。
//!
//! SQLx 负责单个 migration 的事务和版本账本；本模块负责其外层的数据安全协议：
//! preflight -> 一致性备份 -> 隔离 dry-run -> 语义/unknown 校验 -> 正式迁移。
//! 正式阶段失败时会在关闭所有连接后从备份原子恢复，避免半升级数据库可见。

use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{Row, SqlitePool};

use cditor_storage::{StorageBackendKind, StorageError, StorageResult};

use crate::config::prepare_path;
use crate::config::{SqliteDurability, SqliteStorageOptions};
use crate::error::sqlite_error;

mod validation;

use validation::{ensure_base_validation, table_exists, validate_database};

pub(crate) static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqliteMigrationDescriptor {
    pub version: i64,
    pub description: String,
    pub checksum_sha384: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqliteMigrationPlan {
    pub database_path: PathBuf,
    pub source_version: i64,
    pub target_version: i64,
    pub database_bytes: u64,
    pub required_free_bytes: u64,
    pub available_free_bytes: Option<u64>,
    pub pending: Vec<SqliteMigrationDescriptor>,
}

impl SqliteMigrationPlan {
    pub fn requires_migration(&self) -> bool {
        !self.pending.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqliteMigrationChecksums {
    pub semantic_sha256: String,
    pub unknown_raw_sha256: String,
    pub asset_refs_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqliteMigrationValidation {
    pub integrity_ok: bool,
    pub foreign_key_violations: u64,
    pub checksums: SqliteMigrationChecksums,
}

impl SqliteMigrationValidation {
    fn is_valid_and_preserves(&self, before: &Self) -> bool {
        self.integrity_ok && self.foreign_key_violations == 0 && self.checksums == before.checksums
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqliteMigrationStage {
    Preflight,
    Backup,
    DryRun,
    Applying,
    Validating,
    Completed,
    RollingBack,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqliteMigrationProgress {
    pub stage: SqliteMigrationStage,
    pub completed_units: usize,
    pub total_units: usize,
    pub migration_version: Option<i64>,
}

#[derive(Debug, Clone, Default)]
pub struct MigrationCancellation(Arc<AtomicBool>);

impl MigrationCancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqliteMigrationReport {
    pub plan: SqliteMigrationPlan,
    pub backup_path: PathBuf,
    pub before: SqliteMigrationValidation,
    pub dry_run: SqliteMigrationValidation,
    pub after: SqliteMigrationValidation,
    pub elapsed: Duration,
}

pub struct SqliteMigrationManager;

impl SqliteMigrationManager {
    pub async fn preflight(options: &SqliteStorageOptions) -> StorageResult<SqliteMigrationPlan> {
        prepare_path(options)?;
        let target_version = MIGRATOR
            .iter()
            .map(|migration| migration.version)
            .max()
            .unwrap_or(0);
        if !options.path.exists() || std::fs::metadata(&options.path).map_err(io_error)?.len() == 0
        {
            return Ok(SqliteMigrationPlan {
                database_path: options.path.clone(),
                source_version: 0,
                target_version,
                database_bytes: 0,
                required_free_bytes: 0,
                available_free_bytes: available_space(
                    options.path.parent().unwrap_or(Path::new(".")),
                ),
                pending: Vec::new(),
            });
        }

        let database_bytes = database_footprint(&options.path)?;
        let required_free_bytes = database_bytes
            .saturating_mul(3)
            .saturating_add(16 * 1024 * 1024);
        let available_free_bytes = available_space(options.path.parent().unwrap_or(Path::new(".")));
        if available_free_bytes.is_some_and(|available| available < required_free_bytes) {
            return Err(migration_error(format!(
                "preflight requires {required_free_bytes} free bytes but only {} are available",
                available_free_bytes.unwrap_or_default()
            )));
        }

        let pool = connect_pool(options, false).await?;
        let result = inspect_applied_migrations(
            &pool,
            options.path.clone(),
            target_version,
            database_bytes,
            required_free_bytes,
            available_free_bytes,
        )
        .await;
        pool.close().await;
        result
    }

    pub async fn migrate_if_needed(
        options: &SqliteStorageOptions,
    ) -> StorageResult<Option<SqliteMigrationReport>> {
        Self::migrate_with(options, &MigrationCancellation::default(), |_| {}).await
    }

    pub async fn migrate_with(
        options: &SqliteStorageOptions,
        cancellation: &MigrationCancellation,
        mut progress: impl FnMut(SqliteMigrationProgress),
    ) -> StorageResult<Option<SqliteMigrationReport>> {
        let started = Instant::now();
        progress(event(SqliteMigrationStage::Preflight, 0, 1, None));
        let plan = Self::preflight(options).await?;
        if !plan.requires_migration() {
            return Ok(None);
        }
        check_cancelled(cancellation, SqliteMigrationStage::Preflight)?;
        progress(event(SqliteMigrationStage::Preflight, 1, 1, None));

        let source_pool = connect_pool(options, false).await?;
        let before = validate_database(&source_pool).await?;
        ensure_base_validation(&before, "source database")?;

        progress(event(SqliteMigrationStage::Backup, 0, 1, None));
        let backup_path = unique_sibling(&options.path, "migration-backup")?;
        create_consistent_backup(&source_pool, &backup_path).await?;
        progress(event(SqliteMigrationStage::Backup, 1, 1, None));
        source_pool.close().await;
        check_cancelled(cancellation, SqliteMigrationStage::Backup)?;

        let dry_run_path = unique_sibling(&options.path, "migration-dry-run")?;
        std::fs::copy(&backup_path, &dry_run_path).map_err(io_error)?;
        let mut dry_options = options.clone();
        dry_options.path = dry_run_path.clone();
        dry_options.create_if_missing = false;
        let dry_result = async {
            let dry_pool = connect_pool(&dry_options, false).await?;
            let apply_result = apply_pending(
                &dry_pool,
                &plan,
                cancellation,
                SqliteMigrationStage::DryRun,
                &mut progress,
            )
            .await;
            let validation = match apply_result {
                Ok(()) => validate_database(&dry_pool).await,
                Err(error) => Err(error),
            };
            dry_pool.close().await;
            validation
        }
        .await;
        remove_database_files(&dry_run_path);
        let dry_run = dry_result?;
        if !dry_run.is_valid_and_preserves(&before) {
            return Err(migration_error(format!(
                "dry-run validation failed: before={:?}, after={dry_run:?}",
                before.checksums
            )));
        }
        check_cancelled(cancellation, SqliteMigrationStage::DryRun)?;

        let source_pool = connect_pool(options, false).await?;
        let apply_result = apply_pending(
            &source_pool,
            &plan,
            cancellation,
            SqliteMigrationStage::Applying,
            &mut progress,
        )
        .await;
        progress(event(SqliteMigrationStage::Validating, 0, 1, None));
        let after_result = match apply_result {
            Ok(()) => validate_database(&source_pool)
                .await
                .and_then(|validation| {
                    if validation.is_valid_and_preserves(&before) {
                        Ok(validation)
                    } else {
                        Err(migration_error(format!(
                            "post-migration validation failed: before={:?}, after={:?}",
                            before.checksums, validation
                        )))
                    }
                }),
            Err(error) => Err(error),
        };
        source_pool.close().await;

        let after = match after_result {
            Ok(after) => after,
            Err(error) => {
                progress(event(SqliteMigrationStage::RollingBack, 0, 1, None));
                if let Err(rollback_error) = Self::rollback(&options.path, &backup_path).await {
                    return Err(migration_error(format!(
                        "{error}; automatic rollback also failed: {rollback_error}"
                    )));
                }
                progress(event(SqliteMigrationStage::RollingBack, 1, 1, None));
                return Err(error);
            }
        };
        progress(event(SqliteMigrationStage::Validating, 1, 1, None));
        progress(event(SqliteMigrationStage::Completed, 1, 1, None));
        Ok(Some(SqliteMigrationReport {
            plan,
            backup_path,
            before,
            dry_run,
            after,
            elapsed: started.elapsed(),
        }))
    }

    /// 用已验证备份原子替换数据库。调用前必须关闭该路径上的所有连接。
    pub async fn rollback(database_path: &Path, backup_path: &Path) -> StorageResult<()> {
        if !backup_path.is_file() {
            return Err(migration_error(format!(
                "backup does not exist: {}",
                backup_path.display()
            )));
        }
        let options = SqliteStorageOptions::file(backup_path).create_if_missing(false);
        let backup_pool = connect_pool(&options, false).await?;
        let validation = validate_database(&backup_pool).await?;
        backup_pool.close().await;
        ensure_base_validation(&validation, "migration backup")?;

        let restore_path = unique_sibling(database_path, "migration-restore")?;
        std::fs::copy(backup_path, &restore_path).map_err(io_error)?;
        let restore = std::fs::OpenOptions::new()
            .write(true)
            .open(&restore_path)
            .map_err(io_error)?;
        restore.sync_all().map_err(io_error)?;
        remove_sidecars(database_path);
        if let Err(error) = std::fs::rename(&restore_path, database_path) {
            let _ = std::fs::remove_file(&restore_path);
            return Err(io_error(error));
        }
        sync_parent(database_path)?;
        Ok(())
    }
}

pub(crate) async fn connect_pool(
    options: &SqliteStorageOptions,
    create_if_missing: bool,
) -> StorageResult<SqlitePool> {
    let synchronous = match options.durability {
        SqliteDurability::Full => SqliteSynchronous::Full,
        SqliteDurability::Balanced => SqliteSynchronous::Normal,
    };
    let connect = SqliteConnectOptions::new()
        .filename(&options.path)
        .create_if_missing(create_if_missing)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(synchronous)
        .busy_timeout(options.busy_timeout);
    SqlitePoolOptions::new()
        .max_connections(options.max_connections)
        .after_connect(|connection, _| {
            Box::pin(async move {
                sqlx::query("PRAGMA foreign_keys = ON")
                    .execute(&mut *connection)
                    .await?;
                sqlx::query("PRAGMA temp_store = MEMORY")
                    .execute(&mut *connection)
                    .await?;
                sqlx::query("PRAGMA wal_autocheckpoint = 1000")
                    .execute(&mut *connection)
                    .await?;
                Ok(())
            })
        })
        .connect_with(connect)
        .await
        .map_err(sqlite_error)
}

async fn inspect_applied_migrations(
    pool: &SqlitePool,
    database_path: PathBuf,
    target_version: i64,
    database_bytes: u64,
    required_free_bytes: u64,
    available_free_bytes: Option<u64>,
) -> StorageResult<SqliteMigrationPlan> {
    if !table_exists(pool, "_sqlx_migrations").await? {
        return Err(migration_error(
            "existing SQLite database has no SQLx migration ledger; refusing an unverified upgrade",
        ));
    }
    let rows =
        sqlx::query("SELECT version, success, checksum FROM _sqlx_migrations ORDER BY version")
            .fetch_all(pool)
            .await
            .map_err(sqlite_error)?;
    let mut source_version = 0;
    for row in rows {
        let version: i64 = row.try_get("version").map_err(sqlite_error)?;
        let success: bool = row.try_get("success").map_err(sqlite_error)?;
        let checksum: Vec<u8> = row.try_get("checksum").map_err(sqlite_error)?;
        if !success {
            return Err(migration_error(format!(
                "migration {version} is marked incomplete"
            )));
        }
        let Some(known) = MIGRATOR
            .iter()
            .find(|migration| migration.version == version)
        else {
            return Err(migration_error(format!(
                "database contains unknown migration version {version}"
            )));
        };
        if known.checksum.as_ref() != checksum {
            return Err(migration_error(format!(
                "migration {version} checksum does not match this build"
            )));
        }
        source_version = source_version.max(version);
    }
    let pending = MIGRATOR
        .iter()
        .filter(|migration| migration.version > source_version)
        .map(descriptor)
        .collect();
    Ok(SqliteMigrationPlan {
        database_path,
        source_version,
        target_version,
        database_bytes,
        required_free_bytes,
        available_free_bytes,
        pending,
    })
}

async fn apply_pending(
    pool: &SqlitePool,
    plan: &SqliteMigrationPlan,
    cancellation: &MigrationCancellation,
    stage: SqliteMigrationStage,
    progress: &mut impl FnMut(SqliteMigrationProgress),
) -> StorageResult<()> {
    let total = plan.pending.len();
    progress(event(stage, 0, total, None));
    for (index, pending) in plan.pending.iter().enumerate() {
        check_cancelled(cancellation, stage)?;
        let migration = MIGRATOR
            .iter()
            .find(|migration| migration.version == pending.version)
            .expect("plan only contains embedded migrations")
            .clone();
        let one = sqlx::migrate::Migrator {
            migrations: Cow::Owned(vec![migration]),
            ignore_missing: true,
            locking: true,
            no_tx: false,
        };
        one.run(pool).await.map_err(|error| {
            migration_error(format!("migration {} failed: {error}", pending.version))
        })?;
        progress(event(stage, index + 1, total, Some(pending.version)));
    }
    Ok(())
}

async fn create_consistent_backup(pool: &SqlitePool, backup_path: &Path) -> StorageResult<()> {
    if backup_path.exists() {
        return Err(migration_error(format!(
            "refusing to overwrite backup {}",
            backup_path.display()
        )));
    }
    let path = backup_path.to_string_lossy().into_owned();
    sqlx::query("VACUUM main INTO ?")
        .bind(path)
        .execute(pool)
        .await
        .map_err(sqlite_error)?;
    let backup = std::fs::OpenOptions::new()
        .read(true)
        .open(backup_path)
        .map_err(io_error)?;
    backup.sync_all().map_err(io_error)?;
    sync_parent(backup_path)
}

fn descriptor(migration: &sqlx::migrate::Migration) -> SqliteMigrationDescriptor {
    SqliteMigrationDescriptor {
        version: migration.version,
        description: migration.description.to_string(),
        checksum_sha384: hex(migration.checksum.as_ref()),
    }
}

fn event(
    stage: SqliteMigrationStage,
    completed_units: usize,
    total_units: usize,
    migration_version: Option<i64>,
) -> SqliteMigrationProgress {
    SqliteMigrationProgress {
        stage,
        completed_units,
        total_units,
        migration_version,
    }
}

fn check_cancelled(
    cancellation: &MigrationCancellation,
    stage: SqliteMigrationStage,
) -> StorageResult<()> {
    if cancellation.is_cancelled() {
        return Err(migration_error(format!(
            "migration cancelled at {stage:?} boundary"
        )));
    }
    Ok(())
}

fn unique_sibling(path: &Path, label: &str) -> StorageResult<PathBuf> {
    let parent = path.parent().unwrap_or(Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("cditor.db");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| migration_error(error.to_string()))?
        .as_nanos();
    for attempt in 0..100u32 {
        let candidate = parent.join(format!("{file_name}.{label}-{now}-{attempt}"));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(migration_error(format!(
        "could not allocate unique {label} path beside {}",
        path.display()
    )))
}

fn database_footprint(path: &Path) -> StorageResult<u64> {
    let mut bytes = std::fs::metadata(path).map_err(io_error)?.len();
    for sidecar in sidecar_paths(path) {
        if let Ok(metadata) = std::fs::metadata(sidecar) {
            bytes = bytes.saturating_add(metadata.len());
        }
    }
    Ok(bytes)
}

fn remove_database_files(path: &Path) {
    let _ = std::fs::remove_file(path);
    remove_sidecars(path);
}

fn remove_sidecars(path: &Path) {
    for sidecar in sidecar_paths(path) {
        let _ = std::fs::remove_file(sidecar);
    }
}

fn sidecar_paths(path: &Path) -> [PathBuf; 2] {
    [
        PathBuf::from(format!("{}-wal", path.display())),
        PathBuf::from(format!("{}-shm", path.display())),
    ]
}

fn sync_parent(path: &Path) -> StorageResult<()> {
    let parent = std::fs::File::open(path.parent().unwrap_or(Path::new("."))).map_err(io_error)?;
    parent.sync_all().map_err(io_error)
}

#[cfg(unix)]
fn available_space(path: &Path) -> Option<u64> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let path = CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut stats = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: `path` is NUL-terminated and `stats` points to writable storage for statvfs.
    let result = unsafe { libc::statvfs(path.as_ptr(), stats.as_mut_ptr()) };
    if result != 0 {
        return None;
    }
    // SAFETY: statvfs returned success and initialized the structure.
    let stats = unsafe { stats.assume_init() };
    Some(u64::from(stats.f_bavail).saturating_mul(stats.f_frsize))
}

#[cfg(not(unix))]
fn available_space(_path: &Path) -> Option<u64> {
    None
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn migration_error(message: impl Into<String>) -> StorageError {
    StorageError::Migration {
        backend: StorageBackendKind::Sqlite,
        message: message.into(),
    }
}

fn io_error(error: std::io::Error) -> StorageError {
    StorageError::Io(error.to_string())
}
