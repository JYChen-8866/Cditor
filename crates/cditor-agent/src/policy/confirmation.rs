use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConfirmationPolicy {
    None,
    ConfirmEach,
    ConfirmBatch,
    ConfirmAfterPreview,
}

impl Default for ConfirmationPolicy {
    fn default() -> Self {
        Self::ConfirmEach
    }
}
