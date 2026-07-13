use cditor_core::ids::BlockId;
use gpui::{Context, Window};

use crate::gui::app::cditor_v2_view::{CditorV2View, CditorViewState};
use crate::gui::persistence::EditorSaveStatus;

pub(in crate::gui::app) fn block_focus_offset_after_missed_hit_test(
    focused_block_id: Option<BlockId>,
    target_block_id: BlockId,
    target_caret_offset: Option<usize>,
) -> usize {
    if focused_block_id == Some(target_block_id) {
        target_caret_offset.unwrap_or(0)
    } else {
        0
    }
}

impl CditorV2View {
    pub(crate) fn insert_paragraph_after_block_from_gui(
        &mut self,
        block_id: BlockId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.readonly {
            return false;
        }
        window.focus(&self.focus, cx);
        let result = match &mut self.state {
            CditorViewState::Ready(runtime) => runtime.insert_paragraph_after_block(block_id),
            CditorViewState::Loading { .. } | CditorViewState::LoadFailed { .. } => return false,
        };
        match result {
            Ok(_) => {
                self.slash_menu = None;
                self.mark_dirty(cx);
                cx.notify();
                true
            }
            Err(error) => {
                self.save_status = EditorSaveStatus::Failed(error);
                cx.notify();
                false
            }
        }
    }

    pub(crate) fn delete_block_from_gui(
        &mut self,
        block_id: BlockId,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.readonly {
            return false;
        }
        let result = match &mut self.state {
            CditorViewState::Ready(runtime) => runtime.delete_block_by_id(block_id),
            CditorViewState::Loading { .. } | CditorViewState::LoadFailed { .. } => return false,
        };
        match result {
            Ok(true) => {
                if self.gutter_toolbar_block_id == Some(block_id) {
                    self.gutter_toolbar_block_id = None;
                    self.block_transform_menu_open = false;
                    self.color_menu_open = false;
                }
                if self.action_block_id == Some(block_id) {
                    self.action_block_id = None;
                }
                self.mark_dirty(cx);
                cx.notify();
                true
            }
            Ok(false) => false,
            Err(error) => {
                self.save_status = EditorSaveStatus::Failed(error);
                cx.notify();
                false
            }
        }
    }
}
