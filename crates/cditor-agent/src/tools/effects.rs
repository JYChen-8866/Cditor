use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolEffects {
    pub local_read: bool,
    pub local_write: bool,
    pub data_egress: bool,
    pub external_cost: bool,
}

impl ToolEffects {
    pub const READ_ONLY: Self = Self {
        local_read: true,
        local_write: false,
        data_egress: false,
        external_cost: false,
    };

    pub const WRITE: Self = Self {
        local_read: false,
        local_write: true,
        data_egress: false,
        external_cost: false,
    };

    pub fn is_pure_read(&self) -> bool {
        self.local_read && !self.local_write && !self.data_egress && !self.external_cost
    }

    pub fn requires_confirmation(&self) -> bool {
        self.local_write || self.data_egress
    }
}
