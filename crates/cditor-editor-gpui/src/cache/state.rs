use std::{collections::HashSet, sync::Arc};

use cditor_core::ids::{BlockId, SurfaceId};
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
    pub(crate) fn reset_session(&mut self) -> Vec<Arc<RenderImage>> {
        self.text_layouts.clear();
        self.table_cell_layouts.clear();
        self.text_surface_layouts.clear();
        self.code_highlights.clear();
        let mut retired_images = self.mermaid_renders.clear();
        self.mermaid_source_blocks.clear();
        retired_images.extend(self.video_playbacks.clear());
        self.whiteboard_thumbnails.clear();
        self.pending_text_layout_prewarms.clear();
        self.text_layout_prewarm_generations.clear();
        retired_images
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

        assert!(retired.is_empty());
        assert!(cache.text_layouts.is_empty());
        assert!(cache.table_cell_layouts.is_empty());
        assert!(cache.text_surface_layouts.is_empty());
        assert_eq!(cache.text_layouts.estimated_metadata_bytes(), 0);
        assert_eq!(cache.table_cell_layouts.estimated_metadata_bytes(), 0);
        assert_eq!(cache.text_surface_layouts.estimated_metadata_bytes(), 0);
        assert!(cache.mermaid_source_blocks.is_empty());
    }
}
