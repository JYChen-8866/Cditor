use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentErrorCode {
    NotFound,
    StaleRead,
    InvalidStructure,
    ParseError,
    LossyConversion,
    PermissionDenied,
    ConfirmationRejected,
    PreparedMutationExpired,
    ActiveComposition,
    ConfirmationRequired,
    PersistenceFailed,
    BudgetExceeded,
    Cancelled,
    Internal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BudgetDimension {
    Blocks,
    Bytes,
    Tokens,
    Deadline,
    ToolCalls,
}

#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentToolError {
    #[error("not found: {block_id}")]
    NotFound { block_id: uuid::Uuid },
    #[error("stale read")]
    StaleRead,
    #[error("invalid structure")]
    InvalidStructure { reason: String },
    #[error("parse error")]
    ParseError {
        line: u32,
        column: u32,
        message: String,
    },
    #[error("lossy")]
    LossyConversionRequiresApproval { warnings: Vec<String> },
    #[error("permission denied")]
    PermissionDenied { capability: String },
    #[error("confirmation rejected")]
    ConfirmationRejected,
    #[error("expired")]
    PreparedMutationExpired,
    #[error("composition active")]
    ActiveComposition { block_id: uuid::Uuid },
    #[error("persistence failed")]
    PersistenceFailed {
        transaction_id: uuid::Uuid,
        recoverable: bool,
    },
    #[error("budget: {dimension:?}")]
    BudgetExceeded { dimension: BudgetDimension },
    #[error("cancelled")]
    Cancelled,
}
#[cfg(test)]
mod t {
    use super::*;
    #[test]
    fn serde_roundtrip() {
        let e = AgentToolError::NotFound {
            block_id: uuid::Uuid::new_v4(),
        };
        let j = serde_json::to_string(&e).unwrap();
        assert!(j.contains("not_found"));
    }
    #[test]
    fn budget_dimension_values() {
        let d = BudgetDimension::Tokens;
        let j = serde_json::to_string(&d).unwrap();
        assert!(j.contains("tokens"));
    }
}
