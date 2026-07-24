use std::time::Duration;

use gpui::{AppContext, Context, Pixels, Point, Window};

use crate::app::cditor_v2_view::{CditorV2View, CditorViewState};
use crate::block::BlockDragOverlaySnapshot;
use crate::input::BlockDragSelectionController;
use cditor_core::block::{BlockDropTarget, DragPoint, GutterBlockDragState};
use cditor_core::ids::BlockId;

use super::geometry::drop_target_for_document_y_from_rects;

use super::gutter_drag_metrics::{
    GUTTER_DRAG_AUTO_SCROLL_TICK_MS, gutter_drag_auto_scroll_delta, gutter_drag_guideline_geometry,
    gutter_drag_pointer_document_y,
};

fn gutter_drag_pointer_viewport_y_for_view(view: &CditorV2View, window_y: f32) -> f64 {
    f64::from(window_y)
        - view
            .infer_document_viewport_origin()
            .map(|origin| origin.y)
            .unwrap_or(0.0)
}

fn gutter_drag_pointer_document_y_for_view(view: &CditorV2View, window_y: f32) -> f64 {
    gutter_drag_pointer_document_y(
        window_y,
        view.infer_document_viewport_origin()
            .map(|origin| origin.y)
            .unwrap_or(0.0),
        view.ready_session()
            .and_then(|session| session.layout_viewport().ok())
            .map(|snapshot| snapshot.global_scroll_top)
            .unwrap_or(0.0),
    )
}

impl CditorV2View {
    pub(crate) fn gutter_mouse_down_from_gui(
        &mut self,
        block_id: BlockId,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus.editor, cx);
        self.interaction.hovered_block_id = Some(block_id);
        self.interaction.action_block_id = Some(block_id);
        self.overlay.gutter_toolbar_block_id = Some(block_id);
        self.overlay.block_transform_menu_open = false;
        self.overlay.color_menu_open = false;
        self.interaction.text_drag_selection = None;
        self.interaction.block_drag_selection = BlockDragSelectionController::default();
        self.interaction.gutter_block_drag = Some(GutterBlockDragState::new(
            block_id,
            DragPoint::new(f32::from(position.x), f32::from(position.y)),
        ));
        if let CditorViewState::Ready(session) = &self.state {
            let _ = session.dispatch(cditor_editor_protocol::command::CommandEnvelope::new(
                cditor_editor_protocol::command::CditorCommand::FocusBlock { block_id },
                cditor_editor_protocol::command::CommandSource::Toolbar,
            ));
        }
        cx.notify();
    }

    pub(in crate::app) fn update_gutter_block_drag(
        &mut self,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(mut drag) = self.interaction.gutter_block_drag else {
            return false;
        };
        let point = DragPoint::new(f32::from(position.x), f32::from(position.y));
        let threshold_changed = drag.update_position(point);
        if drag.exceeded_threshold {
            self.overlay.gutter_toolbar_block_id = None;
            self.overlay.block_transform_menu_open = false;
            self.overlay.color_menu_open = false;
        }
        let auto_scrolled = if drag.exceeded_threshold {
            self.apply_gutter_drag_auto_scroll(gutter_drag_pointer_viewport_y_for_view(
                self,
                f32::from(position.y),
            ))
        } else {
            false
        };
        self.interaction.gutter_block_drag = Some(drag);
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
        let Some(mut drag) = self.interaction.gutter_block_drag else {
            return false;
        };
        let pointer_document_y =
            gutter_drag_pointer_document_y_for_view(self, drag.current_position.y);
        let target = drag
            .exceeded_threshold
            .then(|| self.drop_target_for_document_y(drag.block_id, pointer_document_y))
            .flatten();
        let target_changed = drag.target != target;
        drag.target = target;
        self.interaction.gutter_block_drag = Some(drag);
        target_changed
    }

    fn should_continue_gutter_drag_auto_scroll(&self) -> bool {
        let Some(drag) = self.interaction.gutter_block_drag else {
            return false;
        };
        if !drag.exceeded_threshold {
            return false;
        }
        let Some(viewport) = self
            .ready_session()
            .and_then(|session| session.layout_viewport().ok())
        else {
            return false;
        };
        gutter_drag_auto_scroll_delta(
            gutter_drag_pointer_viewport_y_for_view(self, drag.current_position.y),
            viewport.viewport_height,
        )
        .abs()
            >= f64::EPSILON
    }

    fn schedule_gutter_drag_auto_scroll_tick(&mut self, cx: &mut Context<Self>) {
        if self.interaction.gutter_drag_auto_scroll_scheduled {
            return;
        }
        self.interaction.gutter_drag_auto_scroll_scheduled = true;
        let tick = cx.background_spawn(async move {
            std::thread::sleep(Duration::from_millis(GUTTER_DRAG_AUTO_SCROLL_TICK_MS));
        });
        cx.spawn(async move |view, cx| {
            let _ = tick.await;
            let _ = view.update(cx, |view, cx| {
                view.interaction.gutter_drag_auto_scroll_scheduled = false;
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
        let Some(drag) = self.interaction.gutter_block_drag else {
            return false;
        };
        if !drag.exceeded_threshold {
            return false;
        }
        let auto_scrolled = self.apply_gutter_drag_auto_scroll(
            gutter_drag_pointer_viewport_y_for_view(self, drag.current_position.y),
        );
        let target_changed = self.refresh_gutter_block_drag_target();
        auto_scrolled || target_changed
    }

    fn apply_gutter_drag_auto_scroll(&mut self, pointer_y: f64) -> bool {
        let CditorViewState::Ready(session) = &self.state else {
            return false;
        };
        let Ok(viewport) = session.layout_viewport() else {
            return false;
        };
        let delta = gutter_drag_auto_scroll_delta(pointer_y, viewport.viewport_height);
        if delta.abs() < f64::EPSILON {
            return false;
        }
        session
            .request_scroll_delta(delta)
            .is_ok_and(|outcome| outcome.changed)
    }

    fn drop_target_for_document_y(
        &self,
        source_block_id: BlockId,
        document_y: f64,
    ) -> Option<BlockDropTarget> {
        drop_target_for_document_y_from_rects(
            &self.interaction.projected_block_rects,
            source_block_id,
            document_y,
        )
    }

    pub(in crate::app) fn block_drag_overlay_snapshot(&self) -> Option<BlockDragOverlaySnapshot> {
        let drag = self.interaction.gutter_block_drag?;
        if !drag.exceeded_threshold {
            return None;
        }

        let window_start_global_y = self.interaction.projected_block_rects.first()?.document_top;
        let guideline = gutter_drag_guideline_geometry(
            &self.interaction.projected_block_rects,
            drag.target?,
            window_start_global_y,
        )?;

        Some(BlockDragOverlaySnapshot {
            y_px: guideline.y_px,
            start_x_px: guideline.start_x_px,
            end_x_px: guideline.end_x_px,
            visible: true,
        })
    }
}
