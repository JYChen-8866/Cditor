use std::{collections::HashSet, sync::Arc};

use cditor_core::ids::{BlockId, SurfaceId};
#[cfg(feature = "gpui-dynamic-image")]
use gpui::DynamicImage;
use gpui::RenderImage;

use crate::app::text_layout_prewarm::{TextLayoutPrewarmGenerationState, TextLayoutPrewarmKey};
use crate::features::code::highlight::CodeHighlightCache;
use crate::features::mermaid::MermaidRenderCache;
use crate::features::video::VideoPlaybackCache;
use crate::features::whiteboard::WhiteboardThumbnailCache;
use crate::surfaces::table_cell::TableCellLayoutKey;

use super::{
    PlatformGeometryRegistry, auxiliary_geometry_registry, block_geometry_registry,
    table_geometry_registry,
};

#[derive(Default)]
pub(crate) struct RenderCacheTrimResult {
    pub(crate) retired: RetiredRenderResources,
    pub(crate) mermaid_evicted_entries: usize,
    pub(crate) video_evicted_entries: usize,
    pub(crate) platform_geometry_evicted_entries: usize,
}

pub(crate) struct RenderCacheState {
    pub(crate) text_layouts: PlatformGeometryRegistry<BlockId>,
    pub(crate) table_cell_layouts: PlatformGeometryRegistry<TableCellLayoutKey>,
    pub(crate) text_surface_layouts: PlatformGeometryRegistry<SurfaceId>,
    pub(crate) code_highlights: CodeHighlightCache,
    pub(crate) mermaid_renders: MermaidRenderCache,
    pub(crate) mermaid_source_blocks: HashSet<BlockId>,
    pub(crate) video_playbacks: VideoPlaybackCache,
    pub(crate) whiteboard_thumbnails: WhiteboardThumbnailCache,
    pub(crate) pending_text_layout_prewarms: HashSet<TextLayoutPrewarmKey>,
    pub(crate) text_layout_prewarm_generations: TextLayoutPrewarmGenerationState,
}

#[derive(Default)]
pub(crate) struct RetiredRenderResources {
    pub(crate) images: Vec<Arc<RenderImage>>,
    #[cfg(feature = "gpui-dynamic-image")]
    pub(crate) dynamic_images: Vec<Arc<DynamicImage>>,
}

impl Default for RenderCacheState {
    fn default() -> Self {
        Self {
            text_layouts: block_geometry_registry(),
            table_cell_layouts: table_geometry_registry(),
            text_surface_layouts: auxiliary_geometry_registry(),
            code_highlights: Default::default(),
            mermaid_renders: Default::default(),
            mermaid_source_blocks: Default::default(),
            video_playbacks: Default::default(),
            whiteboard_thumbnails: Default::default(),
            pending_text_layout_prewarms: Default::default(),
            text_layout_prewarm_generations: Default::default(),
        }
    }
}

