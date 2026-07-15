use super::*;

const DOWN_PLACER_MIN_HEIGHT_PX: f64 = 180.0;
const DOWN_PLACER_MAX_HEIGHT_PX: f64 = 360.0;
const DOWN_PLACER_VIEWPORT_RATIO: f64 = 0.5;
const FOCUSED_BLOCK_BOTTOM_RESERVE_LINES: f64 = 4.0;
const FOCUSED_BLOCK_MIN_EDGE_MARGIN_PX: f64 = 8.0;

impl DocumentRuntime {
    pub fn down_placer_height(&self) -> f64 {
        (self.scroll.viewport_height * DOWN_PLACER_VIEWPORT_RATIO)
            .clamp(DOWN_PLACER_MIN_HEIGHT_PX, DOWN_PLACER_MAX_HEIGHT_PX)
    }

    pub(super) fn scroll_extent_height(&self, content_height: f64) -> f64 {
        content_height + self.down_placer_height()
    }

    pub fn sync_viewport_height(&mut self, viewport_height: f64) -> Result<bool, String> {
        let viewport_height = viewport_height.max(1.0);
        if (self.scroll.viewport_height - viewport_height).abs() < 0.5 {
            return Ok(false);
        }
        self.scroll
            .set_viewport_height(viewport_height)
            .map_err(|error| error.to_string())?;
        let total_height = self.scroll_extent_height(self.page_layout.total_height());
        self.scroll
            .set_model_total_height(total_height)
            .map_err(|error| error.to_string())?;
        self.scroll
            .set_displayed_total_height(total_height)
            .map_err(|error| error.to_string())?;
        Ok(true)
    }

