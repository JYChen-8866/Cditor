use crate::editor_view::{CditorV2View, OverlayUiState};
use cditor_core::ids::BlockId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GutterToolbarTransition {
    PointerDown,
    ClickReleased(BlockId),
    DragReleased,
    Dismissed,
}

fn apply_gutter_toolbar_transition(
    overlay: &mut OverlayUiState,
    transition: GutterToolbarTransition,
) {
    overlay.gutter_toolbar_block_id = match transition {
        GutterToolbarTransition::ClickReleased(block_id) => Some(block_id),
        GutterToolbarTransition::PointerDown
        | GutterToolbarTransition::DragReleased
        | GutterToolbarTransition::Dismissed => None,
    };
    overlay.block_transform_menu_open = false;
    overlay.color_menu_open = false;
    overlay.color_menu_hover_generation = overlay.color_menu_hover_generation.wrapping_add(1);
}

impl CditorV2View {
    pub(super) fn transition_gutter_toolbar(&mut self, transition: GutterToolbarTransition) {
        apply_gutter_toolbar_transition(&mut self.overlay, transition);
    }

    pub(crate) fn clear_gutter_action(&mut self) {
        self.interaction.action_block_id = None;
        self.transition_gutter_toolbar(GutterToolbarTransition::Dismissed);
        self.interaction.gutter_block_drag = None;
        self.interaction.gutter_drag_auto_scroll_scheduled = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gutter_toolbar_opens_only_after_a_click_release() {
        let mut overlay = OverlayUiState {
            gutter_toolbar_block_id: Some(5),
            block_transform_menu_open: true,
            color_menu_open: true,
            ..Default::default()
        };

        apply_gutter_toolbar_transition(&mut overlay, GutterToolbarTransition::PointerDown);
        assert_eq!(overlay.gutter_toolbar_block_id, None);
        assert!(!overlay.block_transform_menu_open);
        assert!(!overlay.color_menu_open);

        apply_gutter_toolbar_transition(&mut overlay, GutterToolbarTransition::ClickReleased(7));
        assert_eq!(overlay.gutter_toolbar_block_id, Some(7));

        apply_gutter_toolbar_transition(&mut overlay, GutterToolbarTransition::DragReleased);
        assert_eq!(overlay.gutter_toolbar_block_id, None);
    }
}
