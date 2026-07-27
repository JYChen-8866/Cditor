use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedactionPolicy {
    pub redact_api_keys: bool,
    pub redact_file_paths: bool,
    pub redact_email_addresses: bool,
    pub max_log_content_bytes: usize,
    pub log_ids_only: bool,
}

impl Default for RedactionPolicy {
    fn default() -> Self {
        Self {
            redact_api_keys: true,
            redact_file_paths: false,
            redact_email_addresses: true,
            max_log_content_bytes: 256,
            log_ids_only: true,
        }
    }
}
