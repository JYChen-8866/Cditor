use std::time::Duration;

use gpui::{AppContext, Context, Pixels, Point, Window};

use crate::core::block::{BlockDropTarget, DragPoint, GutterBlockDragState};
use crate::core::ids::BlockId;
use crate::gui::app::cditor_v2_view::{CditorV2View, CditorViewState};
use crate::gui::block::BlockDragOverlaySnapshot;
use crate::gui::input::BlockDragSelectionController;

use super::geometry::{drop_target_for_document_y_from_rects, parent_drop_target_from_rects};

const GUTTER_DRAG_AUTO_SCROLL_EDGE_PX: f64 = 40.0;
const GUTTER_DRAG_AUTO_SCROLL_MAX_STEP_PX: f64 = 24.0;
const GUTTER_DRAG_AUTO_SCROLL_TICK_MS: u64 = 16;

pub(in crate::gui::app) fn gutter_drag_auto_scroll_delta(
    pointer_y: f64,
    viewport_height: f64,
) -> f64 {
    if viewport_height <= GUTTER_DRAG_AUTO_SCROLL_EDGE_PX * 2.0 {
        return 0.0;
    }
    if pointer_y < GUTTER_DRAG_AUTO_SCROLL_EDGE_PX {
        -((GUTTER_DRAG_AUTO_SCROLL_EDGE_PX - pointer_y) / GUTTER_DRAG_AUTO_SCROLL_EDGE_PX)
            .clamp(0.0, 1.0)
            * GUTTER_DRAG_AUTO_SCROLL_MAX_STEP_PX
    } else if pointer_y > viewport_height - GUTTER_DRAG_AUTO_SCROLL_EDGE_PX {
        ((pointer_y - (viewport_height - GUTTER_DRAG_AUTO_SCROLL_EDGE_PX))
            / GUTTER_DRAG_AUTO_SCROLL_EDGE_PX)
            .clamp(0.0, 1.0)
            * GUTTER_DRAG_AUTO_SCROLL_MAX_STEP_PX
    } else {
        0.0
    }
}

