use std::time::{SystemTime, UNIX_EPOCH};

use sqlx::Row;

use cditor_storage::{StorageError, StorageResult};

use crate::error::sqlite_error;

pub(crate) fn unix_millis() -> StorageResult<i64> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| StorageError::Io(error.to_string()))?
        .as_millis();
    i64::try_from(millis).map_err(|_| StorageError::CorruptData("system time overflow".into()))
}

pub(crate) fn sort_key(index: usize) -> String {
    format!("{index:020}")
}

pub(crate) fn checked_i64(value: u64) -> StorageResult<i64> {
    i64::try_from(value).map_err(|_| StorageError::VersionOutOfRange { value })
}

pub(crate) fn checked_u64(value: i64, field: &str) -> StorageResult<u64> {
    u64::try_from(value)
        .map_err(|_| StorageError::CorruptData(format!("{field} is negative: {value}")))
}

pub(crate) fn checked_u32(value: i64, field: &str) -> StorageResult<u32> {
    u32::try_from(value)
        .map_err(|_| StorageError::CorruptData(format!("{field} is out of range: {value}")))
}

pub(crate) fn checked_u16(value: i64, field: &str) -> StorageResult<u16> {
    u16::try_from(value)
        .map_err(|_| StorageError::CorruptData(format!("{field} is out of range: {value}")))
}

pub(crate) fn row_version(row: &sqlx::sqlite::SqliteRow, field: &str) -> StorageResult<u64> {
    checked_u64(row.try_get(field).map_err(sqlite_error)?, field)
}
