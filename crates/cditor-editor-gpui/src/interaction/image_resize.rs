use gpui::{Context, Pixels, Point, Window};

use crate::editor_view::{CditorV2View, CditorViewState};
use crate::features::media::image_width_ratio_milli_for_width;
use crate::input::BlockDragSelectionController;
use crate::persistence::EditorSaveStatus;
use cditor_core::ids::BlockId;
use cditor_editor_protocol::command::{CommandSource, EditorCommand};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct GuiImageResizeDrag {
    pub(crate) block_id: BlockId,
    content_version: u64,
    start_pointer_x: f32,
    start_width_px: f32,
    pub(crate) current_width_px: f32,
    image_aspect_ratio: f32,
    has_caption: bool,
    max_width_px: f32,
}

impl CditorV2View {
    pub(crate) fn start_image_resize_from_gui(
        &mut self,
        block_id: BlockId,
        content_version: u64,
        current_width_px: f32,
        image_aspect_ratio: f32,
        has_caption: bool,
        max_width_px: f32,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.status.readonly {
            return;
        }
        window.focus(&self.focus.editor, cx);
        self.interaction.text_drag_selection = None;
        self.interaction.block_drag_selection = BlockDragSelectionController::default();
        self.clear_gutter_action();
        self.interaction.scrollbar_drag = None;
        self.interaction.hovered_block_id = Some(block_id);
        self.interaction.action_block_id = Some(block_id);
        self.interaction.image_resize_drag = Some(GuiImageResizeDrag {
            block_id,
            content_version,
            start_pointer_x: f32::from(position.x),
            start_width_px: current_width_px,
            current_width_px: current_width_px.clamp(max_width_px * 0.2, max_width_px),
            image_aspect_ratio,
            has_caption,
            max_width_px,
        });
        crate::diagnostics::image_resize::trace(
            "start",
            format_args!(
                "block={block_id} version={content_version} width={current_width_px:.2} max_width={max_width_px:.2} aspect={image_aspect_ratio:.5} caption={has_caption} pointer_x={:.2}",
                f32::from(position.x)
            ),
        );
        if let CditorViewState::Ready(session) = &self.state {
            let _ = session.dispatch(cditor_editor_protocol::command::CommandEnvelope::new(
                cditor_editor_protocol::command::CditorCommand::FocusBlock { block_id },
                cditor_editor_protocol::command::CommandSource::Toolbar,
            ));
        }
        cx.notify();
    }

    pub(crate) fn image_resize_preview(&self) -> Option<(BlockId, f32, f64)> {
        self.interaction.image_resize_drag.map(|drag| {
            let measured_height = crate::features::media::image_block_measured_height(
                drag.current_width_px / drag.image_aspect_ratio.max(f32::EPSILON),
                drag.has_caption,
            );
            (drag.block_id, drag.current_width_px, measured_height)
        })
    }

    pub(crate) fn update_image_resize_drag(
        &mut self,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(mut drag) = self.interaction.image_resize_drag else {
            return false;
        };
        let dx = f32::from(position.x) - drag.start_pointer_x;
        let next_width =
            (drag.start_width_px + dx).clamp(drag.max_width_px * 0.2, drag.max_width_px);
        if (next_width - drag.current_width_px).abs() < 0.5 {
            return true;
        }
        drag.current_width_px = next_width;
        let measured_height = crate::features::media::image_block_measured_height(
            next_width / drag.image_aspect_ratio.max(f32::EPSILON),
            drag.has_caption,
        );
        let applied = self.ready_session().map(|session| {
            session.apply_measured_block_height(
                drag.block_id,
                drag.content_version,
                measured_height,
            )
        });
        crate::diagnostics::image_resize::trace(
            "drag",
            format_args!(
                "block={} version={} pointer_x={:.2} width={next_width:.2} height={measured_height:.2} apply={applied:?}",
                drag.block_id,
                drag.content_version,
                f32::from(position.x),
            ),
        );
        self.interaction.image_resize_drag = Some(drag);
        cx.notify();
        true
    }

