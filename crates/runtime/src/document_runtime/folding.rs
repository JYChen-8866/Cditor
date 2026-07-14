use super::*;

impl DocumentRuntime {
    pub fn toggle_block_fold(&mut self, block_id: BlockId) -> Result<bool, String> {
        let kind = self.kind_for_block(block_id);
        let can_toggle = match kind {
            RichBlockKind::Heading { .. } => true,
            RichBlockKind::Toggle => self
                .visible_index
                .has_foldable_content(&self.index, block_id),
            _ => false,
        };
        if !can_toggle {
            return Ok(false);
        }

        let scroll_anchor = self.target_for_global_offset(self.scroll.global_scroll_top);
        let update = self
            .visible_index
            .toggle_folded(&self.index, block_id)
            .map_err(|error| error.to_string())?;
        let Some((_, folded)) = update.folded else {
            return Ok(false);
        };
        let document_index = self
            .index
            .index_of(block_id)
            .ok_or_else(|| format!("missing block {block_id} in document index"))?;
        if folded {
            self.index.flags[document_index] |= cditor_core::document::BLOCK_FLAG_FOLDED;
        } else {
            self.index.flags[document_index] &= !cditor_core::document::BLOCK_FLAG_FOLDED;
        }

        self.rebuild_height_indexes_from_layout_meta()?;
        // Visibility changes remap visible indices. Keep the range permissive and let
        // payload_window_covers verify every block id, otherwise expanding after a
        // collapsed structural insertion leaves a stale short range and renders only skeletons.
        self.payload_window.block_range = 0..self.visible_index.total_visible_count();
        self.restore_scroll_anchor_after_visibility_change(scroll_anchor)?;
        self.layout_dirty = true;
        Ok(true)
    }

    pub fn is_block_folded(&self, block_id: BlockId) -> bool {
        self.visible_index.is_folded(block_id)
    }

    fn restore_scroll_anchor_after_visibility_change(
        &mut self,
        anchor: Option<GlobalScrollTarget>,
    ) -> Result<(), String> {
        let Some(anchor) = anchor else {
            return Ok(());
        };
        let Some(target) = self
            .visible_index
            .resolve_scroll_target(&self.index, anchor.block_id)
        else {
            return Ok(());
        };
        let block_top = self
            .height_index
            .offset_of_block(target.visible_index)
            .unwrap_or_default();
        let offset_in_block = if target.target_block_id == anchor.block_id {
            self.height_index
                .heights
                .get(target.visible_index)
                .copied()
                .map(|height| anchor.offset_in_block.min(height))
                .unwrap_or_default()
        } else {
            0.0
        };
        self.scroll
            .scroll_to_global_offset(
                block_top + offset_in_block,
                ScrollOrigin::ProgrammaticVirtualScroll,
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn section_runtime() -> DocumentRuntime {
        DocumentRuntime::from_payloads(
            1,
            vec![
                BlockPayloadRecord::rich_text(1, RichBlockKind::Heading { level: 1 }, "H1"),
                BlockPayloadRecord::rich_text(2, RichBlockKind::Paragraph, "intro"),
                BlockPayloadRecord::rich_text(3, RichBlockKind::Heading { level: 2 }, "H2"),
                BlockPayloadRecord::rich_text(4, RichBlockKind::Paragraph, "detail"),
                BlockPayloadRecord::rich_text(5, RichBlockKind::Heading { level: 3 }, "H3"),
                BlockPayloadRecord::rich_text(6, RichBlockKind::Paragraph, "deep"),
                BlockPayloadRecord::rich_text(7, RichBlockKind::Heading { level: 1 }, "Next"),
                BlockPayloadRecord::rich_text(8, RichBlockKind::Paragraph, "tail"),
            ],
            720.0,
        )
    }

    #[test]
    fn h1_fold_updates_visible_projection_and_total_height_in_one_batch() {
        let mut runtime = section_runtime();
        let expanded_height = runtime.height_index.total_height();

        assert!(runtime.toggle_block_fold(1).unwrap());

        assert!(runtime.is_block_folded(1));
        assert_eq!(runtime.visible_index.visible_block_ids, vec![1, 7, 8]);
        assert_eq!(runtime.height_index.len(), 3);
        assert!(runtime.height_index.total_height() < expanded_height);
        assert_eq!(runtime.projection().total_visible_blocks, 3);
        let heading = &runtime.projection().blocks[0];
        assert!(heading.chrome.collapsed);
        assert_eq!(
            heading.chrome.prefix,
            cditor_core::block::BlockPrefixSnapshot::Heading { collapsed: true }
        );
        assert_ne!(
            runtime.index.flags[0] & cditor_core::document::BLOCK_FLAG_FOLDED,
            0
        );
        assert!(runtime.has_dirty_layout());
    }

    #[test]
    fn expanding_heading_restores_cached_block_heights() {
        let mut runtime = section_runtime();
        let expanded_height = runtime.height_index.total_height();
        runtime.toggle_block_fold(1).unwrap();

        assert!(runtime.toggle_block_fold(1).unwrap());

        assert!(!runtime.is_block_folded(1));
        assert_eq!(runtime.visible_index.total_visible_count(), 8);
        assert_eq!(runtime.height_index.total_height(), expanded_height);
    }

    #[test]
    fn enter_on_folded_heading_inserts_peer_after_section_and_expand_restores_payloads() {
        let mut runtime = section_runtime();
        runtime.focus_block_at_offset(1, 2).unwrap();
        runtime.toggle_block_fold(1).unwrap();

        runtime.handle_enter().unwrap();

        let inserted = runtime.focused_block_id().unwrap();
        assert_eq!(inserted, 9);
        assert_eq!(runtime.index.block_ids, vec![1, 2, 3, 4, 5, 6, 9, 7, 8]);
        assert_eq!(runtime.visible_index.visible_block_ids, vec![1, 9, 7, 8]);
        assert_eq!(
            runtime.kind_for_block(inserted),
            RichBlockKind::Heading { level: 1 }
        );
        assert_eq!(runtime.payload_window.get(1).unwrap().plain_text(), "H1");
        assert_eq!(
            runtime.payload_window.get(inserted).unwrap().plain_text(),
            ""
        );

        runtime.toggle_block_fold(1).unwrap();

        assert_eq!(
            runtime.visible_index.visible_block_ids,
            vec![1, 2, 3, 4, 5, 6, 9, 7, 8]
        );
        assert!(
            !runtime
                .projection_for_window()
                .render_window
                .is_placeholder()
        );
        assert!(
            runtime
                .projection_for_window()
                .blocks
                .iter()
                .all(|block| matches!(block.payload, BlockPayloadView::Loaded(_)))
        );
    }

    #[test]
    fn empty_heading_can_still_toggle_its_persistent_fold_state() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord::rich_text(
                1,
                RichBlockKind::Heading { level: 4 },
                "empty section",
            )],
            720.0,
        );

