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
    pub video: VideoDiagnostics,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VideoDiagnostics {
    pub tracked_blocks: usize,
    pub loading_sessions: usize,
    pub deferred_sessions: usize,
    pub ready_sessions: usize,
    pub playing_sessions: usize,
    pub failed_sessions: usize,
    pub render_images: usize,
    pub resident_cpu_frame_bytes: usize,
    pub resident_render_image_bytes: usize,
    /// Process-wide conservative decoder reservation shared by all editors.
    pub reserved_decoder_bytes: usize,
    pub decoder_budget_bytes: usize,
    pub max_active_sessions_per_editor: usize,
}
