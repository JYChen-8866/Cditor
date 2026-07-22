use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostCapabilitySnapshot {
    pub clipboard_read: bool,
    pub clipboard_write: bool,
    pub file_picker: bool,
    pub external_links: bool,
    pub assets: bool,
    pub ai: bool,
    pub persistence: bool,
}

impl HostCapabilitySnapshot {
    pub const fn none() -> Self {
        Self {
            clipboard_read: false,
            clipboard_write: false,
            file_picker: false,
            external_links: false,
            assets: false,
            ai: false,
            persistence: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_default_to_denied() {
        assert_eq!(
            HostCapabilitySnapshot::default(),
            HostCapabilitySnapshot::none()
        );
    }
}
