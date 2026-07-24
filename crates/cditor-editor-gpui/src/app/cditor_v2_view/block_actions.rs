use cditor_core::ids::BlockId;
use gpui::{Context, Window};

use crate::app::cditor_v2_view::CditorV2View;
use crate::persistence::EditorSaveStatus;
use cditor_editor_protocol::command::{CditorCommand, CommandOutcomeStatus, CommandSource};

pub(in crate::app) fn block_focus_offset_after_missed_hit_test(
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
        if self.status.readonly {
            return false;
        }
        window.focus(&self.focus.editor, cx);
        match self.dispatch_command(
            CditorCommand::InsertParagraphAfterBlock { block_id },
            CommandSource::Toolbar,
            cx,
        ) {
            Ok(outcome) if outcome.status == CommandOutcomeStatus::Applied => {
                self.overlay.slash_menu = None;
                cx.notify();
                true
            }
            Ok(_) => false,
            Err(error) => {
                self.status.save_status = EditorSaveStatus::Failed(error.to_string());
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
        if self.status.readonly {
            return false;
        }
        match self.dispatch_command(
            CditorCommand::DeleteBlock { block_id },
            CommandSource::Toolbar,
            cx,
        ) {
            Ok(outcome) if outcome.status == CommandOutcomeStatus::Applied => {
                if self.overlay.gutter_toolbar_block_id == Some(block_id) {
                    self.overlay.gutter_toolbar_block_id = None;
                    self.overlay.block_transform_menu_open = false;
                    self.overlay.color_menu_open = false;
                }
                if self.interaction.action_block_id == Some(block_id) {
                    self.interaction.action_block_id = None;
                }
                cx.notify();
                true
            }
            Ok(_) => false,
            Err(error) => {
                self.status.save_status = EditorSaveStatus::Failed(error.to_string());
                cx.notify();
                false
            }
        }
    }
}
