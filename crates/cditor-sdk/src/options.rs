use std::{fmt, sync::Arc, time::Duration};

use cditor_core::ids::DocumentId;
use cditor_storage::DocumentStorage;

#[derive(Clone)]
pub enum CditorDocumentSource {
    Memory,
    Demo,
    LargeDemo,
    Storage(Arc<dyn DocumentStorage>),
}

impl PartialEq for CditorDocumentSource {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Memory, Self::Memory)
            | (Self::Demo, Self::Demo)
            | (Self::LargeDemo, Self::LargeDemo) => true,
            (Self::Storage(a), Self::Storage(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}

impl Eq for CditorDocumentSource {}

impl fmt::Debug for CditorDocumentSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Memory => formatter.write_str("Memory"),
            Self::Demo => formatter.write_str("Demo"),
            Self::LargeDemo => formatter.write_str("LargeDemo"),
            Self::Storage(storage) => formatter
                .debug_tuple("Storage")
                .field(&storage.backend_kind())
                .finish(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CditorOptions {
    pub document_id: Option<DocumentId>,
    pub source: CditorDocumentSource,
    pub readonly: bool,
    pub debug_overlay: bool,
    pub payload_window_size: usize,
    pub autosave_interval: Option<Duration>,
    pub storage_load_timeout: Duration,
    pub embedded_composer: bool,
}

impl Default for CditorOptions {
    fn default() -> Self {
        Self {
            document_id: None,
            source: CditorDocumentSource::Memory,
            readonly: false,
            debug_overlay: false,
            payload_window_size: 128,
            autosave_interval: Some(Duration::from_millis(250)),
            storage_load_timeout: Duration::from_secs(90),
            embedded_composer: false,
        }
    }
}
