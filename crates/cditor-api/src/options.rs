use std::time::Duration;

use std::sync::Arc;

use cditor_core::ids::DocumentId;
use cditor_storage::StorageProvider;

pub type WorkspaceId = u64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CditorOptions {
    pub workspace_id: Option<WorkspaceId>,
    pub document_id: Option<DocumentId>,
    pub backend: CditorBackend,
    pub readonly: bool,
    pub debug_overlay: bool,
    pub payload_window_size: usize,
    pub autosave_interval: Option<Duration>,
}

#[derive(Clone)]
pub enum CditorBackend {
    Demo,
    LargeDemo,
    Memory,
    Persistent { provider: Arc<dyn StorageProvider> },
    Cloud { endpoint: String },
}

impl PartialEq for CditorBackend {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Demo, Self::Demo)
            | (Self::LargeDemo, Self::LargeDemo)
            | (Self::Memory, Self::Memory) => true,
            (Self::Persistent { provider: a }, Self::Persistent { provider: b }) => {
                Arc::ptr_eq(a, b)
            }
            (Self::Cloud { endpoint: a }, Self::Cloud { endpoint: b }) => a == b,
            _ => false,
        }
    }
}

impl Eq for CditorBackend {}

impl std::fmt::Debug for CditorBackend {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Demo => formatter.write_str("Demo"),
            Self::LargeDemo => formatter.write_str("LargeDemo"),
            Self::Memory => formatter.write_str("Memory"),
            Self::Persistent { provider } => formatter
                .debug_struct("Persistent")
                .field("provider", &provider.label())
                .finish(),
            Self::Cloud { endpoint } => formatter
                .debug_struct("Cloud")
                .field("endpoint", endpoint)
                .finish(),
        }
    }
}

impl Default for CditorOptions {
    fn default() -> Self {
        Self {
            workspace_id: None,
            document_id: None,
            backend: CditorBackend::Demo,
            readonly: false,
            debug_overlay: false,
            payload_window_size: 128,
            autosave_interval: Some(Duration::from_millis(250)),
        }
    }
}
