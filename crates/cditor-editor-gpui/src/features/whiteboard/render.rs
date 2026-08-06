use cditor_component::SvgIcon;
use gpui::{
    div, px, rgb, AnyElement, Entity, InteractiveElement, IntoElement, ParentElement, Styled,
};

use crate::editor_view::CditorV2View;
use crate::skeleton::{SkeletonItem, SkeletonVariant};
use crate::theme::GuiTheme;
use cditor_core::ids::BlockId;

use super::cache::WhiteboardThumbnailCache;
use super::WHITEBOARD_THUMBNAIL_HEIGHT_PX;

const WHITEBOARD_FRAME_RADIUS_PX: f32 = 6.0;

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
        .relative()
        .w_full()
        .h(px(WHITEBOARD_THUMBNAIL_HEIGHT_PX))
        .rounded(px(WHITEBOARD_FRAME_RADIUS_PX))
        .border_1()
        .border_color(rgb(theme.border))
        .bg(rgb(theme.page))
        .overflow_hidden();
    if let Some(board) = cache.entity(block_id) {
        let is_drafft = board.is_drafft();
        frame = frame
            .child(board.render())
            .child(expand_button(block_id, theme, view.clone()));
        if !is_drafft {
            frame = frame.cursor_pointer().on_mouse_down(
                gpui::MouseButton::Left,
                move |event, _window, cx| {
                    if !should_open_editor(event.click_count) {
                        return;
                    }
                    view.update(cx, |view, cx| {
                        view.open_whiteboard_editor_from_gui(block_id, cx);
                    });
                    cx.stop_propagation();
                },
            );
        }
    } else {
        frame = frame.child(
            SkeletonItem::new(SkeletonVariant::Image)
                .height_px(WHITEBOARD_THUMBNAIL_HEIGHT_PX)
                .render(theme),
        );
    }
    frame.into_any_element()
}

fn expand_button(
    block_id: BlockId,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
) -> impl IntoElement {
    const FULLSCREEN: &[u8] = include_bytes!("../../../../../assets/icons/fullscreen.svg");

    div()
        .id(("whiteboard-expand", block_id))
        .absolute()
        .top(px(10.0))
        .right(px(10.0))
        .w(px(30.0))
        .h(px(30.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(5.0))
        .bg(rgb(theme.surface))
        .border_1()
        .border_color(rgb(theme.border))
        .text_color(rgb(theme.text))
        .cursor_pointer()
        .hover(move |style| style.bg(rgb(theme.hover_surface)))
        .on_mouse_down(gpui::MouseButton::Left, move |_event, _window, cx| {
            view.update(cx, |view, cx| {
                view.open_whiteboard_editor_from_gui(block_id, cx);
            });
            cx.stop_propagation();
        })
        .child(
            SvgIcon::new("whiteboard-fullscreen-icon", FULLSCREEN)
                .color(rgb(theme.text))
                .size(px(16.0)),
        )
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
        assert_eq!(WHITEBOARD_THUMBNAIL_HEIGHT_PX, 464.0);
        assert_eq!(WHITEBOARD_FRAME_RADIUS_PX, 6.0);
    }

    #[test]
    fn fullscreen_button_uses_the_shared_svg_asset() {
        const FULLSCREEN: &[u8] = include_bytes!("../../../../../assets/icons/fullscreen.svg");
        assert!(std::str::from_utf8(FULLSCREEN).unwrap().contains("<svg"));
    }
}
