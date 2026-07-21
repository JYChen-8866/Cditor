use super::*;

impl DocumentRuntime {
    pub(super) fn kind_for_block(&self, block_id: BlockId) -> RichBlockKind {
        self.payload_window
            .get(block_id)
            .map(|payload| payload.kind.clone())
            .or_else(|| {
                self.index
                    .index_of(block_id)
                    .map(|index| rich_block_kind_from_tag(self.index.kind_tags[index]))
            })
            .unwrap_or(RichBlockKind::Paragraph)
    }

    pub(super) fn kind_at_index(&self, index: usize) -> RichBlockKind {
        self.index
            .block_ids
            .get(index)
            .and_then(|block_id| self.payload_window.get(*block_id))
            .map(|payload| payload.kind.clone())
            .unwrap_or_else(|| rich_block_kind_from_tag(self.index.kind_tags[index]))
    }

    pub(super) fn subtree_end(&self, index: usize) -> usize {
        let depth = self.index.depths[index];
        let mut end = index + 1;
        while end < self.index.block_ids.len() && self.index.depths[end] > depth {
            end += 1;
        }
        end
    }

    pub(super) fn direct_children(&self, parent_id: Option<BlockId>) -> Vec<BlockId> {
        self.index
            .block_ids
            .iter()
            .enumerate()
            .filter_map(|(index, block_id)| {
                (self.index.parent_ids[index] == parent_id).then_some(*block_id)
            })
            .collect()
    }

    pub(super) fn direct_child_position(
        &self,
        parent_id: Option<BlockId>,
        block_id: BlockId,
    ) -> Option<usize> {
        self.direct_children(parent_id)
            .iter()
            .position(|candidate| *candidate == block_id)
    }

    pub(super) fn index_record_for_block(
        &self,
        block_id: BlockId,
    ) -> Result<BlockIndexRecord, String> {
        let index = self
            .index
            .index_of(block_id)
            .ok_or_else(|| format!("missing block {block_id} in index"))?;
        Ok(BlockIndexRecord::new(
            block_id,
            self.index.parent_ids[index],
            self.index.depths[index],
            self.index.kind_tags[index],
            self.index.flags[index],
        )
        .with_layout_meta(self.index.layout_meta[index]))
    }

    pub(super) fn index_records(&self) -> Vec<BlockIndexRecord> {
        self.index
            .block_ids
            .iter()
            .enumerate()
            .map(|(index, block_id)| {
                BlockIndexRecord::new(
                    *block_id,
                    self.index.parent_ids[index],
                    self.index.depths[index],
                    self.index.kind_tags[index],
                    self.index.flags[index],
                )
                .with_layout_meta(self.index.layout_meta[index])
            })
            .collect()
    }

    pub(super) fn rebuild_structure_index(
        &mut self,
        records: Vec<BlockIndexRecord>,
    ) -> Result<(), String> {
        self.index = DocumentIndex::new(
            self.document_id,
            records,
            self.index.structure_version.saturating_add(1),
        )
        .map_err(|error| error.to_string())?;
        self.visible_index = VisibleDocumentIndex::from_document_index(&self.index);
        self.list_projection_cache = ListProjectionCache::build(&self.index);
        self.payload_window.block_range = 0..self.visible_index.total_visible_count();
        self.rebuild_height_indexes_from_layout_meta()?;
        self.selected_block_ids.clear();
        Ok(())
    }

    pub(super) fn rebuild_height_indexes_from_layout_meta(&mut self) -> Result<(), String> {
        self.height_index =
            BlockHeightIndex::from_visible_document(&self.index, &self.visible_index)
                .map_err(|error| error.to_string())?;
        self.page_layout =
            PageLayoutIndex::from_block_height_index(&self.height_index, PagePolicy::default())
                .map_err(|error| error.to_string())?;
        let total_height = self.scroll_extent_height(self.page_layout.total_height());
        self.scroll
            .set_model_total_height(total_height)
            .map_err(|error| error.to_string())?;
        self.scroll
            .set_displayed_total_height(total_height)
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub(super) fn next_available_block_id(&self) -> BlockId {
        self.index
            .block_ids
            .iter()
            .copied()
            .chain(self.payload_window.payloads.keys().copied())
            .max()
            .unwrap_or(0)
            .saturating_add(1)
    }
}
