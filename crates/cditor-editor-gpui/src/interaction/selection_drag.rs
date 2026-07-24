use std::time::Duration;

use gpui::{AppContext, Context, Pixels, Point};

use crate::app::cditor_v2_view::{CditorV2View, CditorViewState};
use crate::text::ParleyTextPosition;
use cditor_core::edit::{DocumentSelection, TextPosition};
use cditor_core::ids::BlockId;

const TEXT_DRAG_AUTO_SCROLL_TICK_MS: u64 = 16;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct GuiTextDragSelection {
    pub(crate) anchor_block_id: BlockId,
    pub(crate) anchor_position: ParleyTextPosition,
    pub(crate) pointer_position: Point<Pixels>,
}

impl CditorV2View {
    fn text_position_at_point(
        &self,
        position: Point<Pixels>,
    ) -> Option<(BlockId, ParleyTextPosition)> {
        let session = self.ready_session()?;
        let viewport = session.layout_viewport().ok()?;
        let block_id = self
            .infer_document_viewport_origin()
            .and_then(|viewport_origin| {
                let document_y =
                    f32::from(position.y) as f64 - viewport_origin.y + viewport.global_scroll_top;
                projected_block_at_document_y(&self.interaction.projected_block_rects, document_y)
            })
            .or_else(|| {
                current_layout_block_at_viewport_y(
                    &self.interaction.projected_block_rects,
                    &self.cache.text_layouts,
                    session,
                    position.y,
                )
            })?;
        self.text_position_for_block_at_position(block_id, position)
            .map(|position| (block_id, position))
    }

    pub(crate) fn update_text_drag_selection(
        &mut self,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let Some(drag) = self.interaction.text_drag_selection else {
            return;
        };
        self.interaction.text_drag_selection = Some(GuiTextDragSelection {
            pointer_position: position,
            ..drag
        });
        let Some((focus_block_id, focus_position)) = self.text_position_at_point(position) else {
            self.schedule_text_drag_auto_scroll(cx);
            return;
        };
        if let CditorViewState::Ready(session) = &self.state {
            let _ = session.dispatch(cditor_editor_protocol::command::CommandEnvelope::new(
                cditor_editor_protocol::command::CditorCommand::SetDocumentSelection {
                    selection: DocumentSelection {
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
                    },
                },
                cditor_editor_protocol::command::CommandSource::Toolbar,
            ));
            cx.stop_propagation();
            cx.notify();
        }
        self.schedule_text_drag_auto_scroll(cx);
    }

    pub(crate) fn finish_text_drag_selection(&mut self) {
        self.interaction.text_drag_selection = None;
        self.interaction.text_drag_auto_scroll_scheduled = false;
    }

    fn schedule_text_drag_auto_scroll(&mut self, cx: &mut Context<Self>) {
        let Some(drag) = self.interaction.text_drag_selection else {
            return;
        };
        let Some(viewport) = self
            .ready_session()
            .and_then(|session| session.layout_viewport().ok())
        else {
            return;
        };
        let pointer_y = self
            .infer_document_viewport_origin()
            .map(|origin| text_drag_pointer_viewport_y(drag.pointer_position.y, origin.y))
            .unwrap_or(f64::NAN);
        let delta = crate::interaction::gutter_drag_metrics::gutter_drag_auto_scroll_delta(
            pointer_y,
            viewport.viewport_height,
        );
        if delta.abs() < f64::EPSILON {
            return;
        }
        if self.interaction.text_drag_auto_scroll_scheduled {
            return;
        }
        self.interaction.text_drag_auto_scroll_scheduled = true;
        let tick = cx.background_spawn(async move {
            std::thread::sleep(Duration::from_millis(TEXT_DRAG_AUTO_SCROLL_TICK_MS));
        });
        cx.spawn(async move |view, cx| {
            let _ = tick.await;
            let _ = view.update(cx, |view, cx| {
                view.interaction.text_drag_auto_scroll_scheduled = false;
                if view.tick_text_drag_auto_scroll(cx) {
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn tick_text_drag_auto_scroll(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(drag) = self.interaction.text_drag_selection else {
            return false;
        };
        let Some(pointer_y) = self
            .infer_document_viewport_origin()
            .map(|origin| text_drag_pointer_viewport_y(drag.pointer_position.y, origin.y))
        else {
            return false;
        };
        let Some(session) = self.ready_session() else {
            return false;
        };
        let Ok(viewport) = session.layout_viewport() else {
            return false;
        };
        let delta = crate::interaction::gutter_drag_metrics::gutter_drag_auto_scroll_delta(
            pointer_y,
            viewport.viewport_height,
        );
        if delta.abs() < f64::EPSILON
            || !session
                .request_scroll_delta(delta)
                .is_ok_and(|outcome| outcome.changed)
        {
            return false;
        }
        self.update_text_drag_selection(drag.pointer_position, cx);
        true
    }

    pub(crate) fn finish_block_drag_selection(&mut self) {
        let _ = self.interaction.block_drag_selection.finish();
    }
}

fn text_drag_pointer_viewport_y(window_y: Pixels, viewport_origin_y: f64) -> f64 {
    f64::from(window_y) - viewport_origin_y
}

fn projected_block_at_document_y(
    rects: &[crate::interaction::geometry::ProjectedBlockRect],
    document_y: f64,
) -> Option<BlockId> {
    rects
        .iter()
        .find(|rect| rect.document_top <= document_y && document_y < rect.document_bottom)
        .map(|rect| rect.block_id)
}

fn current_layout_block_at_viewport_y(
    rects: &[crate::interaction::geometry::ProjectedBlockRect],
    layouts: &std::collections::HashMap<BlockId, crate::text::RichTextPlatformLayout>,
    session: &cditor_session::EditorSessionHandle,
    viewport_y: Pixels,
) -> Option<BlockId> {
    rects.iter().find_map(|rect| {
        let layout = layouts.get(&rect.block_id)?;
        if session
            .surface_version(cditor_core::ids::SurfaceId::Block(rect.block_id))
            .ok()
            .flatten()?
            .content_version
            != layout.content_version
        {
            return None;
        }
        (layout.bounds.top() <= viewport_y && viewport_y < layout.bounds.bottom())
            .then_some(rect.block_id)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interaction::geometry::ProjectedBlockRect;
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
                crate::text::test_platform_layout(
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

        let session = cditor_session::EditorSession::new(runtime, false).into_handle();
        assert_eq!(
            current_layout_block_at_viewport_y(&rects, &layouts, &session, px(210.0)),
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
