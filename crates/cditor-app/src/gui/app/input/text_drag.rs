use std::time::Duration;

use gpui::{AppContext, Context, Pixels, Point};

use crate::gui::app::cditor_v2_view::{CditorV2View, CditorViewState};
use crate::gui::text::ParleyTextPosition;
use cditor_core::edit::{DocumentSelection, TextPosition};
use cditor_core::ids::BlockId;

const TEXT_DRAG_AUTO_SCROLL_TICK_MS: u64 = 16;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::gui::app) struct GuiTextDragSelection {
    pub(in crate::gui::app) anchor_block_id: BlockId,
    pub(in crate::gui::app) anchor_position: ParleyTextPosition,
    pub(in crate::gui::app) pointer_position: Point<Pixels>,
}

impl CditorV2View {
    fn text_position_at_point(
        &self,
        position: Point<Pixels>,
    ) -> Option<(BlockId, ParleyTextPosition)> {
        let runtime = self.ready_runtime_ref()?;
        let block_id = self
            .infer_document_viewport_origin()
            .and_then(|viewport_origin| {
                let document_y = f32::from(position.y) as f64 - viewport_origin.y
                    + runtime.scroll.global_scroll_top;
                projected_block_at_document_y(&self.projected_block_rects, document_y)
            })
            .or_else(|| {
                current_layout_block_at_viewport_y(
                    &self.projected_block_rects,
                    &self.text_layouts,
                    runtime,
                    position.y,
                )
            })?;
        self.text_position_for_block_at_position(block_id, position)
            .map(|position| (block_id, position))
    }

    pub(in crate::gui::app) fn update_text_drag_selection(
        &mut self,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let Some(drag) = self.text_drag_selection else {
            return;
        };
        self.text_drag_selection = Some(GuiTextDragSelection {
            pointer_position: position,
            ..drag
        });
        let Some((focus_block_id, focus_position)) = self.text_position_at_point(position) else {
            self.schedule_text_drag_auto_scroll(cx);
            return;
        };
        if let CditorViewState::Ready(runtime) = &mut self.state {
            let _ = runtime.set_document_selection(DocumentSelection {
                anchor: TextPosition {
                    block_id: drag.anchor_block_id,
                    offset: drag.anchor_position.offset,
                    affinity: drag.anchor_position.affinity,
                },
                focus: TextPosition {
                    block_id: focus_block_id,
                    offset: focus_position.offset,
                    affinity: focus_position.affinity,
                },
            });
            cx.stop_propagation();
            cx.notify();
        }
        self.schedule_text_drag_auto_scroll(cx);
    }

    pub(in crate::gui::app) fn finish_text_drag_selection(&mut self) {
        self.text_drag_selection = None;
        self.text_drag_auto_scroll_scheduled = false;
    }

