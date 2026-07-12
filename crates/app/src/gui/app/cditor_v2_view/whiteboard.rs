use std::rc::Rc;

use ding_board::{Scene, WhiteboardView};
use gpui::{AppContext, Context};

use crate::gui::GuiTheme;
use crate::gui::app::cditor_v2_view::{CditorV2View, CditorViewState};
use crate::gui::block::whiteboard_style_fn;
use crate::gui::overlay::WhiteboardEditorSession;
use cditor_core::ids::BlockId;
use cditor_core::rich_text::BlockPayload;

impl CditorV2View {
    pub(crate) fn open_whiteboard_editor_from_gui(
        &mut self,
        block_id: BlockId,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(scene_json) = self.ready_runtime_ref().and_then(|runtime| {
            runtime.block_payload_record(block_id).and_then(|payload| {
                let BlockPayload::Whiteboard(whiteboard) = &payload.payload else {
                    return None;
                };
                Some(whiteboard.scene_json.clone())
            })
        }) else {
            return false;
        };
        let readonly = self.readonly;
        let style = whiteboard_style_fn(GuiTheme::light());
        let host = cx.entity().downgrade();
        let board = cx.new(|board_cx| {
            let scene = Scene::from_json(&scene_json);
            let mut board = if readonly {
                WhiteboardView::new_read_only(scene, style, board_cx)
            } else {
                WhiteboardView::new(scene, style, board_cx)
            };
            if !readonly {
                board.set_on_change(Rc::new(move |scene_json, _window, app| {
                    let _ = host.update(app, |view, cx| {
                        let changed = match &mut view.state {
                            CditorViewState::Ready(runtime) => runtime
                                .update_whiteboard_scene_json(block_id, scene_json)
                                .unwrap_or(false),
                            _ => false,
                        };
                        if changed {
                            // Skip thumbnail invalidation during editing — the editor
                            // is fullscreen so the thumbnail is not visible. We rebuild
                            // the thumbnail on close instead.
                            view.mark_dirty(cx);
                        }
                    });
                }));
            }
            board
        });
        self.whiteboard_editor = Some(WhiteboardEditorSession { block_id, board });
        cx.notify();
        true
    }

    pub(crate) fn close_whiteboard_editor_from_gui(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(session) = self.whiteboard_editor.take() else {
            return false;
        };
        // Flush the final scene state back to the runtime payload before dropping
        // the board entity. This ensures edits made since the last on_change fire
        // are not lost.
        let scene_json = session.board.read(cx).scene().to_json();
        if let Some(runtime) = self.ready_runtime() {
            let changed = runtime
                .update_whiteboard_scene_json(session.block_id, scene_json)
                .unwrap_or(false);
            if changed {
                self.whiteboard_thumbnails.invalidate(session.block_id);
                self.mark_dirty(cx);
            }
        }
        cx.notify();
        true
    }
}
