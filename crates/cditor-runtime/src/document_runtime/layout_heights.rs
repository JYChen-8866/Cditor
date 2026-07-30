use super::*;

impl DocumentRuntime {
    pub fn has_dirty_layout(&self) -> bool {
        self.layout.dirty
    }

    pub fn mark_layout_saved(&mut self) {
        self.layout.dirty = false;
    }

    pub fn queue_measured_height(
        &mut self,
        block_id: BlockId,
        content_version: u64,
        height: f64,
    ) -> Result<bool, String> {
        if !height.is_finite() || height < 0.0 {
            trace_image_resize(
                "height.reject",
                format_args!("block={block_id} version={content_version} height={height} invalid"),
            );
            return Err(format!(
                "invalid measured height for block {block_id}: {height}"
            ));
        }
        let Some(payload) = self.document.payload_window.get(block_id) else {
            trace_image_resize(
                "height.reject",
                format_args!(
                    "block={block_id} version={content_version} height={height:.2} payload_missing"
                ),
            );
            return Ok(false);
        };
        if payload.content_version != content_version {
            trace_image_resize(
                "height.reject",
                format_args!(
                    "block={block_id} requested_version={content_version} current_version={} height={height:.2} stale_version",
                    payload.content_version,
                ),
            );
            return Ok(false);
        }
        let Some(document_index) = self.document.index.index_of(block_id) else {
            return Ok(false);
        };

        let indexed_height = self
            .document
            .visible_index
            .visible_index_of(block_id)
            .and_then(|visible_index| self.layout.height_index.heights.get(visible_index).copied());
        let metadata_height = self.document.index.layout_meta[document_index].effective_height();
        let index_matches = indexed_height.is_none_or(|previous| (previous - height).abs() < 0.5);
        let metadata_matches = (metadata_height - height).abs() < 0.5;
        if index_matches && metadata_matches {
            self.layout.pending_measured_heights.remove(&block_id);
            trace_image_resize(
                "height.unchanged",
                format_args!(
                    "block={block_id} version={content_version} indexed={indexed_height:?} metadata={metadata_height:.2} next={height:.2}"
                ),
            );
            return Ok(false);
        }

        self.layout.pending_measured_heights.insert(
            block_id,
            PendingMeasuredHeight {
                content_version,
                height,
            },
        );
        trace_image_resize(
            "height.queued",
            format_args!(
                "block={block_id} version={content_version} indexed={indexed_height:?} metadata={metadata_height:.2} next={height:.2} pending={} ",
                self.layout.pending_measured_heights.len(),
            ),
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
        if self.layout.pending_measured_heights.is_empty() {
            return Ok(false);
        }

        let restore_scroll_anchor = matches!(priority, HeightCorrectionPriority::Normal);
        let viewport_anchor = restore_scroll_anchor
            .then(|| self.target_for_global_offset(self.layout.scroll.global_scroll_top))
            .flatten();
        let pending = std::mem::take(&mut self.layout.pending_measured_heights);
        trace_image_resize(
            "height.flush_begin",
            format_args!(
                "priority={priority:?} pending={} scroll_top={:.2} total={:.2}",
                pending.len(),
                self.layout.scroll.global_scroll_top,
                self.layout.page_layout.total_height(),
            ),
        );
        let mut affected_pages = HashSet::new();
        let mut should_restore_anchor = false;
        let mut applied = false;
        let mut global_height_changed = false;

        for (block_id, pending_height) in pending {
            let Some(payload) = self.document.payload_window.get(block_id) else {
                continue;
            };
            if payload.content_version != pending_height.content_version {
                continue;
            }
            let Some(document_index) = self.document.index.index_of(block_id) else {
                continue;
            };
            let Some(visible_index) = self.document.visible_index.visible_index_of(block_id) else {
                self.document.index.layout_meta[document_index]
                    .update_height(pending_height.height);
                self.layout.dirty = true;
                applied = true;
                continue;
            };

            let indexed_height = self
                .layout
                .height_index
                .heights
                .get(visible_index)
                .copied()
                .unwrap_or_else(|| {
                    self.document.index.layout_meta[document_index].effective_height()
                });
            let metadata_height =
                self.document.index.layout_meta[document_index].effective_height();
            let index_matches = (indexed_height - pending_height.height).abs() < 0.5;
            let metadata_matches = (metadata_height - pending_height.height).abs() < 0.5;
            if index_matches && metadata_matches {
                continue;
            }

            if !metadata_matches {
                self.document.index.layout_meta[document_index]
                    .update_height(pending_height.height);
            }
            self.layout.dirty = true;
            trace_image_resize(
                "height.applied",
                format_args!(
                    "block={block_id} visible_index={visible_index} version={} indexed={indexed_height:.2} metadata={metadata_height:.2} next={:.2} update_index={} update_metadata={}",
                    pending_height.content_version,
                    pending_height.height,
                    !index_matches,
                    !metadata_matches,
                ),
            );
            if !index_matches {
                self.layout
                    .height_index
                    .update_height(visible_index, pending_height.height)
                    .map_err(|error| error.to_string())?;
                global_height_changed = true;
                if let Some(page_index) =
                    self.layout.page_layout.page_for_block_index(visible_index)
                {
                    affected_pages.insert(page_index);
                }
                if let Some(anchor) = viewport_anchor
                    && visible_index <= anchor.block_index
                {
                    should_restore_anchor = true;
                }
            }
            applied = true;
        }

        if !applied {
            return Ok(false);
        }

        if !global_height_changed {
            trace_image_resize(
                "height.flush_end",
                format_args!(
                    "priority={priority:?} metadata_only=true total={:.2} displayed_total={:.2} scroll_top={:.2}",
                    self.layout.page_layout.total_height(),
                    self.layout.scroll.displayed_total_height,
                    self.layout.scroll.global_scroll_top,
                ),
            );
            return Ok(true);
        }

        for page_index in affected_pages {
            let before = self.layout.page_layout.pages[page_index].height;
            self.synchronize_page_after_global_update(page_index)?;
            trace_image_resize(
                "page.synchronized",
                format_args!(
                    "page={page_index} before={before:.2} after={:.2}",
                    self.layout.page_layout.pages[page_index].height,
                ),
            );
        }

        let previous_model_total_height = self.layout.scroll.model_total_height;
        let total_height = self.scroll_extent_height(self.layout.page_layout.total_height());
        self.layout
            .scroll
            .set_model_total_height(total_height)
            .map_err(|error| error.to_string())?;
        let scrollbar_drag_active = self.layout.scrollbar_drag.is_some();
        if let Some(scrollbar_drag) = &mut self.layout.scrollbar_drag {
            scrollbar_drag.push_pending_height_correction(PendingHeightCorrection {
                old_total_height: previous_model_total_height,
                new_total_height: total_height,
            });
        } else {
            self.layout
                .scroll
                .set_displayed_total_height(total_height)
                .map_err(|error| error.to_string())?;
        }

        if restore_scroll_anchor
            && !scrollbar_drag_active
            && should_restore_anchor
            && let Some(anchor) = viewport_anchor
            && let Some(new_anchor_top) =
                self.layout.height_index.offset_of_block(anchor.block_index)
        {
            let restored = new_anchor_top + anchor.offset_in_block;
            self.layout
                .scroll
                .scroll_to_global_offset(restored, ScrollOrigin::ProgrammaticVirtualScroll)
                .map_err(|error| error.to_string())?;
        }

        trace_image_resize(
            "height.flush_end",
            format_args!(
                "priority={priority:?} total={total_height:.2} displayed_total={:.2} scroll_top={:.2} restore_anchor={should_restore_anchor}",
                self.layout.scroll.displayed_total_height, self.layout.scroll.global_scroll_top,
            ),
        );

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