    pub(crate) fn commit_image_resize_drag(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(drag) = self.interaction.image_resize_drag else {
            return false;
        };
        clear_committed_image_resize_action(&mut self.interaction.action_block_id, drag.block_id);
        let ratio = image_width_ratio_milli_for_width(drag.current_width_px, drag.max_width_px);
        crate::diagnostics::image_resize::trace(
            "commit.begin",
            format_args!(
                "block={} preview_version={} width={:.2} ratio={} aspect={:.5} caption={}",
                drag.block_id,
                drag.content_version,
                drag.current_width_px,
                ratio,
                drag.image_aspect_ratio,
                drag.has_caption,
            ),
        );
        if let Err(error) = self.dispatch_command(
            EditorCommand::SetMediaWidthRatio {
                block_id: drag.block_id,
                ratio_milli: ratio,
            },
            CommandSource::Toolbar,
            cx,
        ) {
            crate::diagnostics::image_resize::trace(
                "commit.command_error",
                format_args!("block={} error={error}", drag.block_id),
            );
            self.status.save_status = EditorSaveStatus::Failed(error.to_string());
            self.interaction.image_resize_drag = None;
        } else {
            let measured_height = crate::features::media::image_block_measured_height(
                drag.current_width_px / drag.image_aspect_ratio.max(f32::EPSILON),
                drag.has_caption,
            );
            let committed_content_version = self.ready_session().and_then(|session| {
                session
                    .loaded_payload_record(drag.block_id)
                    .ok()
                    .flatten()
                    .map(|record| record.content_version)
            });
            if let Some(content_version) = committed_content_version {
                let applied = self.ready_session().map(|session| {
                    session.apply_measured_block_height(
                        drag.block_id,
                        content_version,
                        measured_height,
                    )
                });
                crate::diagnostics::image_resize::trace(
                    "commit.height",
                    format_args!(
                        "block={} committed_version={} width={:.2} height={measured_height:.2} apply={applied:?}",
                        drag.block_id, content_version, drag.current_width_px,
                    ),
                );
                if let Some(committed) = self.interaction.image_resize_drag.as_mut() {
                    committed.content_version = content_version;
                }
            }
            let timer = cx
                .background_executor()
                .timer(std::time::Duration::from_millis(32));
            cx.spawn(async move |view, cx| {
                timer.await;
                let _ = view.update(cx, |view, cx| {
                    if view
                        .interaction
                        .image_resize_drag
                        .is_some_and(|current| current.block_id == drag.block_id)
                    {
                        crate::diagnostics::image_resize::trace(
                            "commit.preview_clear",
                            format_args!("block={}", drag.block_id),
                        );
                        view.interaction.image_resize_drag = None;
                        cx.notify();
                    }
                });
            })
            .detach();
        }
        cx.notify();
        true
    }
}

fn clear_committed_image_resize_action(
    action_block_id: &mut Option<BlockId>,
    image_block_id: BlockId,
) {
    if *action_block_id == Some(image_block_id) {
        *action_block_id = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cditor_core::rich_text::{BlockPayload, BlockPayloadRecord, ImagePayload, RichBlockKind};
    use gpui::{AppContext, TestAppContext, point, px};

    #[test]
    fn committing_image_resize_clears_matching_action_root() {
        let mut action_block_id = Some(7);

        clear_committed_image_resize_action(&mut action_block_id, 7);

        assert_eq!(action_block_id, None);
    }

    #[test]
    fn committing_image_resize_preserves_newer_action_root() {
        let mut action_block_id = Some(8);

        clear_committed_image_resize_action(&mut action_block_id, 7);

        assert_eq!(action_block_id, Some(8));
    }

    #[gpui::test]
    fn dragging_applies_the_latest_image_height_before_the_next_projection(
        cx: &mut TestAppContext,
    ) {
        let runtime = cditor_runtime::DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord {
                block_id: 1,
                content_version: 1,
                kind: RichBlockKind::Image,
                payload: BlockPayload::Image(ImagePayload {
                    source: "asset://image".to_owned(),
                    ..ImagePayload::default()
                }),
            }],
            720.0,
        );
        let view = cx.new(|cx| CditorV2View::from_runtime(runtime, false, cx));

        view.update(cx, |view, cx| {
            view.interaction.image_resize_drag = Some(GuiImageResizeDrag {
                block_id: 1,
                content_version: 1,
                start_pointer_x: 0.0,
                start_width_px: 400.0,
                current_width_px: 400.0,
                image_aspect_ratio: 2.0,
                has_caption: false,
                max_width_px: 704.0,
            });

            assert!(view.update_image_resize_drag(point(px(200.0), px(0.0)), cx));
            let preview = view.image_resize_preview().unwrap();
            let frame = view
                .ready_session()
                .unwrap()
                .render_frame(cditor_session::RenderFrameRequest {
                    viewport_height: 720.0,
                    include_diagnostics: false,
                    height_correction_priority: crate::scroll::HeightCorrectionPriority::Normal,
                    min_scrollbar_thumb_height: 24.0,
                })
                .unwrap();
            let image = frame
                .projection
                .blocks
                .iter()
                .find(|block| block.block_id == 1)
                .unwrap();

            assert_eq!(image.layout.effective_height(), preview.2);
        });
    }
}
