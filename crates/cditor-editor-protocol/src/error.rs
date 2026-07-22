use std::fmt;

use cditor_core::ids::{BlockId, DocumentId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolErrorCode {
    UnsupportedVersion,
    UnknownCommand,
    InvalidArguments,
    NotReady,
    Readonly,
    DocumentNotFound,
    BlockNotFound,
    InvalidSelection,
    PermissionDenied,
    StalePrecondition,
    CompositionConflict,
    ApplyFailed,
    Cancelled,
    Timeout,
    Busy,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolError {
    pub code: ProtocolErrorCode,
    pub message: String,
    pub document_id: Option<DocumentId>,
    pub block_id: Option<BlockId>,
    pub retryable: bool,
}

impl ProtocolError {
    pub fn new(code: ProtocolErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            document_id: None,
            block_id: None,
            retryable: false,
        }
    }

    pub const fn with_document(mut self, document_id: DocumentId) -> Self {
        self.document_id = Some(document_id);
        self
    }

    pub const fn with_block(mut self, block_id: BlockId) -> Self {
        self.block_id = Some(block_id);
        self
    }

    pub const fn retryable(mut self) -> Self {
        self.retryable = true;
        self
    }
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ProtocolError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_error_roundtrip_preserves_context() {
        let error = ProtocolError::new(ProtocolErrorCode::BlockNotFound, "missing block")
            .with_document(7)
            .with_block(11)
            .retryable();
        let value = serde_json::to_value(&error).unwrap();
        assert_eq!(
            serde_json::from_value::<ProtocolError>(value).unwrap(),
            error
        );
    }
}
