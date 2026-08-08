use gpui::{Context, PlatformTextEditAction, Window};

use cditor_editor_protocol::command::{CditorCommand, CommandSource};

use crate::editor_view::CditorV2View;

/// Maps UIKit's edit-menu vocabulary to the editor's stable command protocol.
pub(crate) fn command_for_native_edit_action(action: PlatformTextEditAction) -> CditorCommand {
    match action {
        PlatformTextEditAction::Copy => CditorCommand::CopySelection,
        PlatformTextEditAction::Cut => CditorCommand::CutSelection,
        PlatformTextEditAction::Paste => CditorCommand::PasteClipboard,
        PlatformTextEditAction::SelectAll => CditorCommand::SelectAll,
    }
}

impl CditorV2View {
    /// Queries the editor command state without requiring GPUI focus or a
    /// software-keyboard session.
    pub(crate) fn allows_native_edit_action(&self, action: PlatformTextEditAction) -> bool {
        self.sdk_command_state(&command_for_native_edit_action(action))
            .enabled
    }

    /// Executes a native edit-menu action through the same command router used
    /// by desktop menus and hardware-keyboard bindings.
    pub(crate) fn perform_native_edit_action(
        &mut self,
        action: PlatformTextEditAction,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let command = command_for_native_edit_action(action);
        if !self.allows_native_edit_action(action) {
            return false;
        }
        self.dispatch_command(command, CommandSource::ContextMenu, cx)
            .is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::command_for_native_edit_action;
    use cditor_editor_protocol::command::CditorCommand;
    use gpui::PlatformTextEditAction;

    #[test]
    fn native_actions_use_stable_editor_commands() {
        assert_eq!(
            command_for_native_edit_action(PlatformTextEditAction::Copy),
            CditorCommand::CopySelection
        );
        assert_eq!(
            command_for_native_edit_action(PlatformTextEditAction::Cut),
            CditorCommand::CutSelection
        );
        assert_eq!(
            command_for_native_edit_action(PlatformTextEditAction::Paste),
            CditorCommand::PasteClipboard
        );
        assert_eq!(
            command_for_native_edit_action(PlatformTextEditAction::SelectAll),
            CditorCommand::SelectAll
        );
    }
}