    pub fn scroll_by_delta(&mut self, delta_y: f64) -> Result<(), String> {
        self.scroll
            .scroll_by_delta(delta_y, ScrollOrigin::UserWheel)
            .map(|_| ())
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn scroll_focused_block_into_view(&mut self) -> Result<bool, String> {
        let Some(block_id) = self.focused_block_id() else {
            return Ok(false);
        };
        let Some(visible_index) = self.visible_index.visible_index_of(block_id) else {
            return Ok(false);
        };
        let Some(block_top) = self.height_index.offset_of_block(visible_index) else {
            return Ok(false);
        };
        let block_height = self
            .height_index
            .heights
            .get(visible_index)
            .copied()
            .unwrap_or(0.0);
        let block_bottom = block_top + block_height;
        let viewport_top = self.scroll.global_scroll_top;
        let viewport_height = self.scroll.viewport_height.max(1.0);
        let viewport_bottom = viewport_top + viewport_height;
        let top_margin =
            48.0_f64.min((viewport_height / 4.0).max(FOCUSED_BLOCK_MIN_EDGE_MARGIN_PX));
        let bottom_reserve =
            focused_block_bottom_reserve_px(&self.kind_for_block(block_id), viewport_height);

        let next_scroll_top = if block_bottom + bottom_reserve > viewport_bottom {
            block_bottom + bottom_reserve - viewport_height
        } else if block_top - top_margin < viewport_top {
            block_top - top_margin
        } else {
            return Ok(false);
        };
        let before = self.scroll.global_scroll_top;
        self.scroll
            .scroll_to_global_offset(next_scroll_top, ScrollOrigin::ProgrammaticVirtualScroll)
            .map_err(|error| error.to_string())?;
        Ok((self.scroll.global_scroll_top - before).abs() > 0.5)
    }

    /// Scrolls through the virtual height model; `alignment` is 0.0 for start,
    /// 0.5 for center, 1.0 for end, and `None` for nearest.
    pub fn scroll_to_block_with_alignment(
        &mut self,
        block_id: BlockId,
        alignment: Option<f64>,
    ) -> Result<bool, String> {
        let target = self
            .visible_index
            .resolve_scroll_target(&self.index, block_id)
            .ok_or_else(|| format!("block {block_id} is missing from the document"))?;
        let block_top = self
            .height_index
            .offset_of_block(target.visible_index)
            .ok_or_else(|| format!("block {block_id} has no layout offset"))?;
        let block_height = self
            .height_index
            .heights
            .get(target.visible_index)
            .copied()
            .unwrap_or_default();
        let viewport_top = self.scroll.global_scroll_top;
        let viewport_height = self.scroll.viewport_height.max(1.0);
        let viewport_bottom = viewport_top + viewport_height;
        let next_scroll_top = match alignment {
            Some(alignment) => {
                block_top - (viewport_height - block_height) * alignment.clamp(0.0, 1.0)
            }
            None if block_top < viewport_top => block_top,
            None if block_top + block_height > viewport_bottom => {
                block_top + block_height - viewport_height
            }
            None => return Ok(false),
        };
        let before = self.scroll.global_scroll_top;
        self.scroll
            .scroll_to_global_offset(next_scroll_top, ScrollOrigin::ProgrammaticVirtualScroll)
            .map_err(|error| error.to_string())?;
        Ok((self.scroll.global_scroll_top - before).abs() > 0.5)
    }

    pub fn scrollbar_visual_state(&self, policy: ScrollbarPolicy) -> ScrollbarVisualState {
        ScrollbarVisualState::from_virtual_scroll(&self.scroll, policy)
    }

    pub fn begin_scrollbar_drag(&mut self, policy: ScrollbarPolicy) -> ScrollbarVisualState {
        let visual = self.scrollbar_visual_state(policy);
        if visual.enabled {
            self.scrollbar_drag = Some(ScrollbarDragSession::begin(&mut self.scroll, visual));
        }
        visual
    }

    pub fn drag_scrollbar_to_thumb_top(
        &mut self,
        policy: ScrollbarPolicy,
        thumb_top: f64,
    ) -> Result<Option<ScrollbarDragUpdate>, String> {
        let Some(session) = &self.scrollbar_drag else {
            return Ok(None);
        };
        session
            .drag_to_thumb_top(&mut self.scroll, policy, thumb_top)
            .map(Some)
            .map_err(|error| error.to_string())
    }

    pub fn finish_scrollbar_drag(&mut self) -> Result<Option<ScrollbarDragEnd>, String> {
        let Some(session) = self.scrollbar_drag.take() else {
            return Ok(None);
        };
        let end = session.finish(&mut self.scroll);
        self.scroll
            .set_displayed_total_height(self.scroll.model_total_height)
            .map_err(|error| error.to_string())?;
        Ok(Some(end))
    }

    pub fn target_for_global_offset(&self, global_y: f64) -> Option<GlobalScrollTarget> {
        let clamped = self.scroll.clamp_global_scroll_top(global_y);
        let block_hit = self.height_index.block_at_offset(clamped)?;
        let block_id = self.visible_index.id_at_visible_index(block_hit.index)?;
        let page_hit = self.page_layout.page_at_offset(clamped)?;
        let confidence = self
            .height_index
            .confidence
            .get(block_hit.index)
            .copied()
            .unwrap_or(HeightConfidence::Default);
        let precision = if confidence == HeightConfidence::Exact
            && self
                .page_layout
                .pages
                .get(page_hit.page_index)
                .is_some_and(|page| page.confidence == HeightConfidence::Exact)
        {
            cditor_editor::scroll::ScrollPrecision::Exact
        } else if confidence == HeightConfidence::Exact {
            cditor_editor::scroll::ScrollPrecision::LocalExact
        } else {
            cditor_editor::scroll::ScrollPrecision::Estimated
        };
        Some(GlobalScrollTarget {
            global_scroll_top: clamped,
            block_index: block_hit.index,
            block_id,
            block_top: block_hit.block_top,
            offset_in_block: block_hit.offset_in_block,
            page_index: page_hit.page_index,
            page_top: page_hit.page_top,
            offset_in_page: page_hit.offset_in_page,
            precision,
        })
    }

    pub fn current_page_window(&self) -> Range<usize> {
        let page_count = self.page_layout.page_count();
        if page_count == 0 {
            return 0..0;
        }

        let current_page = self
            .target_for_global_offset(self.scroll.global_scroll_top)
            .map(|target| target.page_index)
            .unwrap_or(0)
            .min(page_count - 1);
        WindowPlanner::new(1, 2, WindowPlannerPolicy::default()).plan(current_page, page_count)
    }

    pub fn current_page_window_planned(&mut self) -> Range<usize> {
        let page_count = self.page_layout.page_count();
        if page_count == 0 {
            return 0..0;
        }
        let Some(target) = self.target_for_global_offset(self.scroll.global_scroll_top) else {
            return 0..0;
        };
        let viewport_height = self.scroll.viewport_height.max(1.0);
        let position_in_page_viewports = (target.offset_in_page / viewport_height).clamp(0.0, 1.0);
        let direction = if self.scroll.global_scroll_top > self.last_planned_scroll_top {
            ScrollDirection::Down
        } else if self.scroll.global_scroll_top < self.last_planned_scroll_top {
            ScrollDirection::Up
        } else {
            ScrollDirection::Still
        };
        self.last_planned_scroll_top = self.scroll.global_scroll_top;
        self.window_plan_clock_ms = self.window_plan_clock_ms.saturating_add(16);
        let decision = self.window_planner.plan_commit(WindowPlanRequest {
            target_page: target.page_index,
            page_count,
            scroll_direction: direction,
            position_in_page_viewports,
            pinned_pages: self.pinned_pages_for_window_plan(),
            now_ms: self.window_plan_clock_ms,
        });
        match decision {
            WindowPlanDecision::Keep { page_range, .. }
            | WindowPlanDecision::Commit { page_range } => page_range,
        }
    }

    fn pinned_pages_for_window_plan(&self) -> BTreeSet<usize> {
        let mut pages = BTreeSet::new();
        if let Some(block_id) = self.focused_block_id()
            && let Some(visible_index) = self.visible_index.visible_index_of(block_id)
            && let Some(page) = self.page_layout.page_for_block_index(visible_index)
        {
            pages.insert(page);
        }
        for block_id in &self.selected_block_ids {
            if let Some(visible_index) = self.visible_index.visible_index_of(*block_id)
                && let Some(page) = self.page_layout.page_for_block_index(visible_index)
            {
                pages.insert(page);
            }
        }
        pages
    }
}

fn focused_block_bottom_reserve_px(kind: &RichBlockKind, viewport_height: f64) -> f64 {
    let desired = text_line_height_for_kind(kind) * FOCUSED_BLOCK_BOTTOM_RESERVE_LINES;
    let max_reserve = (viewport_height * 0.4).max(FOCUSED_BLOCK_MIN_EDGE_MARGIN_PX);
    desired.clamp(FOCUSED_BLOCK_MIN_EDGE_MARGIN_PX, max_reserve)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroll_focused_block_into_view_reveals_block_below_viewport() {
        let payloads = (1..=10)
            .map(|block_id| {
                BlockPayloadRecord::rich_text(block_id, RichBlockKind::Paragraph, "line")
            })
            .collect::<Vec<_>>();
        let mut runtime = DocumentRuntime::from_payloads(1, payloads, 100.0);
        runtime.focus_block_at_offset(10, 0).unwrap();

        assert_eq!(runtime.scroll.global_scroll_top, 0.0);
        assert!(runtime.scroll_focused_block_into_view().unwrap());
        assert!(runtime.scroll.global_scroll_top > 0.0);
    }

    #[test]
    fn scroll_focused_block_into_view_keeps_a_few_lines_above_bottom_spacer() {
        let payloads = (1..=8)
            .map(|block_id| {
                BlockPayloadRecord::rich_text(block_id, RichBlockKind::Paragraph, "line")
            })
            .collect::<Vec<_>>();
        let mut runtime = DocumentRuntime::from_payloads(1, payloads, 240.0);
        runtime.sync_viewport_height(241.0).unwrap();
        runtime.focus_block_at_offset(8, 0).unwrap();
        let visible_index = runtime.visible_index.visible_index_of(8).unwrap();
        let block_bottom = runtime.height_index.offset_of_block(visible_index).unwrap()
            + runtime.height_index.heights[visible_index];
        runtime
            .scroll
            .scroll_to_global_offset(
                block_bottom - runtime.scroll.viewport_height + 12.0,
                ScrollOrigin::ProgrammaticVirtualScroll,
            )
            .unwrap();
        let reserve = focused_block_bottom_reserve_px(
            &RichBlockKind::Paragraph,
            runtime.scroll.viewport_height,
        );

        assert!(block_bottom < runtime.scroll.global_scroll_top + runtime.scroll.viewport_height);
        assert!(runtime.scroll_focused_block_into_view().unwrap());
        assert!(
            runtime.scroll.global_scroll_top + runtime.scroll.viewport_height
                >= block_bottom + reserve - 0.5
        );
    }

    #[test]
    fn sync_viewport_height_updates_scroll_state() {
        let mut runtime = DocumentRuntime::demo();

        assert!(runtime.sync_viewport_height(480.0).unwrap());
        assert_eq!(runtime.scroll.viewport_height, 480.0);
        assert!(runtime.scroll.model_total_height > runtime.height_index.total_height());
        assert!(!runtime.sync_viewport_height(480.25).unwrap());
    }

    #[test]
    fn down_placer_focus_creates_trailing_paragraph_once() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord::rich_text(
                1,
                RichBlockKind::Paragraph,
                "hello",
            )],
            320.0,
        );

        assert!(runtime.focus_or_create_down_placer_paragraph().unwrap());
        let trailing = runtime
            .visible_index
            .visible_block_ids
            .last()
            .copied()
            .unwrap();
        assert_eq!(runtime.focused_block_id(), Some(trailing));
        assert_eq!(
            runtime.payload_window.get(trailing).unwrap().plain_text(),
            ""
        );

        assert!(!runtime.focus_or_create_down_placer_paragraph().unwrap());
        assert_eq!(runtime.visible_index.total_visible_count(), 2);
    }
}
