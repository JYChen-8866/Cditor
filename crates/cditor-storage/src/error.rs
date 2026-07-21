use std::fmt;
use std::time::Duration;

use crate::backend::StorageBackendKind;

pub type StorageResult<T> = Result<T, StorageError>;

#[derive(Debug)]
pub enum StorageError {
    InvalidConfiguration(String),
    Migration {
        backend: StorageBackendKind,
        message: String,
    },
    NotFound {
        entity: &'static str,
        id: String,
    },
    CorruptData(String),
    Serialization(String),
    VersionOutOfRange {
        value: u64,
    },
    Busy {
        waited: Duration,
    },
    Timeout {
        operation: &'static str,
        timeout: Duration,
    },
    Conflict(String),
    Io(String),
    Backend {
        backend: StorageBackendKind,
        message: String,
    },
}

impl fmt::Display for StorageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfiguration(message) => {
                write!(formatter, "invalid storage configuration: {message}")
            }
            Self::Migration { backend, message } => {
                write!(formatter, "{backend} migration failed: {message}")
            }
            Self::NotFound { entity, id } => write!(formatter, "{entity} not found: {id}"),
            Self::CorruptData(message) => write!(formatter, "corrupt storage data: {message}"),
            Self::Serialization(message) => write!(formatter, "serialization failed: {message}"),
            Self::VersionOutOfRange { value } => {
                write!(formatter, "version {value} exceeds backend range")
            }
            Self::Busy { waited } => write!(
                formatter,
                "storage remained busy for {:.1} seconds",
                waited.as_secs_f64()
            ),
            Self::Timeout { operation, timeout } => write!(
                formatter,
                "{operation} timed out after {:.1} seconds",
                timeout.as_secs_f64()
            ),
            Self::Conflict(message) => write!(formatter, "storage conflict: {message}"),
            Self::Io(message) => write!(formatter, "storage I/O failed: {message}"),
            Self::Backend { backend, message } => {
                write!(formatter, "{backend} storage failed: {message}")
            }
        }
    }
}

impl std::error::Error for StorageError {}