        assert!(runtime.toggle_block_fold(1).unwrap());
        assert!(runtime.is_block_folded(1));
        assert_eq!(runtime.visible_index.visible_block_ids, vec![1]);
        assert_eq!(
            runtime.projection().blocks[0].chrome.prefix,
            cditor_core::block::BlockPrefixSnapshot::Heading { collapsed: true }
        );

        assert!(runtime.toggle_block_fold(1).unwrap());
        assert!(!runtime.is_block_folded(1));
    }

    #[test]
    fn non_foldable_paragraph_does_not_change_visibility() {
        let mut runtime = section_runtime();

        assert!(!runtime.toggle_block_fold(2).unwrap());
        assert_eq!(runtime.visible_index.total_visible_count(), 8);
    }

    #[test]
    fn folding_keeps_document_selection_truth_while_hiding_its_fragments() {
        let mut runtime = section_runtime();
        runtime.set_document_text_selection(2, 1, 6, 2).unwrap();

        runtime.toggle_block_fold(1).unwrap();

        assert!(runtime.has_document_text_selection());
        assert!(
            runtime
                .projection()
                .blocks
                .iter()
                .all(|block| block.block_id != 2 && block.block_id != 6)
        );
    }

    #[test]
    fn collapsing_a_section_restores_a_hidden_viewport_anchor_to_the_heading() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![
                BlockPayloadRecord::rich_text(1, RichBlockKind::Heading { level: 1 }, "H1"),
                BlockPayloadRecord::rich_text(2, RichBlockKind::Paragraph, "one"),
                BlockPayloadRecord::rich_text(3, RichBlockKind::Paragraph, "two"),
                BlockPayloadRecord::rich_text(4, RichBlockKind::Paragraph, "three"),
                BlockPayloadRecord::rich_text(5, RichBlockKind::Heading { level: 1 }, "Next"),
                BlockPayloadRecord::rich_text(6, RichBlockKind::Paragraph, "tail"),
            ],
            48.0,
        );
        let hidden_anchor_top = runtime.height_index.offset_of_block(3).unwrap();
        runtime
            .scroll
            .scroll_to_global_offset(hidden_anchor_top, ScrollOrigin::ProgrammaticVirtualScroll)
            .unwrap();

        runtime.toggle_block_fold(1).unwrap();

        assert_eq!(runtime.scroll.global_scroll_top, 0.0);
    }
}
