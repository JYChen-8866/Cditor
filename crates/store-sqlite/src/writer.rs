use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::Duration;

use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};

use cditor_storage::{StorageError, StorageResult};

type SharedWriterLock = Arc<AsyncMutex<()>>;

static WRITER_REGISTRY: OnceLock<Mutex<HashMap<PathBuf, Weak<AsyncMutex<()>>>>> = OnceLock::new();

#[derive(Debug, Clone)]
pub(crate) struct SqliteWriterGate {
    lock: SharedWriterLock,
    busy_timeout: Duration,
}

impl SqliteWriterGate {
    pub(crate) fn for_path(path: &Path, busy_timeout: Duration) -> StorageResult<Self> {
        let path = normalized_database_path(path)?;
        let registry = WRITER_REGISTRY.get_or_init(|| Mutex::new(HashMap::new()));
        let mut registry = registry
            .lock()
            .map_err(|_| StorageError::Io("SQLite writer registry is poisoned".to_owned()))?;
        registry.retain(|_, writer| writer.strong_count() > 0);
        let lock = registry
            .get(&path)
            .and_then(Weak::upgrade)
            .unwrap_or_else(|| {
                let lock = Arc::new(AsyncMutex::new(()));
                registry.insert(path, Arc::downgrade(&lock));
                lock
            });
        Ok(Self { lock, busy_timeout })
    }

    pub(crate) async fn acquire(&self) -> StorageResult<OwnedMutexGuard<()>> {
        tokio::time::timeout(self.busy_timeout, self.lock.clone().lock_owned())
            .await
            .map_err(|_| StorageError::Busy {
                waited: self.busy_timeout,
            })
    }
}

fn normalized_database_path(path: &Path) -> StorageResult<PathBuf> {
    if path.exists() {
        return std::fs::canonicalize(path).map_err(|error| StorageError::Io(error.to_string()));
    }
    let file_name = path.file_name().ok_or_else(|| {
        StorageError::InvalidConfiguration("SQLite path has no file name".to_owned())
    })?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let parent =
        std::fs::canonicalize(parent).map_err(|error| StorageError::Io(error.to_string()))?;
    Ok(parent.join(file_name))
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn same_canonical_path_reuses_process_writer_lock() {
        let temp = TempDir::new().unwrap();
        let first =
            SqliteWriterGate::for_path(&temp.path().join("same.cditor.db"), Duration::from_secs(1))
                .unwrap();
        let second = SqliteWriterGate::for_path(
            &temp.path().join(".").join("same.cditor.db"),
            Duration::from_secs(2),
        )
        .unwrap();
        assert!(Arc::ptr_eq(&first.lock, &second.lock));
        assert_eq!(first.busy_timeout, Duration::from_secs(1));
        assert_eq!(second.busy_timeout, Duration::from_secs(2));
    }

    #[tokio::test]
    async fn writer_wait_timeout_reports_busy_duration() {
        let temp = TempDir::new().unwrap();
        let gate = SqliteWriterGate::for_path(
            &temp.path().join("busy.cditor.db"),
            Duration::from_millis(1),
        )
        .unwrap();
        let _guard = gate.acquire().await.unwrap();
        let error = gate.acquire().await.unwrap_err();
        assert!(matches!(
            error,
            StorageError::Busy { waited } if waited == Duration::from_millis(1)
        ));
    }
}