    fn schedule_text_drag_auto_scroll(&mut self, cx: &mut Context<Self>) {
        let Some(drag) = self.text_drag_selection else {
            return;
        };
        let Some(runtime) = self.ready_runtime_ref() else {
            return;
        };
        let pointer_y = self
            .infer_document_viewport_origin()
            .map(|origin| text_drag_pointer_viewport_y(drag.pointer_position.y, origin.y))
            .unwrap_or(f64::NAN);
        let delta =
            crate::gui::app::interaction::gutter_drag_metrics::gutter_drag_auto_scroll_delta(
                pointer_y,
                runtime.scroll.viewport_height,
            );
        if delta.abs() < f64::EPSILON {
            return;
        }
        if self.text_drag_auto_scroll_scheduled {
            return;
        }
        self.text_drag_auto_scroll_scheduled = true;
        let tick = cx.background_spawn(async move {
            std::thread::sleep(Duration::from_millis(TEXT_DRAG_AUTO_SCROLL_TICK_MS));
        });
        cx.spawn(async move |view, cx| {
            let _ = tick.await;
            let _ = view.update(cx, |view, cx| {
                view.text_drag_auto_scroll_scheduled = false;
                if view.tick_text_drag_auto_scroll(cx) {
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn tick_text_drag_auto_scroll(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(drag) = self.text_drag_selection else {
            return false;
        };
        let Some(pointer_y) = self
            .infer_document_viewport_origin()
            .map(|origin| text_drag_pointer_viewport_y(drag.pointer_position.y, origin.y))
        else {
            return false;
        };
        let Some(runtime) = self.ready_runtime() else {
            return false;
        };
        let delta =
            crate::gui::app::interaction::gutter_drag_metrics::gutter_drag_auto_scroll_delta(
                pointer_y,
                runtime.scroll.viewport_height,
            );
        if delta.abs() < f64::EPSILON || runtime.scroll_by_delta(delta).is_err() {
            return false;
        }
        self.update_text_drag_selection(drag.pointer_position, cx);
        true
    }

    pub(in crate::gui::app) fn finish_block_drag_selection(&mut self) {
        let _ = self.block_drag_selection.finish();
    }
}

fn text_drag_pointer_viewport_y(window_y: Pixels, viewport_origin_y: f64) -> f64 {
    f64::from(window_y) - viewport_origin_y
}

fn projected_block_at_document_y(
    rects: &[crate::gui::app::interaction::geometry::ProjectedBlockRect],
    document_y: f64,
) -> Option<BlockId> {
    rects
        .iter()
        .find(|rect| rect.document_top <= document_y && document_y < rect.document_bottom)
        .map(|rect| rect.block_id)
}

fn current_layout_block_at_viewport_y(
    rects: &[crate::gui::app::interaction::geometry::ProjectedBlockRect],
    layouts: &std::collections::HashMap<BlockId, crate::gui::text::RichTextPlatformLayout>,
    runtime: &cditor_runtime::DocumentRuntime,
    viewport_y: Pixels,
) -> Option<BlockId> {
    rects.iter().find_map(|rect| {
        let layout = layouts.get(&rect.block_id)?;
        if runtime.block_content_version(rect.block_id)? != layout.content_version {
            return None;
        }
        (layout.bounds.top() <= viewport_y && viewport_y < layout.bounds.bottom())
            .then_some(rect.block_id)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::app::interaction::geometry::ProjectedBlockRect;
    use cditor_core::rich_text::{BlockPayloadRecord, RichBlockKind};
    use gpui::{Bounds, Size, point, px};

    fn rect(block_id: BlockId, top: f64, bottom: f64) -> ProjectedBlockRect {
        ProjectedBlockRect {
            block_id,
            visible_index: block_id as usize,
            depth: 0,
            document_top: top,
            document_bottom: bottom,
            indent_px: 0.0,
            text_origin_x_in_block_px: 0.0,
            text_origin_y_in_block_px: 0.0,
            text_width_px: 600.0,
            supports_children: false,
        }
    }

    #[test]
    fn projected_drag_hit_test_uses_half_open_ordered_block_bounds() {
        let rects = [rect(10, 100.0, 130.0), rect(20, 130.0, 160.0)];

        assert_eq!(projected_block_at_document_y(&rects, 100.0), Some(10));
        assert_eq!(projected_block_at_document_y(&rects, 129.99), Some(10));
        assert_eq!(projected_block_at_document_y(&rects, 130.0), Some(20));
        assert_eq!(projected_block_at_document_y(&rects, 160.0), None);
    }

    #[test]
    fn projected_drag_hit_test_does_not_target_blocks_outside_the_render_window() {
        let rects = [rect(10, 100.0, 130.0), rect(20, 130.0, 160.0)];

        assert_eq!(projected_block_at_document_y(&rects, 99.0), None);
        assert_eq!(projected_block_at_document_y(&rects, 260.0), None);
    }

    #[test]
    fn overlapping_layout_caches_follow_projection_order_not_hashmap_order() {
        let runtime = cditor_runtime::DocumentRuntime::from_payloads(
            1,
            vec![
                BlockPayloadRecord::rich_text(10, RichBlockKind::Paragraph, "first"),
                BlockPayloadRecord::rich_text(20, RichBlockKind::Paragraph, "second"),
            ],
            720.0,
        );
        let rects = [rect(10, 100.0, 130.0), rect(20, 130.0, 160.0)];
        let mut layouts = std::collections::HashMap::new();
        for (block_id, text) in [(20, "second"), (10, "first")] {
            layouts.insert(
                block_id,
                crate::gui::text::test_platform_layout(
                    block_id,
                    runtime.block_content_version(block_id).unwrap(),
                    text,
                    Bounds {
                        origin: point(px(100.0), px(200.0)),
                        size: Size {
                            width: px(500.0),
                            height: px(24.0),
                        },
                    },
                    None,
                ),
            );
        }

        assert_eq!(
            current_layout_block_at_viewport_y(&rects, &layouts, &runtime, px(210.0)),
            Some(10)
        );
    }

    #[test]
    fn edge_ticker_keeps_viewport_pointer_stable_while_document_endpoint_advances() {
        let rects = [rect(10, 100.0, 130.0), rect(20, 130.0, 160.0)];
        let pointer_y = text_drag_pointer_viewport_y(px(150.0), 100.0);
        assert_eq!(pointer_y, 50.0);
        assert_eq!(
            projected_block_at_document_y(&rects, pointer_y + 60.0),
            Some(10)
        );
        assert_eq!(
            projected_block_at_document_y(&rects, pointer_y + 90.0),
            Some(20)
        );
    }
}
