/// Reclaim intensity for editor-owned and process-wide render-derived caches.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CditorMemoryPressure {
    #[default]
    Normal,
    Warning,
    Critical,
}

/// Result of trimming process-wide editor render caches. The document model,
/// selection, IME state and undo history are intentionally absent: this report
/// only describes resources that can be reconstructed from durable state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CditorMemoryTrimReport {
    pub image_entries_evicted: usize,
    pub image_bytes_evicted: usize,
    pub exact_raster_entries_evicted: usize,
    pub exact_raster_bytes_evicted: usize,
    pub invalidated_image_loads: usize,
    pub retired_images: usize,
}

/// Result of trimming reconstructible resources owned by one editor view.
///
/// Process-wide image, glyph and text-layout caches are deliberately excluded;
/// a host with multiple editor views must trim those once through
/// `trim_process_reconstructible_caches`, then call the per-view entry for each
/// resident editor.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CditorViewMemoryTrimReport {
    pub protected_blocks: usize,
    pub mermaid_entries_evicted: usize,
    pub video_entries_evicted: usize,
    pub platform_geometry_entries_evicted: usize,
    pub retired_render_resources: usize,
    /// The session was synchronously busy, so no view-local cache was trimmed.
    /// Skipping is fail-closed: selection and composition endpoints may be
    /// outside the presented render window and must never be guessed.
    pub skipped_runtime_busy: bool,
}
