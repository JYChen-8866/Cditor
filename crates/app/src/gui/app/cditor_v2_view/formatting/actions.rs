use cditor_core::ids::BlockId;
use cditor_core::rich_text::InlineMark;

use crate::gui::overlay::{BlockTransformAction, InlineFormatAction};

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
        let mark = inline_mark_for_toolbar_action(action);
        let gutter_block_id = (!has_text_selection).then_some(self.gutter_toolbar_block_id);
        let changed = self
            .ready_runtime()
            .and_then(|runtime| {
                let Some(block_id) = gutter_block_id.flatten() else {
                    return runtime.toggle_inline_mark_on_selection(mark).ok();
                };
                let text_len = runtime
                    .block_payload_record(block_id)
                    .map(|payload| payload.plain_text().len())?;
                if text_len == 0 {
                    return Some(false);
                }
                runtime
                    .set_document_text_selection(block_id, 0, block_id, text_len)
                    .ok()?;
                runtime.toggle_inline_mark_on_selection(mark).ok()
            })
            .unwrap_or(false);
        if changed {
            self.mark_dirty(cx);
            cx.notify();
        }
        changed
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
        let result = self
            .ready_runtime()
            .ok_or_else(|| "runtime is not ready".to_owned())
            .and_then(|runtime| {
                if runtime.focused_block_id() != Some(block_id) {
                    runtime.focus_block_at_offset(block_id, 0)?;
                }
                runtime.convert_focused_block_kind(action.kind())
            });
        match result {
            Ok(true) => {
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

pub(super) fn inline_mark_for_toolbar_action(action: InlineFormatAction) -> InlineMark {
    match action {
        InlineFormatAction::Bold => InlineMark::Bold,
        InlineFormatAction::Italic => InlineMark::Italic,
        InlineFormatAction::Underline => InlineMark::Underline,
        InlineFormatAction::Strike => InlineMark::Strike,
        InlineFormatAction::Code => InlineMark::Code,
    }
}
