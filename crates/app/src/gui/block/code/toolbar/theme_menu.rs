use crate::gui::GuiTheme;
use crate::gui::app::CditorV2View;
use crate::gui::block::code::highlight::{CODE_THEME_ITEMS, CodeThemeItem, code_theme_item};
use cditor_core::ids::BlockId;
use gpui::{
    AnyElement, Entity, InteractiveElement, IntoElement, MouseButton, ParentElement, Styled,
    deferred, div, px, rgb,
};

use super::{
    V1_CODE_THEME_POPUP_WIDTH_PX, V1_CODE_THEME_ROW_HEIGHT_PX, V1_CODE_TOOLBAR_BUTTON_RADIUS_PX,
    V1_CODE_TOOLBAR_BUTTON_SIZE_PX,
};

pub(super) fn render_code_theme_button(
    theme: GuiTheme,
    block_id: BlockId,
    current_theme: &'static str,
    open: bool,
    view: Entity<CditorV2View>,
) -> AnyElement {
    let item = code_theme_item(current_theme);
    div()
        .w(px(V1_CODE_TOOLBAR_BUTTON_SIZE_PX))
        .h(px(V1_CODE_TOOLBAR_BUTTON_SIZE_PX))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(V1_CODE_TOOLBAR_BUTTON_RADIUS_PX))
        .text_color(rgb(theme.code_toolbar_icon))
        .bg(rgb(if open {
            theme.code_toolbar_hover
        } else {
            theme.code_toolbar_background
        }))
        .hover(move |style| style.bg(rgb(theme.code_toolbar_hover)))
        .child(render_code_theme_icon(item))
        .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
            let _ = view.update(cx, |view, cx| {
                view.toggle_code_theme_menu_from_gui(block_id, cx);
            });
            cx.stop_propagation();
        })
        .into_any_element()
}

fn render_code_theme_icon(item: CodeThemeItem) -> AnyElement {
    div()
        .relative()
        .w(px(15.0))
        .h(px(15.0))
        .children(item.preview.into_iter().enumerate().map(|(index, color)| {
            let column = index % 2;
            let row = index / 2;
            div()
                .absolute()
                .left(px(column as f32 * 7.0))
                .top(px(row as f32 * 7.0))
                .w(px(6.0))
                .h(px(6.0))
                .rounded(px(2.0))
                .bg(rgb(color))
        }))
        .into_any_element()
}

pub(super) fn render_code_theme_popup(
    theme: GuiTheme,
    current_theme: &'static str,
    view: Entity<CditorV2View>,
) -> AnyElement {
    let panel =
        div()
            .w(px(V1_CODE_THEME_POPUP_WIDTH_PX))
            .p(px(6.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(rgb(theme.code_toolbar_border))
            .bg(rgb(theme.code_toolbar_background))
            .shadow_lg()
            .occlude()
            .child(
                div()
                    .px(px(8.0))
                    .pt(px(4.0))
                    .pb(px(6.0))
                    .text_size(px(11.0))
                    .text_color(rgb(theme.muted))
                    .child("代码高亮主题"),
            )
            .children(CODE_THEME_ITEMS.into_iter().map(|item| {
                render_code_theme_row(theme, item, item.id == current_theme, view.clone())
            }))
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation();
            })
            .on_mouse_down_out(move |_event, _window, cx| {
                let _ = view.update(cx, |view, cx| view.dismiss_code_theme_menu(cx));
            });
    deferred(panel).with_priority(110).into_any_element()
}

fn render_code_theme_row(
    theme: GuiTheme,
    item: CodeThemeItem,
    selected: bool,
    view: Entity<CditorV2View>,
) -> AnyElement {
    div()
        .w_full()
        .h(px(V1_CODE_THEME_ROW_HEIGHT_PX))
        .px(px(8.0))
        .flex()
        .items_center()
        .gap(px(10.0))
        .rounded(px(5.0))
        .cursor_pointer()
        .bg(rgb(if selected {
            theme.code_toolbar_hover
        } else {
            theme.code_toolbar_background
        }))
        .hover(move |style| style.bg(rgb(theme.code_toolbar_hover)).cursor_pointer())
        .child(render_code_theme_preview(item))
        .child(
            div()
                .flex_1()
                .text_size(px(12.0))
                .text_color(rgb(theme.text))
                .child(item.label),
        )
        .child(
            div()
                .w(px(14.0))
                .text_size(px(13.0))
                .text_color(rgb(theme.code_toolbar_icon))
                .child(if selected { "✓" } else { "" }),
        )
        .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
            let _ = view.update(cx, |view, cx| {
                view.select_code_theme_from_gui(item.id, cx);
            });
            cx.stop_propagation();
        })
        .into_any_element()
}

fn render_code_theme_preview(item: CodeThemeItem) -> AnyElement {
    div()
        .w(px(46.0))
        .h(px(18.0))
        .px(px(4.0))
        .flex()
        .items_center()
        .gap(px(3.0))
        .rounded(px(4.0))
        .bg(rgb(item.background))
        .children(
            item.preview
                .into_iter()
                .map(|color| div().w(px(7.0)).h(px(10.0)).rounded(px(2.0)).bg(rgb(color))),
        )
        .into_any_element()
}
