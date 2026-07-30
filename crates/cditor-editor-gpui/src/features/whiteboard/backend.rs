use cditor_core::ids::BlockId;
use cditor_editor_protocol::command::{CommandEnvelope, CommandSource, EditorCommand};
use gpui::{AnyElement, AppContext, Context, Entity, IntoElement};

use crate::editor_view::CditorV2View;
use cditor_whiteboard_drafft::DrafftChromeMode;

#[derive(Clone)]
pub(crate) enum WhiteboardBackendEntity {
    Legacy(Entity<cditor_whiteboard::WhiteboardView>),
    Drafft(Entity<cditor_whiteboard_drafft::DrafftBoardView>),
}

impl WhiteboardBackendEntity {
    pub(crate) fn render(&self) -> AnyElement {
        match self {
            Self::Legacy(entity) => entity.clone().into_any_element(),
            Self::Drafft(entity) => entity.clone().into_any_element(),
        }
    }

    pub(crate) fn scene_json(&self, cx: &gpui::App) -> Option<String> {
        match self {
            Self::Legacy(entity) => Some(entity.read(cx).scene().to_json()),
            Self::Drafft(entity) => entity.read(cx).scene_json().ok(),
        }
    }

    pub(crate) fn is_drafft(&self) -> bool {
        match self {
            Self::Legacy(_) => false,
            Self::Drafft(_) => true,
        }
    }

    pub(crate) fn set_drafft_chrome_mode(&self, mode: DrafftChromeMode, cx: &mut gpui::App) {
        if let Self::Drafft(entity) = self {
            entity.update(cx, |board, cx| {
                board.set_chrome_mode(mode);
                cx.notify();
            });
        }
    }
}

pub(crate) fn try_create_drafft_board(
    scene_json: &str,
    read_only: bool,
    chrome_mode: DrafftChromeMode,
    block_id: BlockId,
    cx: &mut Context<CditorV2View>,
) -> Result<WhiteboardBackendEntity, String> {
    register_fonts(cx)?;
    cditor_whiteboard_drafft::parse_document_json(scene_json)?;
    let scene_json = scene_json.to_owned();
    let entity = cx.new(|board_cx| {
        let mut board = cditor_whiteboard_drafft::DrafftBoardView::from_document_json(
            &scene_json,
            read_only,
            board_cx,
        )
        .expect("Drafft scene was validated before entity creation");
        board.set_chrome_mode(chrome_mode);
        board
    });
    if !read_only {
        let host = cx.entity().downgrade();
        entity.update(cx, |board, _| {
            board.set_on_change(std::rc::Rc::new(move |scene_json, app| {
                let _ = host.update(app, |view, cx| {
                    view.persist_drafft_scene(block_id, scene_json, cx);
                });
            }));
        });
    }
    let host = cx.entity().downgrade();
    entity.update(cx, |board, _| {
        board.set_on_focus_request(std::rc::Rc::new(move |app| {
            host.update(app, |view, cx| {
                view.focus_drafft_block_from_gui(block_id, cx)
            })
            .unwrap_or(false)
        }));
    });
    Ok(WhiteboardBackendEntity::Drafft(entity))
}

impl CditorV2View {
    fn focus_drafft_block_from_gui(&mut self, block_id: BlockId, cx: &mut Context<Self>) -> bool {
        if !self.commit_document_composition_before_external_focus(cx) {
            return false;
        }
        let Some(session) = self.ready_session() else {
            return false;
        };
        if session
            .dispatch(CommandEnvelope::new(
                EditorCommand::FocusBlock { block_id },
                CommandSource::Toolbar,
            ))
            .is_err()
        {
            return false;
        }
        let target = session
            .input_context()
            .ok()
            .and_then(|context| context.target);
        cx.notify();
        matches!(
            target,
            Some(cditor_runtime::InputTarget::ComplexBlock {
                block_id: focused_block_id
            }) if focused_block_id == block_id
        )
    }
}

fn register_fonts(cx: &mut Context<CditorV2View>) -> Result<(), String> {
    use std::sync::OnceLock;

    static REGISTERED: OnceLock<Result<(), String>> = OnceLock::new();
    REGISTERED
        .get_or_init(|| {
            cx.text_system()
                .add_fonts(cditor_whiteboard_drafft::bundled_fonts())
                .map_err(|error| error.to_string())
        })
        .clone()
}
