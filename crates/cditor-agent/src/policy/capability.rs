use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum AgentCapability {
    ReadBlocks,
    ReadDocumentStructure,
    ReadDocumentStats,
    ReadFullContent,
    InsertBlocks,
    UpdateBlocks,
    DeleteBlocks,
    MoveBlocks,
    FormatBlocks,
    Search,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentCapabilityPolicy {
    pub always_allow: BTreeSet<AgentCapability>,
    pub confirm_each: BTreeSet<AgentCapability>,
    pub deny: BTreeSet<AgentCapability>,
}

impl Default for AgentCapabilityPolicy {
    fn default() -> Self {
        let mut always_allow = BTreeSet::new();
        always_allow.insert(AgentCapability::ReadBlocks);
        always_allow.insert(AgentCapability::ReadDocumentStructure);
        always_allow.insert(AgentCapability::ReadDocumentStats);

        let mut confirm_each = BTreeSet::new();
        confirm_each.insert(AgentCapability::InsertBlocks);
        confirm_each.insert(AgentCapability::UpdateBlocks);
        confirm_each.insert(AgentCapability::DeleteBlocks);
        confirm_each.insert(AgentCapability::MoveBlocks);
        confirm_each.insert(AgentCapability::FormatBlocks);
        confirm_each.insert(AgentCapability::ReadFullContent);
        confirm_each.insert(AgentCapability::Search);

        Self {
            always_allow,
            confirm_each,
            deny: BTreeSet::new(),
        }
    }
}

impl AgentCapabilityPolicy {
    pub fn allows(&self, capability: AgentCapability) -> bool {
        !self.deny.contains(&capability)
    }

    pub fn requires_confirmation(&self, capability: AgentCapability) -> bool {
        self.confirm_each.contains(&capability)
    }

    pub fn is_auto_allowed(&self, capability: AgentCapability) -> bool {
        self.always_allow.contains(&capability)
    }
}
