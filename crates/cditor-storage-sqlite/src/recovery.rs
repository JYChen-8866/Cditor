use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use cditor_core::ids::DocumentId;
use cditor_storage::{MaterializedDocumentState, StorageError, StorageResult};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

use crate::config::SqliteStorageOptions;
use crate::error::sqlite_error;
use crate::storage::SqliteDocumentStorage;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SqliteRecoveryCopyStatus {
    Readable,
    IntegrityCheckFailed { details: Vec<String> },
    Unreadable { error: String },
}

#[derive(Debug)]
pub struct SqliteRecoveryCopy {
    path: PathBuf,
    status: SqliteRecoveryCopyStatus,
    reader: Option<SqliteDocumentStorage>,
}

impl SqliteRecoveryCopy {
    /// Copies a stopped SQLite database into an isolated, read-only recovery artifact.
    ///
    /// The normal writer and all source connections must be closed before this call. Raw file
    /// copying is intentional: unlike `VACUUM INTO`, it can preserve a database that SQLite can no
    /// longer open. Copying a live WAL database cannot produce a coherent point-in-time snapshot.
    pub async fn create(source: &Path, destination_dir: &Path) -> StorageResult<Self> {
        if !source.is_file() {
            return Err(StorageError::NotFound {
                entity: "SQLite database",
                id: source.display().to_string(),
            });
        }
        std::fs::create_dir_all(destination_dir)
            .map_err(|error| StorageError::Io(error.to_string()))?;
        let path = unique_recovery_path(source, destination_dir)?;
        copy_and_sync(source, &path)?;
        let source_wal = sidecar_path(source, "-wal");
        if source_wal.is_file() {
            copy_and_sync(&source_wal, &sidecar_path(&path, "-wal"))?;
        }

        let connect = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(false)
            .read_only(true);
        let pool = match SqlitePoolOptions::new()
            .max_connections(1)
            .after_connect(|connection, _| {
                Box::pin(async move {
                    sqlx::query("PRAGMA query_only = ON")
                        .execute(&mut *connection)
                        .await?;
                    Ok(())
                })
            })
            .connect_with(connect)
            .await
        {
            Ok(pool) => pool,
            Err(error) => {
                return Ok(Self {
                    path,
                    status: SqliteRecoveryCopyStatus::Unreadable {
                        error: sqlite_error(error).to_string(),
                    },
                    reader: None,
                });
            }
        };
        let status = match sqlx::query_scalar::<_, String>("PRAGMA quick_check")
            .fetch_all(&pool)
            .await
        {
            Ok(rows) if rows.iter().all(|row| row == "ok") => SqliteRecoveryCopyStatus::Readable,
            Ok(rows) => SqliteRecoveryCopyStatus::IntegrityCheckFailed { details: rows },
            Err(error) => SqliteRecoveryCopyStatus::Unreadable {
                error: sqlite_error(error).to_string(),
            },
        };
        let reader = (!matches!(status, SqliteRecoveryCopyStatus::Unreadable { .. })).then(|| {
            SqliteDocumentStorage::from_recovery_pool(
                pool,
                SqliteStorageOptions::file(&path).create_if_missing(false),
            )
        });
        Ok(Self {
            path,
            status,
            reader,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn status(&self) -> &SqliteRecoveryCopyStatus {
        &self.status
    }

    pub async fn load_materialized_document(
        &self,
        document_id: DocumentId,
    ) -> StorageResult<MaterializedDocumentState> {
        let reader = self.reader.as_ref().ok_or_else(|| {
            StorageError::CorruptData("SQLite recovery copy is not readable".to_owned())
        })?;
        let metadata = reader.load_metadata(document_id).await?;
        let records = reader.load_records(document_id).await?;
        let block_attrs = reader.load_attrs(document_id).await?;
        let block_ids = records.iter().map(|record| record.id).collect::<Vec<_>>();
        let loaded = reader.load_payloads_inner(document_id, &block_ids).await?;
        if !loaded.missing_block_ids.is_empty() {
            return Err(StorageError::CorruptData(format!(
                "recovery copy is missing {} materialized payloads",
                loaded.missing_block_ids.len()
            )));
        }
        Ok(MaterializedDocumentState {
            metadata,
            records,
            block_attrs,
            payloads: loaded.records,
        })
    }

    #[cfg(test)]
    pub(crate) fn pool(&self) -> Option<&sqlx::SqlitePool> {
        self.reader.as_ref().map(|reader| &reader.pool)
    }
}

fn unique_recovery_path(source: &Path, destination_dir: &Path) -> StorageResult<PathBuf> {
    let file_name = source.file_name().ok_or_else(|| {
        StorageError::InvalidConfiguration("SQLite source path has no file name".to_owned())
    })?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| StorageError::Io(error.to_string()))?
        .as_millis();
    let mut name = file_name.to_os_string();
    name.push(format!(".recovery-{timestamp}.sqlite"));
    let mut candidate = destination_dir.join(&name);
    let mut suffix = 0u32;
    while candidate.exists() {
        suffix = suffix.saturating_add(1);
        let mut unique = name.clone();
        unique.push(format!("-{suffix}"));
        candidate = destination_dir.join(unique);
    }
    Ok(candidate)
}

fn sidecar_path(database: &Path, suffix: &str) -> PathBuf {
    let mut value: OsString = database.as_os_str().to_owned();
    value.push(suffix);
    PathBuf::from(value)
}

fn copy_and_sync(source: &Path, destination: &Path) -> StorageResult<()> {
    std::fs::copy(source, destination).map_err(|error| StorageError::Io(error.to_string()))?;
    std::fs::OpenOptions::new()
        .read(true)
        .open(destination)
        .and_then(|file| file.sync_all())
        .map_err(|error| StorageError::Io(error.to_string()))?;
    sync_parent_directory(destination)
}

#[cfg(unix)]
fn sync_parent_directory(path: &Path) -> StorageResult<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| StorageError::Io(error.to_string()))
}

#[cfg(not(unix))]
fn sync_parent_directory(_path: &Path) -> StorageResult<()> {
    Ok(())
}
