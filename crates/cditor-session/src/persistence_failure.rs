use std::fmt;

use cditor_storage::StorageError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersistenceFailureKind {
    Busy,
    CapacityExhausted,
    PermissionDenied,
    Corruption,
    Timeout,
    Io,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistenceFailure {
    pub kind: PersistenceFailureKind,
    pub message: String,
}

impl PersistenceFailure {
    pub fn new(kind: PersistenceFailureKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn with_context(mut self, context: &str) -> Self {
        self.message = format!("{context}: {}", self.message);
        self
    }

    pub const fn retryable(&self) -> bool {
        matches!(
            self.kind,
            PersistenceFailureKind::Busy
                | PersistenceFailureKind::CapacityExhausted
                | PersistenceFailureKind::PermissionDenied
                | PersistenceFailureKind::Timeout
                | PersistenceFailureKind::Io
                | PersistenceFailureKind::Other
        )
    }

    pub const fn requires_recovery_export(&self) -> bool {
        true
    }
}

impl From<StorageError> for PersistenceFailure {
    fn from(error: StorageError) -> Self {
        let kind = match &error {
            StorageError::Busy { .. } => PersistenceFailureKind::Busy,
            StorageError::CapacityExhausted(_) => PersistenceFailureKind::CapacityExhausted,
            StorageError::PermissionDenied(_) => PersistenceFailureKind::PermissionDenied,
            StorageError::CorruptData(_) => PersistenceFailureKind::Corruption,
            StorageError::Timeout { .. } => PersistenceFailureKind::Timeout,
            StorageError::Io(_) => PersistenceFailureKind::Io,
            _ => PersistenceFailureKind::Other,
        };
        Self::new(kind, error.to_string())
    }
}

impl fmt::Display for PersistenceFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for PersistenceFailure {}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn storage_failures_keep_a_stable_kind_across_the_session_boundary() {
        let cases = [
            (
                StorageError::Busy {
                    waited: Duration::from_millis(50),
                },
                PersistenceFailureKind::Busy,
            ),
            (
                StorageError::CapacityExhausted("disk full".to_owned()),
                PersistenceFailureKind::CapacityExhausted,
            ),
            (
                StorageError::PermissionDenied("readonly".to_owned()),
                PersistenceFailureKind::PermissionDenied,
            ),
            (
                StorageError::CorruptData("bad page".to_owned()),
                PersistenceFailureKind::Corruption,
            ),
            (
                StorageError::Timeout {
                    operation: "save",
                    timeout: Duration::from_secs(1),
                },
                PersistenceFailureKind::Timeout,
            ),
            (
                StorageError::Io("offline volume".to_owned()),
                PersistenceFailureKind::Io,
            ),
        ];
        for (error, expected) in cases {
            let failure = PersistenceFailure::from(error);
            assert_eq!(failure.kind, expected);
            assert!(!failure.message.is_empty());
            assert!(failure.requires_recovery_export());
        }
    }

    #[test]
    fn corruption_is_not_blindly_retried() {
        let failure = PersistenceFailure::from(StorageError::CorruptData("bad page".to_owned()));
        assert!(!failure.retryable());
        assert!(
            failure
                .with_context("commit failed")
                .message
                .starts_with("commit failed:")
        );
    }
}
