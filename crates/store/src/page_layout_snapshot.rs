use cditor_core::ids::BlockId;
use cditor_core::layout::{PageLayout, PageLayoutIndex, PagePolicy};

use crate::error::{StorageError, StorageResult};
use crate::layout_cache::LayoutCacheKey;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StoragePageLayoutPage {
    pub layout: PageLayout,
    pub first_block_id: BlockId,
    pub last_block_id: BlockId,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StoragePageLayoutSnapshot {
    pub visible_index_version: i64,
    pub structure_version: u64,
    pub layout_key_hash: String,
    pub page_policy_version: u64,
    pub pages: Vec<StoragePageLayoutPage>,
}

impl StoragePageLayoutSnapshot {
    pub fn from_page_layout(
        visible_index_version: i64,
        structure_version: u64,
        layout_key: LayoutCacheKey,
        page_policy_version: u64,
        page_layout: &PageLayoutIndex,
        visible_block_ids: &[BlockId],
    ) -> StorageResult<Self> {
        PageLayoutIndex::from_cached_pages(
            page_layout.pages.clone(),
            page_layout.policy,
            visible_block_ids.len(),
        )
        .map_err(|error| StorageError::CorruptData(error.to_string()))?;
        let pages = page_layout
            .pages
            .iter()
            .copied()
            .map(|layout| {
                let end = layout.block_end();
                let first_block_id = visible_block_ids.get(layout.block_start).copied();
                let last_block_id = end
                    .checked_sub(1)
                    .and_then(|index| visible_block_ids.get(index))
                    .copied();
                match (first_block_id, last_block_id) {
                    (Some(first_block_id), Some(last_block_id)) => Ok(StoragePageLayoutPage {
                        layout,
                        first_block_id,
                        last_block_id,
                    }),
                    _ => Err(StorageError::CorruptData(format!(
                        "page {} has invalid visible block boundaries",
                        layout.page_index
                    ))),
                }
            })
            .collect::<StorageResult<Vec<_>>>()?;

        Ok(Self {
            visible_index_version,
            structure_version,
            layout_key_hash: layout_key.hash_key(),
            page_policy_version,
            pages,
        })
    }

    pub fn to_page_layout_index(
        &self,
        expected_visible_index_version: i64,
        expected_structure_version: u64,
        expected_layout_key: LayoutCacheKey,
        expected_page_policy_version: u64,
        policy: PagePolicy,
        visible_block_ids: &[BlockId],
    ) -> StorageResult<PageLayoutIndex> {
        if self.visible_index_version != expected_visible_index_version
            || self.structure_version != expected_structure_version
            || self.layout_key_hash != expected_layout_key.hash_key()
            || self.page_policy_version != expected_page_policy_version
        {
            return Err(StorageError::CorruptData(
                "page layout snapshot context does not match the open document".to_owned(),
            ));
        }

        let index = PageLayoutIndex::from_cached_pages(
            self.pages.iter().map(|page| page.layout).collect(),
            policy,
            visible_block_ids.len(),
        )
        .map_err(|error| StorageError::CorruptData(error.to_string()))?;

        for page in &self.pages {
            let end = page.layout.block_end();
            let actual_first = visible_block_ids.get(page.layout.block_start).copied();
            let actual_last = end
                .checked_sub(1)
                .and_then(|index| visible_block_ids.get(index))
                .copied();
            if actual_first != Some(page.first_block_id) || actual_last != Some(page.last_block_id)
            {
                return Err(StorageError::CorruptData(format!(
                    "page {} visible block boundaries do not match the document projection",
                    page.layout.page_index
                )));
            }
        }
        Ok(index)
    }
}

#[cfg(test)]
mod tests {
    use cditor_core::layout::{HeightConfidence, PageLayout, PagePolicy};

    use super::*;

    fn layout_key() -> LayoutCacheKey {
        LayoutCacheKey {
            width_bucket: 10,
            exact_width_px: 800,
            content_version: 1,
            attrs_version: 0,
            style_version: 0,
            font_version: 0,
            theme_version: 0,
            scale_factor_milli: 1_000,
        }
    }

    fn snapshot() -> StoragePageLayoutSnapshot {
        StoragePageLayoutSnapshot {
            visible_index_version: 2,
            structure_version: 7,
            layout_key_hash: layout_key().hash_key(),
            page_policy_version: 1,
            pages: vec![StoragePageLayoutPage {
                layout: PageLayout {
                    page_index: 0,
                    block_start: 0,
                    block_count: 2,
                    height: 88.0,
                    measured_ratio: 1.0,
                    confidence: HeightConfidence::Exact,
                    max_error_hint: 0.0,
                    dirty: false,
                },
                first_block_id: 11,
                last_block_id: 12,
            }],
        }
    }

    #[test]
    fn validates_context_coverage_and_visible_boundaries() {
        let index = snapshot()
            .to_page_layout_index(2, 7, layout_key(), 1, PagePolicy::default(), &[11, 12])
            .unwrap();
        assert_eq!(index.total_height(), 88.0);

        let error = snapshot()
            .to_page_layout_index(2, 7, layout_key(), 1, PagePolicy::default(), &[11, 99])
            .unwrap_err();
        assert!(error.to_string().contains("boundaries"));
    }

    #[test]
    fn rejects_stale_snapshot_context() {
        let error = snapshot()
            .to_page_layout_index(2, 8, layout_key(), 1, PagePolicy::default(), &[11, 12])
            .unwrap_err();
        assert!(error.to_string().contains("context"));
    }
}
