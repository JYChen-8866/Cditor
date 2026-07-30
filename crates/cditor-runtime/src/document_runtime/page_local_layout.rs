use super::*;

const PAGE_LOCAL_CACHE_NEIGHBOR_RADIUS: usize = 1;

impl DocumentRuntime {
    pub(super) fn current_page_layout_identity(&self) -> PageLayoutIdentity {
        PageLayoutIdentity::for_page(
            self.document_id,
            self.document.visible_index.source_structure_version,
            self.document.visible_index.visibility_version,
            0,
            PAGE_POLICY_VERSION,
            0,
        )
    }

    pub(super) fn preheat_page_local_cache(&mut self, page_range: Range<usize>) {
        let page_count = self.layout.page_layout.page_count();
        if page_count == 0 {
            self.layout.page_local_cache.clear();
            return;
        }
        let keep_start = page_range
            .start
            .saturating_sub(PAGE_LOCAL_CACHE_NEIGHBOR_RADIUS);
        let keep_end = page_range
            .end
            .saturating_add(PAGE_LOCAL_CACHE_NEIGHBOR_RADIUS)
            .min(page_count);
        self.layout
            .page_local_cache
            .retain(|page, _| keep_start <= *page && *page < keep_end);

        for page in keep_start..keep_end {
            if self.layout.page_local_cache.contains_key(&page) {
                continue;
            }
            if let Ok(local) = self
                .layout
                .page_layout
                .local_height_index_from_global(page, &self.layout.height_index)
            {
                self.layout.page_local_cache.insert(page, local);
            }
        }
    }

    pub(super) fn cached_or_global_height_estimate(
        &self,
        visible_index: usize,
    ) -> Option<HeightEstimate> {
        let page = self.layout.page_layout.page_for_block_index(visible_index);
        if let Some(local) = page.and_then(|page| self.layout.page_local_cache.get(&page)) {
            let local_index = visible_index.checked_sub(local.block_start)?;
            if let (Some(height), Some(confidence), Some(max_error_hint)) = (
                local.heights.get(local_index),
                local.confidence.get(local_index),
                local.max_error_hints.get(local_index),
            ) {
                return Some(HeightEstimate::new(*height, *confidence, *max_error_hint));
            }
        }
        Some(HeightEstimate::new(
            *self.layout.height_index.heights.get(visible_index)?,
            *self.layout.height_index.confidence.get(visible_index)?,
            0.0,
        ))
    }

    pub(super) fn synchronize_page_after_global_update(
        &mut self,
        page: usize,
    ) -> Result<(), String> {
        let local = self
            .layout
            .page_layout
            .local_height_index_from_global(page, &self.layout.height_index)
            .map_err(|error| error.to_string())?;
        self.layout
            .page_layout
            .synchronize_page_from_local(&local)
            .map_err(|error| error.to_string())?;
        if self.layout.page_local_cache.contains_key(&page) {
            self.layout.page_local_cache.insert(page, local);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runtime_with_blocks(count: u64) -> DocumentRuntime {
        DocumentRuntime::from_payloads(
            1,
            (1..=count)
                .map(|block_id| {
                    BlockPayloadRecord::rich_text(block_id, RichBlockKind::Paragraph, "")
                })
                .collect(),
            720.0,
        )
    }

    #[test]
    fn preheat_keeps_current_and_neighbor_pages_only() {
        let mut runtime = runtime_with_blocks(4_000);
        let page_count = runtime.layout.page_layout.page_count();
        assert!(page_count >= 4);

        runtime.preheat_page_local_cache(2..3);
        let mut cached = runtime
            .layout
            .page_local_cache
            .keys()
            .copied()
            .collect::<Vec<_>>();
        cached.sort_unstable();
        assert_eq!(cached, vec![1, 2, 3]);

        runtime.preheat_page_local_cache(0..1);
        let mut cached = runtime
            .layout
            .page_local_cache
            .keys()
            .copied()
            .collect::<Vec<_>>();
        cached.sort_unstable();
        assert_eq!(cached, vec![0, 1]);
    }

    #[test]
    fn global_height_update_refreshes_cached_page_and_summary() {
        let mut runtime = runtime_with_blocks(2_000);
        runtime.preheat_page_local_cache(0..1);
        let old_total = runtime.layout.page_layout.total_height();
        let old_height = runtime.layout.height_index.heights[0];
        runtime
            .layout
            .height_index
            .update_height(0, old_height + 25.0)
            .unwrap();
        runtime.synchronize_page_after_global_update(0).unwrap();

        assert_eq!(
            runtime.layout.page_local_cache[&0].heights[0],
            old_height + 25.0
        );
        assert_eq!(runtime.layout.page_layout.total_height(), old_total + 25.0);
        assert_eq!(
            runtime.layout.page_layout.total_height(),
            runtime.layout.height_index.total_height()
        );
    }
}
