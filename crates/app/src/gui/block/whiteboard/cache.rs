use std::collections::{HashMap, HashSet};

use ding_board::{Scene, WhiteboardView};
use gpui::{AppContext, Context, Entity};

use crate::gui::GuiTheme;
use crate::gui::app::CditorV2View;
use cditor_core::ids::BlockId;
use cditor_core::rich_text::{BlockPayload, BlockPayloadView};
use cditor_runtime::EditorViewProjection;

use super::style::whiteboard_style_fn;

struct WhiteboardThumbnailEntry {
    content_version: u64,
    theme: GuiTheme,
    entity: Entity<WhiteboardView>,
}

#[derive(Default)]
pub(crate) struct WhiteboardThumbnailCache {
    entries: HashMap<BlockId, WhiteboardThumbnailEntry>,
}

impl WhiteboardThumbnailCache {
    pub(crate) fn sync_visible_window(
        &mut self,
        projection: &EditorViewProjection,
        theme: GuiTheme,
        cx: &mut Context<CditorV2View>,
    ) {
        let visible = projection
            .blocks
            .iter()
            .filter_map(|block| {
                let BlockPayloadView::Loaded(payload) = &block.payload else {
                    return None;
                };
                let BlockPayload::Whiteboard(whiteboard) = &payload.payload else {
                    return None;
                };
                Some((
                    block.block_id,
                    payload.content_version,
                    whiteboard.scene_json.as_str(),
                ))
            })
            .collect::<Vec<_>>();
        let visible_ids = visible
            .iter()
            .map(|(block_id, _, _)| *block_id)
            .collect::<HashSet<_>>();
        self.entries
            .retain(|block_id, _| visible_ids.contains(block_id));

        for (block_id, content_version, scene_json) in visible {
            if self.entries.get(&block_id).is_some_and(|entry| {
                entry.content_version == content_version && entry.theme == theme
            }) {
                continue;
            }
            let scene = Scene::from_json(scene_json);
            let style = whiteboard_style_fn(theme);
            let entity = cx.new(|board_cx| WhiteboardView::new_read_only(scene, style, board_cx));
            self.entries.insert(
                block_id,
                WhiteboardThumbnailEntry {
                    content_version,
                    theme,
                    entity,
                },
            );
        }
    }

    pub(crate) fn entity(&self, block_id: BlockId) -> Option<Entity<WhiteboardView>> {
        self.entries
            .get(&block_id)
            .map(|entry| entry.entity.clone())
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
    }

    pub(crate) fn invalidate(&mut self, block_id: BlockId) {
        self.entries.remove(&block_id);
    }
}
