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
    /// Conservative resident-memory estimate for this editor plus the shared
    /// process caches it uses. This is the saturating sum of
    /// `owned_memory_estimate_bytes` and `shared_memory_estimate_bytes`.
    /// Capacity reservations are reported separately and are intentionally
    /// excluded from this total.
    pub memory_estimate_bytes: u64,
    /// Memory owned by this editor runtime and expected to become reclaimable
    /// when its live entity is hibernated.
    pub owned_memory_estimate_bytes: u64,
    /// Process/thread caches observed through this editor but shared with
    /// other editors. An application aggregating several editor diagnostics
    /// must count this value once, not once per editor.
    pub shared_memory_estimate_bytes: u64,
    pub exact_raster: ExactRasterDiagnostics,
    pub images: ImageCacheDiagnostics,
    pub mermaid: MermaidDiagnostics,
    pub video: VideoDiagnostics,
}

/// Process-thread-local fallback glyph raster cache used when GPUI cannot
/// resolve a text run through the native font bridge. Each entry owns a
/// `RenderImage`, so both entry count and resident pixel bytes matter.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExactRasterDiagnostics {
    pub entries: usize,
    pub resident_image_bytes: usize,
    pub max_entries: usize,
    pub image_byte_budget: usize,
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
}

/// Process-wide decoded raster cache used by document images and media
/// posters. `tracked_entries` includes loading and failed keys; only
/// `decoded_entries` contribute to `resident_decoded_bytes`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImageCacheDiagnostics {
    pub tracked_entries: usize,
    pub decoded_entries: usize,
    pub loading_entries: usize,
    pub failed_entries: usize,
    pub resident_decoded_bytes: usize,
    pub max_entries: usize,
    pub decoded_byte_budget: usize,
}

/// Per-editor Mermaid raster cache. In-flight renders reserve their maximum
/// possible output before admission, but those bytes are not resident yet.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MermaidDiagnostics {
    pub tracked_entries: usize,
    pub ready_entries: usize,
    pub rendering_entries: usize,
    pub failed_entries: usize,
    pub resident_image_bytes: usize,
    pub reserved_render_bytes: usize,
    pub max_entries: usize,
    pub render_byte_budget: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VideoDiagnostics {
    pub tracked_blocks: usize,
    pub loading_sessions: usize,
    pub deferred_sessions: usize,
    pub ready_sessions: usize,
    pub playing_sessions: usize,
    pub failed_sessions: usize,
    /// Stable streaming-image identities currently owned by playback entries.
    pub dynamic_images: usize,
    /// Maximum backend upload slots for those identities. Supporting backends
    /// use at most two completion-tracked slots per dynamic image; fallback
    /// backends may use none.
    pub stable_gpu_slot_capacity: usize,
    /// Current immutable CPU frames. Kept for compatibility with existing
    /// diagnostics consumers; this normally equals `dynamic_images`.
    pub render_images: usize,
    pub resident_cpu_frame_bytes: usize,
    pub resident_render_image_bytes: usize,
    /// Process-wide conservative decoder reservation shared by all editors.
    pub reserved_decoder_bytes: usize,
    pub decoder_budget_bytes: usize,
    pub max_active_sessions_per_editor: usize,
}
