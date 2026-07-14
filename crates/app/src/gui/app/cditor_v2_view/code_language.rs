use cditor_core::ids::BlockId;
use gpui::{Context, Window};

use crate::gui::app::cditor_v2_view::{CditorV2View, GuiPlatformInputTarget};
use crate::gui::input::{
    CodeLanguageEditAction, CodeLanguageEditKeyResult, CodeLanguageEditState,
    CodeLanguagePopupPlacement, apply_code_language_action,
};

impl CditorV2View {
    pub(crate) fn toggle_code_language_dropdown_from_gui(
        &mut self,
        block_id: BlockId,
        language: Option<&str>,
        pointer_y_px: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .code_language_edit
            .as_ref()
            .is_some_and(|edit| edit.block_id == block_id)
        {
            self.cancel_code_language_edit(cx);
            window.focus(&self.focus, cx);
        } else {
            self.start_code_language_edit_from_gui(block_id, language, pointer_y_px, window, cx);
        }
    }

    pub(crate) fn dismiss_code_language_dropdown_from_gui(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let dismissed = self.cancel_code_language_edit(cx);
        if dismissed {
            window.focus(&self.focus, cx);
        }
        dismissed
    }

    pub(crate) fn start_code_language_edit_from_gui(
        &mut self,
        block_id: BlockId,
        language: Option<&str>,
        pointer_y_px: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.code_theme_menu_block_id = None;
        window.focus(&self.code_language_focus, cx);
        self.platform_input_target = Some(GuiPlatformInputTarget::code_language(block_id));
        let placement = code_language_popup_placement(pointer_y_px, window);
        self.code_language_edit = Some(CodeLanguageEditState::new_dropdown_with_placement(
            block_id, language, placement,
        ));
        cx.notify();
    }

    pub(crate) fn commit_code_language_edit(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(edit) = self.code_language_edit.take() else {
            return false;
        };
        if self.platform_input_target == Some(GuiPlatformInputTarget::code_language(edit.block_id))
        {
            self.platform_input_target = None;
        }
        let changed = self
            .ready_runtime()
            .and_then(|runtime| {
                runtime
                    .set_code_block_language(edit.block_id, edit.normalized_draft())
                    .ok()
            })
            .unwrap_or(false);
        if changed {
            self.mark_dirty(cx);
        }
        changed
    }

    pub(crate) fn select_code_language_from_gui(
        &mut self,
        block_id: BlockId,
        language: String,
        cx: &mut Context<Self>,
    ) {
        self.code_language_edit = Some(CodeLanguageEditState {
            block_id,
            original: String::new(),
            draft: language,
            is_open: false,
            selected_index: 0,
            scroll_start: 0,
            placement: CodeLanguagePopupPlacement::Below,
            caret_offset: 0,
            marked_range: None,
        });
        self.platform_input_target = Some(GuiPlatformInputTarget::code_language(block_id));
        self.commit_code_language_edit(cx);
        cx.notify();
    }

    pub(crate) fn apply_code_language_action_from_gui(
        &mut self,
        action: CodeLanguageEditAction,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(edit) = self.code_language_edit.as_mut() else {
            return false;
        };
        match apply_code_language_action(edit, action) {
            CodeLanguageEditKeyResult::Commit => {
                self.commit_code_language_edit(cx);
                true
            }
            CodeLanguageEditKeyResult::Cancel => self.cancel_code_language_edit(cx),
            CodeLanguageEditKeyResult::Changed => {
                cx.notify();
                true
            }
            CodeLanguageEditKeyResult::Ignored => false,
        }
    }

    pub(crate) fn cancel_code_language_edit(&mut self, cx: &mut Context<Self>) -> bool {
        let had_edit = self.code_language_edit.take().is_some();
        if had_edit {
            if self
                .platform_input_target
                .is_some_and(|target| target.is_code_language_for(target.block_id()))
            {
                self.platform_input_target = None;
            }
            cx.notify();
        }
        had_edit
    }

    pub(crate) fn scroll_code_language_suggestions_from_gui(
        &mut self,
        delta_rows: isize,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(edit) = self.code_language_edit.as_mut() else {
            return false;
        };
        let changed = edit.scroll_suggestions(delta_rows);
        if changed {
            cx.notify();
        }
        changed
    }
}

fn code_language_popup_placement(pointer_y_px: f32, window: &Window) -> CodeLanguagePopupPlacement {
    const POPUP_MARGIN_PX: f32 = 12.0;
    const POPUP_ESTIMATED_HEIGHT_PX: f32 = 300.0;

    let viewport_height = f32::from(window.viewport_size().height);
    let below = viewport_height - pointer_y_px - POPUP_MARGIN_PX;
    let above = pointer_y_px - POPUP_MARGIN_PX;
    if below < POPUP_ESTIMATED_HEIGHT_PX && above > below {
        CodeLanguagePopupPlacement::Above
    } else {
        CodeLanguagePopupPlacement::Below
    }
}
