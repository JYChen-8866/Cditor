use gpui::{Context, Window};

use cditor_core::ids::BlockId;

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
        let result = self
            .ready_runtime()
            .ok_or_else(|| "runtime is not ready".to_owned())
            .and_then(|runtime| {
                runtime.focus_block_at_offset(block_id, 0)?;
                runtime.toggle_block_fold(block_id)
            });
        match result {
            Ok(true) => {
                let visible_blocks = self
                    .ready_runtime_ref()
                    .map(|runtime| {
                        runtime
                            .visible_index
                            .visible_block_ids
                            .iter()
                            .copied()
                            .collect::<std::collections::HashSet<_>>()
                    })
                    .unwrap_or_default();
                self.text_layouts
                    .retain(|candidate, _| visible_blocks.contains(candidate));
                self.mark_dirty(cx);
                cx.notify();
                true
            }
            Ok(false) => false,
            Err(error) => {
                self.save_status = crate::gui::persistence::EditorSaveStatus::Failed(error);
                cx.notify();
                false
            }
        }
    }
}
