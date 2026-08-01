use std::collections::{HashMap, HashSet};

use cditor_whiteboard::{Scene, WhiteboardView};
use cditor_whiteboard_gpui::DrafftChromeMode;
use gpui::{AppContext, Context};

use crate::editor_view::CditorV2View;
use crate::theme::GuiTheme;
use cditor_core::ids::BlockId;
use cditor_core::rich_text::{BlockPayload, BlockPayloadView};
use cditor_runtime::EditorViewProjection;
use cditor_runtime::WorkCost;

use super::WhiteboardBackendEntity;
use super::style::whiteboard_style_fn;

struct WhiteboardThumbnailEntry {
    content_version: u64,
    theme: GuiTheme,
    entity: Option<WhiteboardBackendEntity>,
    checked_out: bool,
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
        read_only: bool,
        mut admit_entity: impl FnMut(WorkCost) -> bool,
        cx: &mut Context<CditorV2View>,
    ) -> bool {
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
            .retain(|block_id, entry| visible_ids.contains(block_id) || entry.checked_out);

        let mut deferred = false;
        for (block_id, content_version, scene_json) in visible {
            if let Some(entry) = self.entries.get_mut(&block_id) {
                if entry.checked_out {
                    entry.content_version = content_version;
                    entry.theme = theme;
                    continue;
                }
                if entry.content_version == content_version && entry.theme == theme {
                    continue;
                }
                if entry
                    .entity
                    .as_ref()
                    .and_then(|entity| entity.scene_json(cx))
                    .is_some_and(|current| current == scene_json)
                {
                    entry.content_version = content_version;
                    entry.theme = theme;
                    continue;
                }
            }
            if !admit_entity(whiteboard_entity_cost(scene_json.len())) {
                deferred = true;
                continue;
            }
            let entity = super::backend::try_create_drafft_board(
                scene_json,
                read_only,
                DrafftChromeMode::BottomToolbarOnly,
                block_id,
                cx,
            )
            .unwrap_or_else(|_| legacy_entity(scene_json, cx));
            self.entries.insert(
                block_id,
                WhiteboardThumbnailEntry {
                    content_version,
                    theme,
                    entity: Some(entity),
                    checked_out: false,
                },
            );
        }
        deferred
    }

    pub(crate) fn entity(&self, block_id: BlockId) -> Option<WhiteboardBackendEntity> {
        self.entries
            .get(&block_id)
            .and_then(|entry| entry.entity.clone())
    }

    pub(crate) fn checkout_drafft(&mut self, block_id: BlockId) -> Option<WhiteboardBackendEntity> {
        let entry = self.entries.get_mut(&block_id)?;
        if !entry
            .entity
            .as_ref()
            .is_some_and(|entity| entity.is_drafft())
        {
            return None;
        }
        entry.checked_out = true;
        entry.entity.take()
    }

    pub(crate) fn checkin(&mut self, block_id: BlockId, entity: WhiteboardBackendEntity) {
        if let Some(entry) = self.entries.get_mut(&block_id)
            && entry.checked_out
        {
            entry.entity = Some(entity);
            entry.checked_out = false;
        }
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
    }

    pub(crate) fn invalidate(&mut self, block_id: BlockId) {
        if !self
            .entries
            .get(&block_id)
            .is_some_and(|entry| entry.checked_out)
        {
            self.entries.remove(&block_id);
        }
    }
}

fn legacy_entity(scene_json: &str, cx: &mut Context<CditorV2View>) -> WhiteboardBackendEntity {
    let scene = Scene::from_json(scene_json);
    let style = whiteboard_style_fn();
    WhiteboardBackendEntity::Legacy(
        cx.new(|board_cx| WhiteboardView::new_read_only(scene, style, board_cx)),
    )
}

fn whiteboard_entity_cost(scene_bytes: usize) -> WorkCost {
    WorkCost {
        sync_ms: (0.15 + scene_bytes as f64 / (128.0 * 1024.0)).clamp(0.15, 2.0),
        entity_creates: 1,
        window_diff_items: 1,
        ..WorkCost::ZERO
    }
}

#[cfg(test)]
mod cost_tests {
    use super::*;

    #[test]
    fn entity_cost_accounts_for_window_diff_and_bounds_scene_parse_time() {
        let small = whiteboard_entity_cost(1024);
        let large = whiteboard_entity_cost(10 * 1024 * 1024);
        assert_eq!(small.entity_creates, 1);
        assert_eq!(small.window_diff_items, 1);
        assert!(large.sync_ms > small.sync_ms);
        assert_eq!(large.sync_ms, 2.0);
    }
}
