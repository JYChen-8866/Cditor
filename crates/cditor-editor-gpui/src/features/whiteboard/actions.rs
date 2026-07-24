use std::rc::Rc;

use cditor_whiteboard::{Scene, WhiteboardView};
use gpui::{AppContext, Context};

use crate::editor_view::{CditorV2View, CditorViewState};
use crate::features::whiteboard::whiteboard_style_fn;
use crate::overlay::WhiteboardEditorSession;
use crate::theme::GuiTheme;
use cditor_core::ids::BlockId;
use cditor_editor_protocol::command::{CommandEnvelope, CommandSource, EditorCommand};

impl CditorV2View {
    pub(crate) fn open_whiteboard_editor_from_gui(
        &mut self,
        block_id: BlockId,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(scene_json) = self
            .ready_session()
            .and_then(|session| session.whiteboard_scene(block_id).ok().flatten())
        else {
            return false;
        };
        let readonly = self.status.readonly;
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
                        let result = match &mut view.state {
                            CditorViewState::Ready(session) => session
                                .dispatch_with_snapshot(CommandEnvelope::new(
                                    EditorCommand::UpdateWhiteboardScene {
                                        block_id,
                                        scene_json,
                                    },
                                    CommandSource::Toolbar,
                                ))
                                .map(|snapshot| (snapshot.outcome.changed(), snapshot.revision))
                                .unwrap_or((false, 0)),
                            _ => (false, 0),
                        };
                        if result.0 {
                            // Skip thumbnail invalidation during editing — the editor
                            // is fullscreen so the thumbnail is not visible. We rebuild
                            // the thumbnail on close instead.
                            view.mark_dirty_at_revision(
                                cditor_core::edit::ChangeOrigin::User,
                                result.1,
                                cx,
                            );
                        }
                    });
                }));
            }
            board
        });
        self.features.whiteboard_editor = Some(WhiteboardEditorSession { block_id, board });
        cx.notify();
        true
    }

    pub(crate) fn close_whiteboard_editor_from_gui(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(session) = self.features.whiteboard_editor.take() else {
            return false;
        };
        // Flush the final scene state back to the runtime payload before dropping
        // the board entity. This ensures edits made since the last on_change fire
        // are not lost.
        let scene_json = session.board.read(cx).scene().to_json();
        if let Some(session_handle) = self.ready_session() {
            let result = session_handle.dispatch_with_snapshot(CommandEnvelope::new(
                EditorCommand::UpdateWhiteboardScene {
                    block_id: session.block_id,
                    scene_json,
                },
                CommandSource::Toolbar,
            ));
            if let Ok(snapshot) = result
                && snapshot.outcome.changed()
            {
                self.cache
                    .whiteboard_thumbnails
                    .invalidate(session.block_id);
                self.mark_dirty_at_revision(
                    cditor_core::edit::ChangeOrigin::User,
                    snapshot.revision,
                    cx,
                );
            }
        }
        cx.notify();
        true
    }
}
