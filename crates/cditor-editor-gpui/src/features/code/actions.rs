use std::time::Duration;

use cditor_core::ids::BlockId;
use cditor_editor_protocol::command::{CditorCommand, CommandOutcomeStatus, CommandSource};
use gpui::Context;

use crate::editor_view::CditorV2View;

const COPY_FEEDBACK_DURATION: Duration = Duration::from_millis(1_500);

pub(crate) fn copy_code_block_from_gui(
    view: &mut CditorV2View,
    block_id: BlockId,
    cx: &mut Context<CditorV2View>,
) {
    if matches!(
        view.dispatch_command(
            CditorCommand::CopyBlockText { block_id },
            CommandSource::Toolbar,
            cx,
        ),
        Ok(outcome) if outcome.status == CommandOutcomeStatus::Applied
    ) {
        view.overlay.code_copy_feedback_block_id = Some(block_id);
        view.overlay.code_copy_feedback_generation =
            view.overlay.code_copy_feedback_generation.wrapping_add(1);
        let generation = view.overlay.code_copy_feedback_generation;
        let dismiss_after = cx.background_executor().timer(COPY_FEEDBACK_DURATION);
        cx.spawn(async move |view, cx| {
            let _ = dismiss_after.await;
            let _ = view.update(cx, |view, cx| {
                if copy_feedback_matches(
                    view.overlay.code_copy_feedback_block_id,
                    view.overlay.code_copy_feedback_generation,
                    block_id,
                    generation,
                ) {
                    view.overlay.code_copy_feedback_block_id = None;
                    cx.notify();
                }
            });
        })
        .detach();
        crate::overlays::show_toast(view, "已将代码拷贝到剪贴板", Duration::from_secs(3), cx);
    }
}

pub(super) fn copy_feedback_matches(
    current_block_id: Option<BlockId>,
    current_generation: u64,
    expected_block_id: BlockId,
    expected_generation: u64,
) -> bool {
    current_block_id == Some(expected_block_id) && current_generation == expected_generation
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_feedback_only_expires_for_the_matching_block_and_generation() {
        assert!(copy_feedback_matches(Some(7), 3, 7, 3));
        assert!(!copy_feedback_matches(Some(8), 3, 7, 3));
        assert!(!copy_feedback_matches(Some(7), 4, 7, 3));
        assert!(!copy_feedback_matches(None, 3, 7, 3));
    }
}
