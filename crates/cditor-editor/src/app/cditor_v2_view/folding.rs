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
        if self.readonly {
            return false;
        }
        window.focus(&self.focus, cx);
        let command = self.ready_runtime_ref().and_then(|runtime| {
            matches!(
                runtime.block_kind(block_id),
                Some(cditor_core::rich_text::RichBlockKind::Heading { .. })
            )
            .then(|| {
                if runtime.is_block_folded(block_id) {
                    CditorCommand::UnfoldHeading
                } else {
                    CditorCommand::FoldHeading
                }
            })
        });
        let Some(command) = command else {
            return false;
        };
        if let Some(runtime) = self.ready_runtime() {
            let _ = runtime.dispatch(cditor_editor_protocol::command::CommandEnvelope::new(
                cditor_editor_protocol::command::CditorCommand::SetDocumentSelection {
                    selection: cditor_core::edit::DocumentSelection::caret(
                        cditor_core::edit::TextPosition::downstream(block_id, 0),
                    ),
                },
                cditor_editor_protocol::command::CommandSource::Toolbar,
            ));
        }
        let result = self.dispatch_command(command, CommandSource::Toolbar, cx);
        match result {
            Ok(outcome) if outcome.status == CommandOutcomeStatus::Applied => {
                let visible_blocks = self
                    .ready_runtime_ref()
                    .map(|runtime| {
                        runtime
                            .visible_block_ids()
                            .iter()
                            .copied()
                            .collect::<std::collections::HashSet<_>>()
                    })
                    .unwrap_or_default();
                self.text_layouts
                    .retain(|candidate, _| visible_blocks.contains(candidate));
                cx.notify();
                true
            }
            Ok(_) => false,
            Err(error) => {
                self.save_status = crate::persistence::EditorSaveStatus::Failed(error.to_string());
                cx.notify();
                false
            }
        }
    }
}
