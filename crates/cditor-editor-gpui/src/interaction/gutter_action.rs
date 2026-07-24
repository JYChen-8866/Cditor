use crate::editor_view::CditorV2View;

impl CditorV2View {
    pub(crate) fn clear_gutter_action(&mut self) {
        self.interaction.action_block_id = None;
        self.overlay.gutter_toolbar_block_id = None;
        self.overlay.block_transform_menu_open = false;
        self.overlay.color_menu_open = false;
        self.overlay.color_menu_hover_generation =
            self.overlay.color_menu_hover_generation.wrapping_add(1);
        self.interaction.gutter_block_drag = None;
        self.interaction.gutter_drag_auto_scroll_scheduled = false;
    }
}
