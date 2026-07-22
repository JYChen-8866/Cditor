use serde::{Deserialize, Serialize};

pub const CURRENT_PROTOCOL_VERSION: ProtocolVersion = ProtocolVersion::new(1, 0);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ProtocolVersion {
    pub major: u16,
    pub minor: u16,
}

impl ProtocolVersion {
    pub const fn new(major: u16, minor: u16) -> Self {
        Self { major, minor }
    }

    pub const fn is_compatible_with(self, supported: Self) -> bool {
        self.major == supported.major && self.minor <= supported.minor
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compatibility_requires_same_major_and_supported_minor() {
        assert!(ProtocolVersion::new(1, 0).is_compatible_with(ProtocolVersion::new(1, 2)));
        assert!(!ProtocolVersion::new(1, 3).is_compatible_with(ProtocolVersion::new(1, 2)));
        assert!(!ProtocolVersion::new(2, 0).is_compatible_with(ProtocolVersion::new(1, 9)));
    }
}
