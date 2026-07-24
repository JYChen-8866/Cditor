use std::collections::HashSet;

use cditor_core::ids::{BlockId, SurfaceId};

use crate::features::code::highlight::CodeHighlightCache;
use crate::features::mermaid::MermaidRenderCache;
use crate::features::whiteboard::WhiteboardThumbnailCache;
use crate::surfaces::table_cell::TableCellLayoutKey;

use super::{PlatformLayoutCache, auxiliary_layout_cache, block_layout_cache, table_layout_cache};

pub(crate) struct RenderCacheState {
    pub(crate) text_layouts: PlatformLayoutCache<BlockId>,
    pub(crate) table_cell_layouts: PlatformLayoutCache<TableCellLayoutKey>,
    pub(crate) text_surface_layouts: PlatformLayoutCache<SurfaceId>,
    pub(crate) code_highlights: CodeHighlightCache,
    pub(crate) mermaid_renders: MermaidRenderCache,
    pub(crate) mermaid_source_blocks: HashSet<BlockId>,
    pub(crate) whiteboard_thumbnails: WhiteboardThumbnailCache,
}

impl Default for RenderCacheState {
    fn default() -> Self {
        Self {
            text_layouts: block_layout_cache(),
            table_cell_layouts: table_layout_cache(),
            text_surface_layouts: auxiliary_layout_cache(),
            code_highlights: Default::default(),
            mermaid_renders: Default::default(),
            mermaid_source_blocks: Default::default(),
            whiteboard_thumbnails: Default::default(),
        }
    }
}

impl RenderCacheState {
    pub(crate) fn reset_session(&mut self) {
        self.text_layouts.clear();
        self.table_cell_layouts.clear();
        self.text_surface_layouts.clear();
        self.code_highlights.clear();
        self.mermaid_renders.clear();
        self.mermaid_source_blocks.clear();
        self.whiteboard_thumbnails.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_reset_discards_every_document_bound_render_cache() {
        let mut cache = RenderCacheState::default();
        cache.mermaid_source_blocks.insert(17);

        cache.reset_session();

        assert!(cache.text_layouts.is_empty());
        assert!(cache.table_cell_layouts.is_empty());
        assert!(cache.text_surface_layouts.is_empty());
        assert_eq!(cache.text_layouts.estimated_bytes(), 0);
        assert_eq!(cache.table_cell_layouts.estimated_bytes(), 0);
        assert_eq!(cache.text_surface_layouts.estimated_bytes(), 0);
        assert!(cache.mermaid_source_blocks.is_empty());
    }
}
