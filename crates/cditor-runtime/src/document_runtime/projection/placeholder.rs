use super::*;

impl DocumentRuntime {
    pub(super) fn placeholder_projection_for_ranges(
        &self,
        page_range: Range<usize>,
        block_range: Range<usize>,
    ) -> EditorViewProjection {
        self.placeholder_projection_for_ranges_with_visible_core(
            page_range,
            block_range.clone(),
            block_range,
        )
    }

    pub(super) fn placeholder_projection_for_ranges_with_visible_core(
        &self,
        page_range: Range<usize>,
        block_range: Range<usize>,
        visible_block_range: Range<usize>,
    ) -> EditorViewProjection {
        let total_visible_blocks = self.document.visible_index.total_visible_count();
        let before_window_height = self
            .layout
            .height_index
            .offset_of_block(block_range.start)
            .unwrap_or(0.0);
        let placeholder_height = self.height_for_block_range(&block_range);
        // Geometry spans the render window, but failure ownership belongs to the
        // visible readiness core. An exhausted overscan request must never
        // suppress the visible request or become the target of explicit retry.
        let placeholder_window_failure = self.payload_failure_view_for(&visible_block_range);
        let placeholder_window_error = placeholder_window_failure
            .as_ref()
            .map(|failure| failure.message.clone());
        let render_window = RenderWindow::placeholder(PlaceholderWindow {
            page_range: page_range.clone(),
            block_range: block_range.clone(),
            height: placeholder_height,
            target_anchor: self
                .target_for_global_offset(self.layout.scroll.global_scroll_top)
                .map(|target| cditor_viewport::scroll::ScrollAnchor {
                    block_id: target.block_id,
                    offset_in_block: target.offset_in_block,
                    viewport_y: 0.0,
                }),
        });
        let down_placer_height = self.down_placer_height();
        let after_window_height = (self
            .scroll_extent_height(self.layout.page_layout.total_height())
            - before_window_height
            - placeholder_height)
            .max(0.0);
        let debug = DebugOverlaySnapshot::from_scroll_state(
            &self.layout.scroll,
            0,
            render_window.page_range.clone(),
        )
        .with_entity_stats(0, 0);
        EditorViewProjection {
            document_id: self.document_id,
            viewport_revision: 0,
            window_generation: self.layout.projection.generation(),
            scroll: self.layout.scroll,
            render_window,
            payload_visible_block_range: visible_block_range,
            payload_prefetch_block_range: block_range.clone(),
            payload_prefetch_resident: false,
            layout_prefetch_page_range: page_range.clone(),
            blocks: Vec::new(),
            ai_preview: None,
            before_window_height,
            placeholder_window_height: Some(placeholder_height),
            placeholder_window_error,
            placeholder_window_failure,
            after_window_height,
            down_placer_height,
            total_visible_blocks,
            debug,
        }
    }

    fn height_for_block_range(&self, block_range: &Range<usize>) -> f64 {
        let start = block_range.start.min(self.layout.height_index.len());
        let end = block_range
            .end
            .min(self.layout.height_index.len())
            .max(start);
        let start_offset = self
            .layout
            .height_index
            .offset_of_block(start)
            .unwrap_or(0.0);
        let end_offset = self
            .layout
            .height_index
            .offset_of_block(end)
            .unwrap_or(start_offset);
        (end_offset - start_offset).max(0.0)
    }
}
