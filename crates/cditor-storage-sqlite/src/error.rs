use cditor_storage::{StorageBackendKind, StorageError};

pub(crate) fn sqlite_error(error: sqlx::Error) -> StorageError {
    if let sqlx::Error::Database(database) = &error {
        if database.is_unique_violation() || database.is_foreign_key_violation() {
            return StorageError::Conflict(database.message().to_owned());
        }
        if database.message().contains("database is locked") {
            return StorageError::Busy {
                waited: std::time::Duration::ZERO,
            };
        }
    }
    StorageError::Backend {
        backend: StorageBackendKind::Sqlite,
        message: error.to_string(),
    }
}

pub(crate) fn serialization_error(error: serde_json::Error) -> StorageError {
    StorageError::Serialization(error.to_string())
}
