use cditor_core::ids::BlockId;
#[cfg(test)]
use cditor_core::rich_text::InlineMark;

use crate::overlay::{BlockTransformAction, InlineFormatAction};
use cditor_editor_protocol::command::{
    BlockTransform, CditorCommand, CommandEnvelope, CommandOutcomeStatus, CommandSource,
};

use super::super::CditorV2View;

impl CditorV2View {
    pub(crate) fn open_block_transform_menu_from_gui(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if self.block_transform_menu_open || self.gutter_toolbar_block_id.is_none() {
            return false;
        }
        self.block_transform_menu_open = true;
        self.color_menu_open = false;
        cx.notify();
        true
    }

    pub(crate) fn apply_inline_format_from_toolbar(
        &mut self,
        action: InlineFormatAction,
        has_text_selection: bool,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if self.readonly {
            return false;
        }
        let gutter_block_id = (!has_text_selection).then_some(self.gutter_toolbar_block_id);
        if let Some(block_id) = gutter_block_id.flatten() {
            let prepared = self
                .ready_session()
                .and_then(|session| {
                    let text_len = session
                        .text_block_context(block_id)
                        .ok()
                        .flatten()?
                        .text
                        .len();
                    if text_len == 0 {
                        return Some(false);
                    }
                    session
                        .dispatch_with_snapshot(CommandEnvelope::new(
                            CditorCommand::SetDocumentSelection {
                                selection: cditor_core::edit::DocumentSelection {
                                    anchor: cditor_core::edit::TextPosition::downstream(
                                        block_id, 0,
                                    ),
                                    focus: cditor_core::edit::TextPosition::downstream(
                                        block_id, text_len,
                                    ),
                                },
                            },
                            CommandSource::Toolbar,
                        ))
                        .ok()?;
                    Some(true)
                })
                .unwrap_or(false);
            if !prepared {
                return false;
            }
        }
        let command = command_for_inline_format(action);
        matches!(
            self.dispatch_command(command, CommandSource::Toolbar, cx),
            Ok(outcome) if outcome.status == CommandOutcomeStatus::Applied
        )
    }

    pub(crate) fn transform_block_from_toolbar(
        &mut self,
        block_id: BlockId,
        action: BlockTransformAction,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if self.readonly {
            return false;
        }
        let focused = self
            .ready_session()
            .ok_or_else(|| "runtime is not ready".to_owned())
            .and_then(|session| {
                if session
                    .document_snapshot()
                    .map_err(|error| error.to_string())?
                    .focused_block_id
                    != Some(block_id)
                {
                    session
                        .dispatch_with_snapshot(CommandEnvelope::new(
                            CditorCommand::SetDocumentSelection {
                                selection: cditor_core::edit::DocumentSelection::caret(
                                    cditor_core::edit::TextPosition::downstream(block_id, 0),
                                ),
                            },
                            CommandSource::Toolbar,
                        ))
                        .map_err(|error| error.to_string())?;
                }
                Ok(())
            });
        if let Err(error) = focused {
            self.save_status = crate::persistence::EditorSaveStatus::Failed(error);
            cx.notify();
            return false;
        }
        match self.dispatch_command(
            CditorCommand::TransformBlock(BlockTransform::Kind(action.kind())),
            CommandSource::Toolbar,
            cx,
        ) {
            Ok(outcome) => outcome.status == CommandOutcomeStatus::Applied,
            Err(error) => {
                self.save_status = crate::persistence::EditorSaveStatus::Failed(error.to_string());
                cx.notify();
                false
            }
        }
    }
}

#[cfg(test)]
pub(super) fn inline_mark_for_toolbar_action(action: InlineFormatAction) -> InlineMark {
    match action {
        InlineFormatAction::Bold => InlineMark::Bold,
        InlineFormatAction::Italic => InlineMark::Italic,
        InlineFormatAction::Underline => InlineMark::Underline,
        InlineFormatAction::Strike => InlineMark::Strike,
        InlineFormatAction::Code => InlineMark::Code,
    }
}

fn command_for_inline_format(action: InlineFormatAction) -> CditorCommand {
    match action {
        InlineFormatAction::Bold => CditorCommand::ToggleBold,
        InlineFormatAction::Italic => CditorCommand::ToggleItalic,
        InlineFormatAction::Underline => CditorCommand::ToggleUnderline,
        InlineFormatAction::Strike => CditorCommand::ToggleStrike,
        InlineFormatAction::Code => CditorCommand::ToggleInlineCode,
    }
}

#[cfg(test)]
mod command_tests {
    use super::*;

    #[test]
    fn toolbar_actions_map_to_the_same_commands_as_keyboard_and_sdk() {
        assert_eq!(
            command_for_inline_format(InlineFormatAction::Bold),
            CditorCommand::ToggleBold
        );
        assert_eq!(
            command_for_inline_format(InlineFormatAction::Strike),
            CditorCommand::ToggleStrike
        );
    }
}
