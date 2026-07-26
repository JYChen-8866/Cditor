use cditor_storage::{StorageBackendKind, StorageError};

pub(crate) fn sqlite_error(error: sqlx::Error) -> StorageError {
    if let sqlx::Error::Database(database) = &error {
        if database.is_unique_violation() || database.is_foreign_key_violation() {
            return StorageError::Conflict(database.message().to_owned());
        }
        if let Some(error) = classify_sqlite_failure(database.code().as_deref(), database.message())
        {
            return error;
        }
    }
    StorageError::Backend {
        backend: StorageBackendKind::Sqlite,
        message: error.to_string(),
    }
}

fn classify_sqlite_failure(code: Option<&str>, message: &str) -> Option<StorageError> {
    let primary_code = code
        .and_then(|code| code.parse::<u32>().ok())
        .map(|code| code & 0xff);
    match primary_code {
        Some(5 | 6) => Some(StorageError::Busy {
            waited: std::time::Duration::ZERO,
        }),
        Some(13) => Some(StorageError::CapacityExhausted(message.to_owned())),
        Some(3 | 8) => Some(StorageError::PermissionDenied(message.to_owned())),
        Some(11 | 26) => Some(StorageError::CorruptData(message.to_owned())),
        _ if message.contains("database is locked") || message.contains("database is busy") => {
            Some(StorageError::Busy {
                waited: std::time::Duration::ZERO,
            })
        }
        _ if message.contains("database or disk is full") => {
            Some(StorageError::CapacityExhausted(message.to_owned()))
        }
        _ if message.contains("readonly database") || message.contains("permission denied") => {
            Some(StorageError::PermissionDenied(message.to_owned()))
        }
        _ if message.contains("database disk image is malformed")
            || message.contains("file is not a database") =>
        {
            Some(StorageError::CorruptData(message.to_owned()))
        }
        _ => None,
    }
}

pub(crate) fn serialization_error(error: serde_json::Error) -> StorageError {
    StorageError::Serialization(error.to_string())
}

pub(crate) fn corrupt_json(field: &str, error: serde_json::Error) -> StorageError {
    StorageError::CorruptData(format!("invalid persisted {field} JSON: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sqlite_primary_and_extended_codes_map_to_stable_storage_errors() {
        assert!(matches!(
            classify_sqlite_failure(Some("5"), "busy"),
            Some(StorageError::Busy { .. })
        ));
        assert!(matches!(
            classify_sqlite_failure(Some("517"), "busy snapshot"),
            Some(StorageError::Busy { .. })
        ));
        assert!(matches!(
            classify_sqlite_failure(Some("13"), "full"),
            Some(StorageError::CapacityExhausted(_))
        ));
        assert!(matches!(
            classify_sqlite_failure(Some("8"), "readonly"),
            Some(StorageError::PermissionDenied(_))
        ));
        assert!(matches!(
            classify_sqlite_failure(Some("267"), "corrupt virtual table"),
            Some(StorageError::CorruptData(_))
        ));
    }

    #[test]
    fn sqlite_message_fallback_covers_errors_without_numeric_codes() {
        assert!(matches!(
            classify_sqlite_failure(None, "database or disk is full"),
            Some(StorageError::CapacityExhausted(_))
        ));
        assert!(matches!(
            classify_sqlite_failure(None, "attempt to write a readonly database"),
            Some(StorageError::PermissionDenied(_))
        ));
        assert!(matches!(
            classify_sqlite_failure(None, "database disk image is malformed"),
            Some(StorageError::CorruptData(_))
        ));
    }
}
