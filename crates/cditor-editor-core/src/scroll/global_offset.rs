use std::ops::Range;

use crate::scroll::virtual_scroll::{
    BlockScrollResolver, LayoutPx, ResolvedBlockScrollTarget, ScrollPrecision,
};
use cditor_core::document::VisibleDocumentIndex;
use cditor_core::ids::BlockId;
use cditor_core::layout::{BlockHeightIndex, HeightConfidence, PageLayoutIndex};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlobalOffsetTarget {
    pub block_id: Option<BlockId>,
    pub block_index: usize,
    pub offset_in_block: LayoutPx,
    pub global_scroll_top: LayoutPx,
    pub page_index: usize,
    pub offset_in_page: LayoutPx,
    pub window_local_y: Option<LayoutPx>,
    pub content: TargetContent,
    pub precision: ScrollPrecision,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetContent {
    Loaded,
    Placeholder,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RenderWindowGeometry {
    pub page_range: Range<usize>,
    pub block_range: Range<usize>,
    pub window_start_global_y: LayoutPx,
}

impl RenderWindowGeometry {
    pub fn contains_page(&self, page_index: usize) -> bool {
        self.page_range.start <= page_index && page_index < self.page_range.end
    }

    pub fn contains_block(&self, block_index: usize) -> bool {
        self.block_range.start <= block_index && block_index < self.block_range.end
    }

    pub fn global_to_window_local(&self, global_y: LayoutPx) -> LayoutPx {
        global_y - self.window_start_global_y
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewportLocalCoordinate {
    pub window_local_y: LayoutPx,
    pub viewport_y: LayoutPx,
}

#[derive(Debug)]
pub struct GlobalOffsetMapper<'a> {
    pub visible_index: &'a VisibleDocumentIndex,
    pub block_height_index: &'a BlockHeightIndex,
    pub page_layout_index: &'a PageLayoutIndex,
    pub loaded_window: Option<RenderWindowGeometry>,
}

impl<'a> GlobalOffsetMapper<'a> {
    pub fn new(
        visible_index: &'a VisibleDocumentIndex,
        block_height_index: &'a BlockHeightIndex,
        page_layout_index: &'a PageLayoutIndex,
    ) -> Self {
        Self {
            visible_index,
            block_height_index,
            page_layout_index,
            loaded_window: None,
        }
    }

    pub fn with_loaded_window(mut self, loaded_window: RenderWindowGeometry) -> Self {
        self.loaded_window = Some(loaded_window);
        self
    }

    pub fn target_for_global_offset(&self, global_y: LayoutPx) -> Option<GlobalOffsetTarget> {
        let block_hit = self.block_height_index.block_at_offset(global_y)?;
        let page_hit = self.page_layout_index.page_at_offset(global_y)?;
        let clamped_global_y = block_hit.block_top + block_hit.offset_in_block;
        let block_id = self.visible_index.id_at_visible_index(block_hit.index);
        let content = if self
            .loaded_window
            .as_ref()
            .is_some_and(|window| window.contains_page(page_hit.page_index))
        {
            TargetContent::Loaded
        } else {
            TargetContent::Placeholder
        };
        let window_local_y = self.loaded_window.as_ref().and_then(|window| {
            if window.contains_block(block_hit.index) {
                Some(window.global_to_window_local(clamped_global_y))
            } else {
                None
            }
        });

        Some(GlobalOffsetTarget {
            block_id,
            block_index: block_hit.index,
            offset_in_block: block_hit.offset_in_block,
            global_scroll_top: clamped_global_y,
            page_index: page_hit.page_index,
            offset_in_page: page_hit.offset_in_page,
            window_local_y,
            content,
            precision: self.precision_for_block(block_hit.index),
        })
    }

    pub fn target_for_block(&self, block_id: BlockId) -> Option<GlobalOffsetTarget> {
        let block_index = self.visible_index.visible_index_of(block_id)?;
        let global_y = self.block_height_index.offset_of_block(block_index)?;
        self.target_for_global_offset(global_y)
    }

    pub fn window_local_coordinate(
        &self,
        target: &GlobalOffsetTarget,
        viewport_global_top: LayoutPx,
    ) -> Option<ViewportLocalCoordinate> {
        let window = self.loaded_window.as_ref()?;
        if !window.contains_block(target.block_index) {
            return None;
        }
        let window_local_y = window.global_to_window_local(target.global_scroll_top);
        Some(ViewportLocalCoordinate {
            window_local_y,
            viewport_y: target.global_scroll_top - viewport_global_top,
        })
    }

    fn precision_for_block(&self, block_index: usize) -> ScrollPrecision {
        match self.block_height_index.confidence.get(block_index).copied() {
            Some(HeightConfidence::Exact) => ScrollPrecision::Exact,
            Some(HeightConfidence::Predictive | HeightConfidence::Historical) => {
                ScrollPrecision::LocalExact
            }
            Some(HeightConfidence::Default) => ScrollPrecision::Estimated,
            None => ScrollPrecision::Estimated,
        }
    }
}

impl BlockScrollResolver for GlobalOffsetMapper<'_> {
    fn resolve_block_scroll_target(&self, block_id: BlockId) -> Option<ResolvedBlockScrollTarget> {
        let target = self.target_for_block(block_id)?;
        Some(ResolvedBlockScrollTarget {
            block_index: target.block_index,
            offset_in_block: target.offset_in_block,
            global_scroll_top: target.global_scroll_top,
            precision: target.precision,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cditor_core::document::{BlockIndexRecord, DocumentIndex, VisibleDocumentIndex};
    use cditor_core::ids::DocumentId;
    use cditor_core::layout::{BlockHeightIndex, HeightEstimate, PagePolicy};

    const DOCUMENT_ID: DocumentId = 1;
    const PARAGRAPH: u16 = 1;

    #[test]
    fn maps_global_y_to_visible_block_and_offset() {
        let (visible_index, height_index, page_index) = sample_indexes();
        let mapper = GlobalOffsetMapper::new(&visible_index, &height_index, &page_index);

        let target = mapper.target_for_global_offset(35.0).unwrap();

        assert_eq!(target.block_id, Some(3));
        assert_eq!(target.block_index, 2);
        assert_eq!(target.offset_in_block, 5.0);
        assert_eq!(target.global_scroll_top, 35.0);
        assert_eq!(target.page_index, 0);
    }

    #[test]
    fn returns_placeholder_when_target_page_is_not_loaded() {
        let (visible_index, height_index, page_index) = sample_indexes();
        let mapper = GlobalOffsetMapper::new(&visible_index, &height_index, &page_index)
            .with_loaded_window(RenderWindowGeometry {
                page_range: 0..1,
                block_range: 0..2,
                window_start_global_y: 0.0,
            });

        let target = mapper.target_for_global_offset(75.0).unwrap();

        assert_eq!(target.page_index, 1);
        assert_eq!(target.content, TargetContent::Placeholder);
        assert_eq!(target.window_local_y, None);
    }

    #[test]
    fn converts_global_coordinate_to_window_and_viewport_local() {
        let (visible_index, height_index, page_index) = sample_indexes();
        let mapper = GlobalOffsetMapper::new(&visible_index, &height_index, &page_index)
            .with_loaded_window(RenderWindowGeometry {
                page_range: 1..2,
                block_range: 2..5,
                window_start_global_y: 30.0,
            });

        let target = mapper.target_for_global_offset(75.0).unwrap();
        let local = mapper.window_local_coordinate(&target, 60.0).unwrap();

        assert_eq!(target.content, TargetContent::Loaded);
        assert_eq!(target.window_local_y, Some(45.0));
        assert_eq!(local.window_local_y, 45.0);
        assert_eq!(local.viewport_y, 15.0);
        assert!(local.window_local_y.abs() < 1_000.0);
    }

    #[test]
    fn keeps_ui_local_coordinates_small_for_large_total_height() {
        let visible_records: Vec<_> = (1..=100_000)
            .map(|id| BlockIndexRecord::new(id, None, 0, PARAGRAPH, 0))
            .collect();
        let document_index = DocumentIndex::new(DOCUMENT_ID, visible_records, 1).unwrap();
        let visible_index = VisibleDocumentIndex::from_document_index(&document_index);
        let height_index = BlockHeightIndex::new(
            (0..100_000).map(|_| HeightEstimate::new(200.0, HeightConfidence::Exact, 0.0)),
        )
        .unwrap();
        let page_index = PageLayoutIndex::from_block_height_index(
            &height_index,
            PagePolicy {
                max_blocks: 1_000,
                target_height: 200_000.0,
                ..PagePolicy::default()
            },
        )
        .unwrap();
        let mapper = GlobalOffsetMapper::new(&visible_index, &height_index, &page_index)
            .with_loaded_window(RenderWindowGeometry {
                page_range: 50..51,
                block_range: 50_000..51_000,
                window_start_global_y: 10_000_000.0,
            });

        let target = mapper.target_for_global_offset(10_000_123.0).unwrap();
        let local = mapper
            .window_local_coordinate(&target, 10_000_000.0)
            .unwrap();

        assert_eq!(target.block_index, 50_000);
        assert_eq!(target.offset_in_block, 123.0);
        assert_eq!(local.window_local_y, 123.0);
        assert_eq!(local.viewport_y, 123.0);
    }

    #[test]
    fn mapper_resolves_scroll_to_block_for_virtual_scroll_state() {
        let (visible_index, height_index, page_index) = sample_indexes();
        let mapper = GlobalOffsetMapper::new(&visible_index, &height_index, &page_index);

        let resolved = mapper.resolve_block_scroll_target(4).unwrap();

        assert_eq!(resolved.block_index, 3);
        assert_eq!(resolved.global_scroll_top, 60.0);
    }

    fn sample_indexes() -> (VisibleDocumentIndex, BlockHeightIndex, PageLayoutIndex) {
        let records: Vec<_> = (1..=5)
            .map(|id| BlockIndexRecord::new(id, None, 0, PARAGRAPH, 0))
            .collect();
        let document_index = DocumentIndex::new(DOCUMENT_ID, records, 1).unwrap();
        let visible_index = VisibleDocumentIndex::from_document_index(&document_index);
        let height_index = BlockHeightIndex::new([
            HeightEstimate::new(10.0, HeightConfidence::Exact, 0.0),
            HeightEstimate::new(20.0, HeightConfidence::Exact, 0.0),
            HeightEstimate::new(30.0, HeightConfidence::Exact, 0.0),
            HeightEstimate::new(40.0, HeightConfidence::Exact, 0.0),
            HeightEstimate::new(50.0, HeightConfidence::Exact, 0.0),
        ])
        .unwrap();
        let page_index = PageLayoutIndex::from_block_height_index(
            &height_index,
            PagePolicy {
                max_blocks: 3,
                target_height: 1_000.0,
                ..PagePolicy::default()
            },
        )
        .unwrap();
        (visible_index, height_index, page_index)
    }
}
