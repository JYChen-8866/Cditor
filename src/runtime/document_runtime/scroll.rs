use super::*;

impl DocumentRuntime {
    pub fn scroll_by_delta(&mut self, delta_y: f64) -> Result<(), String> {
        self.scroll
            .scroll_by_delta(delta_y, ScrollOrigin::UserWheel)
            .map(|_| ())
            .map_err(|error| error.to_string())?;
        Ok(())
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
            crate::editor::scroll::ScrollPrecision::Exact
        } else if confidence == HeightConfidence::Exact {
            crate::editor::scroll::ScrollPrecision::LocalExact
        } else {
            crate::editor::scroll::ScrollPrecision::Estimated
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
