#[derive(Debug, Clone, PartialEq)]
pub struct CditorDiagnostics {
    pub storage_backend: Option<cditor_storage::StorageBackendKind>,
    pub document_blocks: usize,
    pub loaded_payloads: usize,
    pub rendered_blocks: usize,
    pub pending_layout_tasks: usize,
    pub pending_saves: usize,
    pub dirty_blocks: usize,
    pub estimated_document_height: f64,
    pub memory_estimate_bytes: u64,
}
