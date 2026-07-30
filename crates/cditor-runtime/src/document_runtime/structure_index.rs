use super::*;

impl DocumentRuntime {
    pub(super) fn kind_for_block(&self, block_id: BlockId) -> RichBlockKind {
        self.document
            .payload_window
            .get(block_id)
            .map(|payload| payload.kind.clone())
            .or_else(|| {
                self.document
                    .index
                    .index_of(block_id)
                    .map(|index| rich_block_kind_from_tag(self.document.index.kind_tags[index]))
            })
            .unwrap_or(RichBlockKind::Paragraph)
    }

    pub(super) fn kind_at_index(&self, index: usize) -> RichBlockKind {
        self.document
            .index
            .block_ids
            .get(index)
            .and_then(|block_id| self.document.payload_window.get(*block_id))
            .map(|payload| payload.kind.clone())
            .unwrap_or_else(|| rich_block_kind_from_tag(self.document.index.kind_tags[index]))
    }

    pub(super) fn subtree_end(&self, index: usize) -> usize {
        let depth = self.document.index.depths[index];
        let mut end = index + 1;
        while end < self.document.index.block_ids.len() && self.document.index.depths[end] > depth {
            end += 1;
        }
        end
    }

    pub(super) fn direct_children(&self, parent_id: Option<BlockId>) -> Vec<BlockId> {
        self.document
            .index
            .block_ids
            .iter()
            .enumerate()
            .filter_map(|(index, block_id)| {
                (self.document.index.parent_ids[index] == parent_id).then_some(*block_id)
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
            .document
            .index
            .index_of(block_id)
            .ok_or_else(|| format!("missing block {block_id} in index"))?;
        Ok(BlockIndexRecord::new(
            block_id,
            self.document.index.parent_ids[index],
            self.document.index.depths[index],
            self.document.index.kind_tags[index],
            self.document.index.flags[index],
        )
        .with_layout_meta(self.document.index.layout_meta[index]))
    }

    pub(super) fn index_records(&self) -> Vec<BlockIndexRecord> {
        self.document
            .index
            .block_ids
            .iter()
            .enumerate()
            .map(|(index, block_id)| {
                BlockIndexRecord::new(
                    *block_id,
                    self.document.index.parent_ids[index],
                    self.document.index.depths[index],
                    self.document.index.kind_tags[index],
                    self.document.index.flags[index],
                )
                .with_layout_meta(self.document.index.layout_meta[index])
            })
            .collect()
    }

    pub(super) fn rebuild_structure_index(
        &mut self,
        records: Vec<BlockIndexRecord>,
    ) -> Result<(), String> {
        self.document.index = DocumentIndex::new(
            self.document_id,
            records,
            self.document.index.structure_version.saturating_add(1),
        )
        .map_err(|error| error.to_string())?;
        self.document.visible_index =
            VisibleDocumentIndex::from_document_index(&self.document.index);
        self.document.list_projection_cache = ListProjectionCache::build(&self.document.index);
        self.document.payload_window.block_range =
            0..self.document.visible_index.total_visible_count();
        self.rebuild_height_indexes_from_layout_meta()?;
        self.selection.selected_block_ids.clear();
        Ok(())
    }

    pub(super) fn rebuild_height_indexes_from_layout_meta(&mut self) -> Result<(), String> {
        self.layout.height_index = BlockHeightIndex::from_visible_document(
            &self.document.index,
            &self.document.visible_index,
        )
        .map_err(|error| error.to_string())?;
        self.layout.page_layout = PageLayoutIndex::from_block_height_index(
            &self.layout.height_index,
            PagePolicy::default(),
        )
        .map_err(|error| error.to_string())?
        .with_identity(self.current_page_layout_identity());
        self.layout.page_local_cache.clear();
        let total_height = self.scroll_extent_height(self.layout.page_layout.total_height());
        self.layout
            .scroll
            .set_model_total_height(total_height)
            .map_err(|error| error.to_string())?;
        self.layout
            .scroll
            .set_displayed_total_height(total_height)
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub(super) fn next_available_block_id(&self) -> BlockId {
        self.document
            .index
            .block_ids
            .iter()
            .copied()
            .chain(self.document.payload_window.payloads.keys().copied())
            .max()
            .unwrap_or(0)
            .saturating_add(1)
    }
}
