use std::time::Duration;

use cditor_core::ids::BlockId;
use cditor_editor_protocol::command::{CditorCommand, CommandOutcomeStatus, CommandSource};
use gpui::Context;

use crate::editor_view::CditorV2View;

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
        crate::overlays::show_toast(view, "已将代码拷贝到剪贴板", Duration::from_secs(3), cx);
    }
}
