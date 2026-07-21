use cditor_core::ids::BlockId;
use gpui::Context;

use crate::gui::app::CditorV2View;
use crate::gui::block::code::highlight::CODE_THEME_ITEMS;

impl CditorV2View {
    pub(crate) fn toggle_code_theme_menu_from_gui(
        &mut self,
        block_id: BlockId,
        cx: &mut Context<Self>,
    ) {
        self.code_language_edit = None;
        self.code_theme_menu_block_id = if self.code_theme_menu_block_id == Some(block_id) {
            None
        } else {
            Some(block_id)
        };
        cx.notify();
    }

    pub(crate) fn select_code_theme_from_gui(
        &mut self,
        theme_name: &'static str,
        cx: &mut Context<Self>,
    ) -> bool {
        if !CODE_THEME_ITEMS.iter().any(|item| item.id == theme_name) {
            return false;
        }
        let changed = self.code_highlight_theme != theme_name;
        self.code_highlight_theme = theme_name;
        self.code_theme_menu_block_id = None;
        cx.notify();
        changed
    }

    pub(crate) fn dismiss_code_theme_menu(&mut self, cx: &mut Context<Self>) -> bool {
        let dismissed = self.code_theme_menu_block_id.take().is_some();
        if dismissed {
            cx.notify();
        }
        dismissed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_selectable_theme_name_is_unique() {
        let mut names = CODE_THEME_ITEMS
            .iter()
            .map(|item| item.id)
            .collect::<Vec<_>>();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), CODE_THEME_ITEMS.len());
    }
}
