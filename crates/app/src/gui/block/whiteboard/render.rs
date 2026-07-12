use gpui::{
    AnyElement, Entity, InteractiveElement, IntoElement, ParentElement, Styled, div, px, rgb,
};

use crate::gui::GuiTheme;
use crate::gui::app::CditorV2View;
use cditor_core::ids::BlockId;

use super::WHITEBOARD_THUMBNAIL_HEIGHT_PX;
use super::cache::WhiteboardThumbnailCache;

fn should_open_editor(click_count: usize) -> bool {
    click_count >= 2
}

pub(crate) fn render_whiteboard_thumbnail(
    block_id: BlockId,
    cache: &WhiteboardThumbnailCache,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
) -> AnyElement {
    let mut frame = div()
        .id(("whiteboard-thumbnail", block_id))
        .w_full()
        .h(px(WHITEBOARD_THUMBNAIL_HEIGHT_PX))
        .rounded(px(3.0))
        .border_1()
        .border_color(rgb(theme.border))
        .bg(rgb(theme.page))
        .cursor_pointer()
        .on_mouse_down(gpui::MouseButton::Left, move |event, _window, cx| {
            if !should_open_editor(event.click_count) {
                return;
            }
            let _ = view.update(cx, |view, cx| {
                view.open_whiteboard_editor_from_gui(block_id, cx);
            });
            cx.stop_propagation();
        })
        .overflow_hidden();
    if let Some(board) = cache.entity(block_id) {
        frame = frame.child(board);
    }
    frame.into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whiteboard_editor_opens_only_on_double_click() {
        assert!(!should_open_editor(1));
        assert!(should_open_editor(2));
        assert!(should_open_editor(3));
    }

    #[test]
    fn thumbnail_height_matches_the_stable_block_inner_box() {
        assert_eq!(WHITEBOARD_THUMBNAIL_HEIGHT_PX, 472.0);
    }
}
