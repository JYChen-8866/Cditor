use gpui::{Context, Window};

use cditor_core::ids::BlockId;

use cditor_editor_protocol::command::{CditorCommand, CommandOutcomeStatus, CommandSource};

use super::CditorV2View;

impl CditorV2View {
    pub(crate) fn toggle_block_fold_from_gui(
        &mut self,
        block_id: BlockId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.status.readonly {
            return false;
        }
        window.focus(&self.focus.editor, cx);
        let command = self
            .ready_session()
            .and_then(|session| session.text_block_context(block_id).ok().flatten())
            .and_then(|context| {
                matches!(
                    context.kind,
                    cditor_core::rich_text::RichBlockKind::Heading { .. }
                )
                .then(|| {
                    if context.folded {
                        CditorCommand::UnfoldHeading
                    } else {
                        CditorCommand::FoldHeading
                    }
                })
            });
        let Some(command) = command else {
            return false;
        };
        if let Some(session) = self.ready_session() {
            let _ = session.dispatch_with_snapshot(
                cditor_editor_protocol::command::CommandEnvelope::new(
                    cditor_editor_protocol::command::CditorCommand::SetDocumentSelection {
                        selection: cditor_core::edit::DocumentSelection::caret(
                            cditor_core::edit::TextPosition::downstream(block_id, 0),
                        ),
                    },
                    cditor_editor_protocol::command::CommandSource::Toolbar,
                ),
            );
        }
        let result = self.dispatch_command(command, CommandSource::Toolbar, cx);
        match result {
            Ok(outcome) if outcome.status == CommandOutcomeStatus::Applied => {
                let cached_block_ids = self.text_layouts.keys().copied().collect::<Vec<_>>();
                let visible_blocks = self
                    .ready_session()
                    .and_then(|session| session.visible_block_subset(&cached_block_ids).ok())
                    .unwrap_or_default();
                self.text_layouts
                    .retain(|candidate, _| visible_blocks.contains(candidate));
                cx.notify();
                true
            }
            Ok(_) => false,
            Err(error) => {
                self.status.save_status =
                    crate::persistence::EditorSaveStatus::Failed(error.to_string());
                cx.notify();
                false
            }
        }
    }
}
