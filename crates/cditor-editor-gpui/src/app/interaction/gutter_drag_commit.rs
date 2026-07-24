use gpui::Context;

use crate::app::cditor_v2_view::CditorV2View;
use cditor_editor_protocol::command::{CommandSource, EditorCommand};

use super::geometry::parent_drop_target_from_rects;

impl CditorV2View {
    pub(in crate::app) fn commit_gutter_block_drag(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(drag) = self.interaction.gutter_block_drag.take() else {
            self.interaction.gutter_drag_auto_scroll_scheduled = false;
            return false;
        };
        if drag.exceeded_threshold {
            clear_committed_gutter_action(&mut self.interaction.action_block_id, drag.block_id);
            self.overlay.gutter_toolbar_block_id = None;
            self.overlay.block_transform_menu_open = false;
            self.overlay.color_menu_open = false;
        }
        self.interaction.gutter_drag_auto_scroll_scheduled = false;
        let Some(target) = drag.target.filter(|_| drag.exceeded_threshold) else {
            cx.notify();
            return true;
        };
        let horizontal_delta = drag.current_position.x - drag.start_position.x;
        let parent_target = (horizontal_delta >= crate::block::chrome::BLOCK_INDENT_STEP_PX)
            .then(|| {
                parent_drop_target_from_rects(
                    &self.interaction.projected_block_rects,
                    drag.block_id,
                    target,
                )
            })
            .flatten();
        let command = if let Some(parent_target) = parent_target {
            EditorCommand::MoveBlockToParent {
                block_id: drag.block_id,
                parent_id: Some(parent_target.parent_id),
                sibling_index: parent_target.sibling_index,
            }
        } else {
            EditorCommand::MoveBlockBefore {
                block_id: drag.block_id,
                before_block_id: target.insert_before_block_id,
            }
        };
        let _ = self.dispatch_command(command, CommandSource::Toolbar, cx);
        cx.notify();
        true
    }
}

fn clear_committed_gutter_action(action_block_id: &mut Option<u64>, committed_block_id: u64) {
    if *action_block_id == Some(committed_block_id) {
        *action_block_id = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn committing_gutter_drag_clears_matching_action_root() {
        let mut action_block_id = Some(7);

        clear_committed_gutter_action(&mut action_block_id, 7);

        assert_eq!(action_block_id, None);
    }

    #[test]
    fn committing_gutter_drag_preserves_newer_action_root() {
        let mut action_block_id = Some(8);

        clear_committed_gutter_action(&mut action_block_id, 7);

        assert_eq!(action_block_id, Some(8));
    }
}