impl CditorV2View {
    pub(crate) fn gutter_mouse_down_from_gui(
        &mut self,
        block_id: BlockId,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus, cx);
        self.hovered_block_id = Some(block_id);
        self.action_block_id = Some(block_id);
        self.text_drag_selection = None;
        self.block_drag_selection = BlockDragSelectionController::default();
        self.gutter_block_drag = Some(GutterBlockDragState::new(
            block_id,
            DragPoint::new(f32::from(position.x), f32::from(position.y)),
        ));
        if let CditorViewState::Ready(runtime) = &mut self.state {
            runtime.focus_block(block_id);
        }
        cx.notify();
    }

    pub(in crate::gui::app) fn update_gutter_block_drag(
        &mut self,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(mut drag) = self.gutter_block_drag else {
            return false;
        };
        let point = DragPoint::new(f32::from(position.x), f32::from(position.y));
        let threshold_changed = drag.update_position(point);
        let auto_scrolled = if drag.exceeded_threshold {
            self.apply_gutter_drag_auto_scroll(f64::from(position.y))
        } else {
            false
        };
        self.gutter_block_drag = Some(drag);
        let target_changed = self.refresh_gutter_block_drag_target();
        if self.should_continue_gutter_drag_auto_scroll() {
            self.schedule_gutter_drag_auto_scroll_tick(cx);
        }
        if threshold_changed || target_changed || auto_scrolled {
            cx.notify();
        }
        true
    }

    fn refresh_gutter_block_drag_target(&mut self) -> bool {
        let Some(mut drag) = self.gutter_block_drag else {
            return false;
        };
        let pointer_document_y = f64::from(drag.current_position.y)
            + self
                .ready_runtime_ref()
                .map(|runtime| runtime.scroll.global_scroll_top)
                .unwrap_or(0.0);
        let target = drag
            .exceeded_threshold
            .then(|| self.drop_target_for_document_y(drag.block_id, pointer_document_y))
            .flatten();
        let target_changed = drag.target != target;
        drag.target = target;
        self.gutter_block_drag = Some(drag);
        target_changed
    }

    fn should_continue_gutter_drag_auto_scroll(&self) -> bool {
        let Some(drag) = self.gutter_block_drag else {
            return false;
        };
        if !drag.exceeded_threshold {
            return false;
        }
        let Some(runtime) = self.ready_runtime_ref() else {
            return false;
        };
        gutter_drag_auto_scroll_delta(
            f64::from(drag.current_position.y),
            runtime.scroll.viewport_height,
        )
        .abs()
            >= f64::EPSILON
    }

    fn schedule_gutter_drag_auto_scroll_tick(&mut self, cx: &mut Context<Self>) {
        if self.gutter_drag_auto_scroll_scheduled {
            return;
        }
        self.gutter_drag_auto_scroll_scheduled = true;
        let tick = cx.background_spawn(async move {
            std::thread::sleep(Duration::from_millis(GUTTER_DRAG_AUTO_SCROLL_TICK_MS));
        });
        cx.spawn(async move |view, cx| {
            let _ = tick.await;
            let _ = view.update(cx, |view, cx| {
                view.gutter_drag_auto_scroll_scheduled = false;
                let changed = view.tick_gutter_drag_auto_scroll();
                if changed {
                    cx.notify();
                }
                if view.should_continue_gutter_drag_auto_scroll() {
                    view.schedule_gutter_drag_auto_scroll_tick(cx);
                }
            });
        })
        .detach();
    }

    fn tick_gutter_drag_auto_scroll(&mut self) -> bool {
        let Some(drag) = self.gutter_block_drag else {
            return false;
        };
        if !drag.exceeded_threshold {
            return false;
        }
        let auto_scrolled = self.apply_gutter_drag_auto_scroll(f64::from(drag.current_position.y));
        let target_changed = self.refresh_gutter_block_drag_target();
        auto_scrolled || target_changed
    }

    fn apply_gutter_drag_auto_scroll(&mut self, pointer_y: f64) -> bool {
        let CditorViewState::Ready(runtime) = &mut self.state else {
            return false;
        };
        let delta = gutter_drag_auto_scroll_delta(pointer_y, runtime.scroll.viewport_height);
        if delta.abs() < f64::EPSILON {
            return false;
        }
        let before = runtime.scroll.global_scroll_top;
        runtime.scroll_by_delta(delta).is_ok() && runtime.scroll.global_scroll_top != before
    }

    pub(in crate::gui::app) fn commit_gutter_block_drag(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(drag) = self.gutter_block_drag.take() else {
            self.gutter_drag_auto_scroll_scheduled = false;
            return false;
        };
        self.gutter_drag_auto_scroll_scheduled = false;
        let Some(target) = drag.target.filter(|_| drag.exceeded_threshold) else {
            cx.notify();
            return true;
        };
        let horizontal_delta = drag.current_position.x - drag.start_position.x;
        let parent_target = (horizontal_delta >= crate::gui::block::chrome::BLOCK_INDENT_STEP_PX)
            .then(|| {
                parent_drop_target_from_rects(&self.projected_block_rects, drag.block_id, target)
            })
            .flatten();
        if let CditorViewState::Ready(runtime) = &mut self.state {
            let moved = if let Some(parent_target) = parent_target {
                runtime
                    .move_block_subtree_to_parent(
                        drag.block_id,
                        Some(parent_target.parent_id),
                        parent_target.sibling_index,
                    )
                    .unwrap_or(false)
            } else {
                runtime
                    .move_block_subtree_before(drag.block_id, target.insert_before_block_id)
                    .unwrap_or(false)
            };
            if moved {
                self.mark_dirty(cx);
            }
        }
        cx.notify();
        true
    }

    fn drop_target_for_document_y(
        &self,
        source_block_id: BlockId,
        document_y: f64,
    ) -> Option<BlockDropTarget> {
        drop_target_for_document_y_from_rects(
            &self.projected_block_rects,
            source_block_id,
            document_y,
        )
    }

    pub(in crate::gui::app) fn block_drag_overlay_snapshot(
        &self,
    ) -> Option<BlockDragOverlaySnapshot> {
        let drag = self.gutter_block_drag?;
        let target = drag.target.filter(|_| drag.exceeded_threshold)?;
        let (y_px, indent_px) = match target.insert_before_block_id {
            Some(block_id) => self
                .projected_block_rects
                .iter()
                .find(|rect| rect.block_id == block_id)
                .map(|rect| (rect.document_top as f32, rect.indent_px))?,
            None => self
                .projected_block_rects
                .last()
                .map(|rect| (rect.document_bottom as f32, rect.indent_px))?,
        };
        Some(BlockDragOverlaySnapshot {
            y_px,
            indent_px,
            visible: true,
        })
    }
}
