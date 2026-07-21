use gpui::{AnyElement, IntoElement, Styled, div, prelude::FluentBuilder, px, rgb};

use crate::gui::GuiTheme;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlockDragOverlaySnapshot {
    pub y_px: f32,
    pub start_x_px: f32,
    pub end_x_px: f32,
    pub visible: bool,
}

pub fn render_block_drag_overlay(
    snapshot: BlockDragOverlaySnapshot,
    theme: GuiTheme,
) -> AnyElement {
    div()
        .absolute()
        .left(px(snapshot.start_x_px))
        .w(px((snapshot.end_x_px - snapshot.start_x_px).max(1.0)))
        .top(px(snapshot.y_px))
        .h(px(2.0))
        .rounded(px(1.0))
        .bg(rgb(theme.action_accent))
        .when(!snapshot.visible, |this| this.hidden())
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drag_overlay_snapshot_is_absolute_and_does_not_encode_height() {
        let snapshot = BlockDragOverlaySnapshot {
            y_px: 120.0,
            start_x_px: 64.0,
            end_x_px: 852.0,
            visible: true,
        };

        assert_eq!(snapshot.y_px, 120.0);
        assert_eq!(snapshot.start_x_px, 64.0);
        assert_eq!(snapshot.end_x_px, 852.0);
        assert!(snapshot.visible);
    }
}
