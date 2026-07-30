use cditor_component::SvgIcon;
use gpui::{
    AnyElement, Entity, InteractiveElement, IntoElement, ParentElement, Styled, div, px, rgb,
};

use crate::editor_view::CditorV2View;
use crate::features::whiteboard::WhiteboardBackendEntity;
use crate::theme::GuiTheme;
use cditor_core::ids::BlockId;

#[derive(Clone)]
pub(crate) struct WhiteboardEditorSession {
    pub(crate) block_id: BlockId,
    pub(crate) board: WhiteboardBackendEntity,
}

pub(crate) fn render_whiteboard_editor(
    session: &WhiteboardEditorSession,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
) -> AnyElement {
    const MINISIZE: &[u8] = include_bytes!("../../../../assets/icons/minisize.svg");

    let close_view = view;
    div()
        .id(("whiteboard-editor-overlay", session.block_id))
        .absolute()
        .top_0()
        .right_0()
        .bottom_0()
        .left_0()
        .bg(rgb(theme.page))
        .on_mouse_down(gpui::MouseButton::Left, |_event, _window, cx| {
            cx.stop_propagation();
        })
        .child(session.board.render())
        .child(
            div()
                .id("whiteboard-editor-close")
                .absolute()
                .top(px(12.0))
                .right(px(12.0))
                .w(px(30.0))
                .h(px(30.0))
                .rounded(px(5.0))
                .flex()
                .items_center()
                .justify_center()
                .bg(rgb(theme.surface))
                .border_1()
                .border_color(rgb(theme.border))
                .text_color(rgb(theme.text))
                .cursor_pointer()
                .hover(move |style| style.bg(rgb(theme.hover_surface)))
                .on_mouse_down(gpui::MouseButton::Left, move |_event, _window, cx| {
                    close_view.update(cx, |view, cx| {
                        view.close_whiteboard_editor_from_gui(cx);
                    });
                    cx.stop_propagation();
                })
                .child(
                    SvgIcon::new("whiteboard-minisize-icon", MINISIZE)
                        .color(rgb(theme.text))
                        .size(px(16.0)),
                ),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    #[test]
    fn minimized_button_uses_the_shared_svg_asset() {
        const MINISIZE: &[u8] = include_bytes!("../../../../assets/icons/minisize.svg");
        assert!(
            std::str::from_utf8(MINISIZE)
                .unwrap()
                .contains("lucide-minimize")
        );
    }
}