impl RenderCacheState {
    /// Trims only rebuildable render state. `protected_blocks` is the current
    /// interaction/viewport pin set; it is deliberately passed in by the view
    /// so a pressure event cannot invalidate a visible caret, selection or
    /// stable block frame.
    pub(crate) fn apply_memory_pressure(
        &mut self,
        pressure: crate::memory_pressure::CditorMemoryPressure,
        protected_blocks: &HashSet<BlockId>,
    ) -> RenderCacheTrimResult {
        let mut result = RenderCacheTrimResult::default();

        let mermaid = self
            .mermaid_renders
            .apply_memory_pressure(pressure, protected_blocks);
        result.mermaid_evicted_entries = mermaid.evicted_entries;
        result.retired.images.extend(mermaid.retired_images);

        let video_entries_before = self.video_playbacks.diagnostics().tracked_blocks;
        let retired_videos = self
            .video_playbacks
            .apply_memory_pressure(pressure, protected_blocks);
        result.video_evicted_entries =
            video_entries_before.saturating_sub(self.video_playbacks.diagnostics().tracked_blocks);
        #[cfg(feature = "gpui-dynamic-image")]
        for video in retired_videos {
            let (dynamic, fallback) = video.into_parts();
            result.retired.dynamic_images.push(dynamic);
            result.retired.images.push(fallback);
        }
        #[cfg(not(feature = "gpui-dynamic-image"))]
        result
            .retired
            .images
            .extend(retired_videos.into_iter().map(|video| video.into_parts()));

        // Platform layouts own shaped text and accessibility snapshots. At a
        // warning retain the newest half of unpinned entries; at critical keep
        // only visible/interactive block surfaces and ephemeral UI surfaces.
        if !matches!(
            pressure,
            crate::memory_pressure::CditorMemoryPressure::Normal
        ) {
            let target = |entries: usize| match pressure {
                crate::memory_pressure::CditorMemoryPressure::Warning => entries / 2,
                crate::memory_pressure::CditorMemoryPressure::Critical => 0,
                crate::memory_pressure::CditorMemoryPressure::Normal => entries,
            };
            let text_unpinned = self
                .text_layouts
                .values()
                .filter(|layout| !protected_blocks.contains(&layout.block_id))
                .count();
            result.platform_geometry_evicted_entries += self
                .text_layouts
                .trim_to_recent(target(text_unpinned), |_, layout| {
                    protected_blocks.contains(&layout.block_id)
                });
            let table_unpinned = self
                .table_cell_layouts
                .keys()
                .filter(|key| !protected_blocks.contains(&key.block_id))
                .count();
            result.platform_geometry_evicted_entries += self
                .table_cell_layouts
                .trim_to_recent(target(table_unpinned), |key, _| {
                    protected_blocks.contains(&key.block_id)
                });
            let surface_unpinned = self
                .text_surface_layouts
                .values()
                .filter(|layout| {
                    layout
                        .surface_id
                        .block_id()
                        .is_some_and(|id| !protected_blocks.contains(&id))
                })
                .count();
            result.platform_geometry_evicted_entries +=
                self.text_surface_layouts
                    .trim_to_recent(target(surface_unpinned), |_, layout| {
                        layout
                            .surface_id
                            .block_id()
                            .is_none_or(|id| protected_blocks.contains(&id))
                    });
        }

        if matches!(
            pressure,
            crate::memory_pressure::CditorMemoryPressure::Critical
        ) {
            self.code_highlights.clear();
            self.whiteboard_thumbnails.clear();
        }
        result
    }

    pub(crate) fn reset_session(&mut self) -> RetiredRenderResources {
        self.text_layouts.clear();
        self.table_cell_layouts.clear();
        self.text_surface_layouts.clear();
        self.code_highlights.clear();
        let mut retired = RetiredRenderResources {
            images: self.mermaid_renders.clear(),
            ..Default::default()
        };
        self.mermaid_source_blocks.clear();
        #[cfg(feature = "gpui-dynamic-image")]
        for video in self.video_playbacks.clear() {
            let (dynamic, fallback) = video.into_parts();
            retired.dynamic_images.push(dynamic);
            retired.images.push(fallback);
        }
        #[cfg(not(feature = "gpui-dynamic-image"))]
        retired.images.extend(
            self.video_playbacks
                .clear()
                .into_iter()
                .map(|video| video.into_parts()),
        );
        self.whiteboard_thumbnails.clear();
        self.pending_text_layout_prewarms.clear();
        self.text_layout_prewarm_generations.clear();
        retired
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_reset_discards_every_document_bound_render_cache() {
        let mut cache = RenderCacheState::default();
        cache.mermaid_source_blocks.insert(17);

        let retired = cache.reset_session();

        assert!(retired.images.is_empty());
        assert!(cache.text_layouts.is_empty());
        assert!(cache.table_cell_layouts.is_empty());
        assert!(cache.text_surface_layouts.is_empty());
        assert_eq!(cache.text_layouts.estimated_metadata_bytes(), 0);
        assert_eq!(cache.table_cell_layouts.estimated_metadata_bytes(), 0);
        assert_eq!(cache.text_surface_layouts.estimated_metadata_bytes(), 0);
        assert!(cache.mermaid_source_blocks.is_empty());
    }
}
