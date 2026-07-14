use std::fmt;

use cditor_core::ids::{BlockId, DocumentId};

/// Stable error type returned by the component SDK.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CditorError {
    ComponentDropped,
    NotReady,
    Readonly,
    DocumentNotFound(DocumentId),
    BlockNotFound(BlockId),
    InvalidSelection,
    InvalidInput(String),
    Unsupported(String),
    Cancelled,
    Timeout,
    Persistence(String),
    Import(String),
    Export(String),
    Asset(String),
    Ai(String),
    Internal(String),
}

impl fmt::Display for CditorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ComponentDropped => formatter.write_str("the Cditor component was dropped"),
            Self::NotReady => formatter.write_str("the Cditor component is not ready"),
            Self::Readonly => formatter.write_str("the document is readonly"),
            Self::DocumentNotFound(id) => write!(formatter, "document {id} was not found"),
            Self::BlockNotFound(id) => write!(formatter, "block {id} was not found"),
            Self::InvalidSelection => formatter.write_str("the document selection is invalid"),
            Self::InvalidInput(message)
            | Self::Unsupported(message)
            | Self::Persistence(message)
            | Self::Import(message)
            | Self::Export(message)
            | Self::Asset(message)
            | Self::Ai(message)
            | Self::Internal(message) => formatter.write_str(message),
            Self::Cancelled => formatter.write_str("the operation was cancelled"),
            Self::Timeout => formatter.write_str("the operation timed out"),
        }
    }
}

impl std::error::Error for CditorError {}
