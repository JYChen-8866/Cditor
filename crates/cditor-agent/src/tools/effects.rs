use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolEffects {
    Pure,
    LocalRead,
    LocalReadWithEgress,
    LocalWrite,
    LocalWriteWithEgress,
    ExternalCost,
}

impl ToolEffects {
    pub const fn is_write(self) -> bool {
        matches!(
            self,
            Self::LocalWrite | Self::LocalWriteWithEgress | Self::ExternalCost
        )
    }
}
#[cfg(test)]
mod t {
    use super::*;
    #[test]
    fn effects_is_write() {
        assert!(!ToolEffects::LocalRead.is_write());
        assert!(ToolEffects::LocalWrite.is_write());
    }
}
