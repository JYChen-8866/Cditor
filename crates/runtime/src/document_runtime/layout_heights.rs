use super::*;

impl DocumentRuntime {
    pub fn has_dirty_layout(&self) -> bool {
        self.layout_dirty
    }

    pub fn mark_layout_saved(&mut self) {
        self.layout_dirty = false;
    }

    pub fn queue_measured_height(
        &mut self,
        block_id: BlockId,
        content_version: u64,
        height: f64,
    ) -> Result<bool, String> {
        if !height.is_finite() || height < 0.0 {
            return Err(format!(
                "invalid measured height for block {block_id}: {height}"
            ));
        }
        let Some(payload) = self.payload_window.get(block_id) else {
            return Ok(false);
        };
        if payload.content_version != content_version {
            return Ok(false);
        }
        let Some(document_index) = self.index.index_of(block_id) else {
            return Ok(false);
        };

        let previous_height = self
            .visible_index
            .visible_index_of(block_id)
            .and_then(|visible_index| self.height_index.heights.get(visible_index).copied())
            .unwrap_or_else(|| self.index.layout_meta[document_index].effective_height());
        if (previous_height - height).abs() < 0.5 {
            self.pending_measured_heights.remove(&block_id);
            return Ok(false);
        }

        self.pending_measured_heights.insert(
            block_id,
            PendingMeasuredHeight {
                content_version,
                height,
            },
        );
        Ok(true)
    }

    pub fn flush_pending_height_corrections(&mut self) -> Result<bool, String> {
        self.flush_pending_height_corrections_with_priority(HeightCorrectionPriority::Normal)
    }

    pub fn flush_pending_height_corrections_with_priority(
        &mut self,
        priority: HeightCorrectionPriority,
    ) -> Result<bool, String> {
        if self.pending_measured_heights.is_empty() {
            return Ok(false);
        }

        let restore_scroll_anchor = matches!(priority, HeightCorrectionPriority::Normal);
        let viewport_anchor = restore_scroll_anchor
            .then(|| self.target_for_global_offset(self.scroll.global_scroll_top))
            .flatten();
        let pending = std::mem::take(&mut self.pending_measured_heights);
        let mut page_deltas: HashMap<usize, f64> = HashMap::new();
        let mut should_restore_anchor = false;
        let mut applied = false;

        for (block_id, pending_height) in pending {
            let Some(payload) = self.payload_window.get(block_id) else {
                continue;
            };
            if payload.content_version != pending_height.content_version {
                continue;
            }
            let Some(document_index) = self.index.index_of(block_id) else {
                continue;
            };
            let Some(visible_index) = self.visible_index.visible_index_of(block_id) else {
                self.index.layout_meta[document_index].update_height(pending_height.height);
                self.layout_dirty = true;
                applied = true;
                continue;
            };

            let previous_height = self
                .height_index
                .heights
                .get(visible_index)
                .copied()
                .unwrap_or_else(|| self.index.layout_meta[document_index].effective_height());
            if (previous_height - pending_height.height).abs() < 0.5 {
                continue;
            }

            self.index.layout_meta[document_index].update_height(pending_height.height);
            self.layout_dirty = true;
            let height_change = self
                .height_index
                .update_height(visible_index, pending_height.height)
                .map_err(|error| error.to_string())?;
            if let Some(page_index) = self.page_layout.page_for_block_index(visible_index) {
                *page_deltas.entry(page_index).or_insert(0.0) += height_change.delta;
            }
            if let Some(anchor) = viewport_anchor
                && visible_index <= anchor.block_index
            {
                should_restore_anchor = true;
            }
            applied = true;
        }

        if !applied {
            return Ok(false);
        }

        for (page_index, delta) in page_deltas {
            if delta.abs() < 0.5 {
                continue;
            }
            let next_page_height = self.page_layout.pages[page_index].height + delta;
            self.page_layout
                .update_page_height(page_index, next_page_height)
                .map_err(|error| error.to_string())?;
        }

        let previous_model_total_height = self.scroll.model_total_height;
        let total_height = self.scroll_extent_height(self.height_index.total_height());
        self.scroll
            .set_model_total_height(total_height)
            .map_err(|error| error.to_string())?;
        let scrollbar_drag_active = self.scrollbar_drag.is_some();
        if let Some(scrollbar_drag) = &mut self.scrollbar_drag {
            scrollbar_drag.push_pending_height_correction(PendingHeightCorrection {
                old_total_height: previous_model_total_height,
                new_total_height: total_height,
            });
        } else {
            self.scroll
                .set_displayed_total_height(total_height)
                .map_err(|error| error.to_string())?;
        }

        if restore_scroll_anchor
            && !scrollbar_drag_active
            && should_restore_anchor
            && let Some(anchor) = viewport_anchor
            && let Some(new_anchor_top) = self.height_index.offset_of_block(anchor.block_index)
        {
            let restored = new_anchor_top + anchor.offset_in_block;
            self.scroll
                .scroll_to_global_offset(restored, ScrollOrigin::ProgrammaticVirtualScroll)
                .map_err(|error| error.to_string())?;
        }

        Ok(true)
    }

    pub fn apply_measured_height(
        &mut self,
        block_id: BlockId,
        content_version: u64,
        height: f64,
    ) -> Result<bool, String> {
        if self.queue_measured_height(block_id, content_version, height)? {
            self.flush_pending_height_corrections()
        } else {
            Ok(false)
        }
    }
}
