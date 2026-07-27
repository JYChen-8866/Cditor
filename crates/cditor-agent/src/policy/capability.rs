use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentCapability {
    ReadDocumentMetadata,
    ReadBlockContent,
    SearchLocalContent,
    WriteBlockContent,
    ChangeDocumentStructure,
    DeleteContent,
    SendDataExternally,
    IncurExternalCost,
}

impl AgentCapability {
    pub const fn is_high_risk(self) -> bool {
        matches!(
            self,
            Self::DeleteContent | Self::ChangeDocumentStructure | Self::SendDataExternally
        )
    }
    pub const fn is_read_only(self) -> bool {
        matches!(
            self,
            Self::ReadDocumentMetadata | Self::ReadBlockContent | Self::SearchLocalContent
        )
    }
    pub const fn is_write(self) -> bool {
        matches!(
            self,
            Self::WriteBlockContent | Self::ChangeDocumentStructure | Self::DeleteContent
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCapabilityPolicy {
    pub require_confirmation: Vec<AgentCapability>,
    pub allow_session_scope: Vec<AgentCapability>,
}

impl Default for AgentCapabilityPolicy {
    fn default() -> Self {
        Self {
            require_confirmation: vec![
                AgentCapability::WriteBlockContent,
                AgentCapability::ChangeDocumentStructure,
                AgentCapability::DeleteContent,
                AgentCapability::SendDataExternally,
                AgentCapability::IncurExternalCost,
            ],
            allow_session_scope: vec![AgentCapability::WriteBlockContent],
        }
    }
}
impl AgentCapabilityPolicy {
    pub fn needs_confirmation(&self, cap: AgentCapability) -> bool {
        self.require_confirmation.contains(&cap)
    }
    pub fn can_always_allow(&self, cap: AgentCapability) -> bool {
        !cap.is_high_risk() && self.allow_session_scope.contains(&cap)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn read_ops_bypass_confirmation() {
        let p = AgentCapabilityPolicy::default();
        assert!(!p.needs_confirmation(AgentCapability::ReadDocumentMetadata));
    }
    #[test]
    fn write_ops_need_confirmation() {
        let p = AgentCapabilityPolicy::default();
        assert!(p.needs_confirmation(AgentCapability::WriteBlockContent));
    }
    #[test]
    fn high_risk_not_always_allow() {
        let p = AgentCapabilityPolicy::default();
        assert!(!p.can_always_allow(AgentCapability::DeleteContent));
    }
    #[test]
    fn read_only_check() {
        assert!(AgentCapability::ReadBlockContent.is_read_only());
        assert!(!AgentCapability::DeleteContent.is_read_only());
    }
}
