use gpui::{
    AnyElement, Entity, FocusHandle, InteractiveElement, IntoElement, MouseButton, ParentElement,
    StatefulInteractiveElement, Styled, div, px, rgb,
};

use crate::editor_view::CditorV2View;
use crate::input::link_edit::{LinkEditField, LinkEditState};
use crate::input::{SINGLE_LINE_INPUT_FONT_SIZE_PX, SingleLineTextInputElement};
use crate::theme::GuiTheme;
use cditor_component::SvgIcon;

const LINK_POPUP_WIDTH_PX: f32 = 280.0;
const LINK_POPUP_MARGIN_PX: f32 = 12.0;
const LINK_POPUP_ROW_HEIGHT_PX: f32 = 32.0;
const ICON_COPY: &[u8] = include_bytes!("../../../../assets/icons/copy.svg");
const ICON_DELETE: &[u8] = include_bytes!("../../../../assets/icons/delete.svg");

pub(crate) fn render_link_edit_popup(
    edit: &LinkEditState,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
    focus: FocusHandle,
    viewport_width: f32,
    viewport_height: f32,
) -> AnyElement {
    let width = LINK_POPUP_WIDTH_PX.min((viewport_width - LINK_POPUP_MARGIN_PX * 2.0).max(1.0));
    let max_x = (viewport_width - LINK_POPUP_MARGIN_PX - width).max(LINK_POPUP_MARGIN_PX);
    let x = edit.x.clamp(LINK_POPUP_MARGIN_PX, max_x);
    let height = LINK_POPUP_ROW_HEIGHT_PX * 3.0 + 2.0;
    let max_y = (viewport_height - LINK_POPUP_MARGIN_PX - height).max(LINK_POPUP_MARGIN_PX);
    let y = edit.y.clamp(LINK_POPUP_MARGIN_PX, max_y);

    let text_row = link_input_row(
        "link-edit-text",
        edit,
        LinkEditField::Text,
        &edit.text_draft,
        "链接文字",
        theme,
        view.clone(),
        focus.clone(),
        None,
    );
    let copy_view = view.clone();
    let url_row = link_input_row(
        "link-edit-url",
        edit,
        LinkEditField::Url,
        &edit.href_draft,
        "https://",
        theme,
        view.clone(),
        focus,
        Some(
            div()
                .id("link-edit-copy")
                .size(px(22.0))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.0))
                .cursor_pointer()
                .hover(|style| style.bg(rgb(theme.hover_surface)))
                .child(
                    SvgIcon::new("link-edit-copy-icon", ICON_COPY)
                        .color(rgb(theme.muted))
                        .size(px(14.0)),
                )
                .on_mouse_down(MouseButton::Left, {
                    move |_event, _window, cx| {
                        copy_view.update(cx, |view, cx| view.copy_link_href_from_popup(cx));
                        cx.stop_propagation();
                    }
                })
                .into_any_element(),
        ),
    );
    let clear_view = view.clone();
    let footer = div()
        .w_full()
        .h(px(LINK_POPUP_ROW_HEIGHT_PX))
        .px(px(10.0))
        .flex()
        .items_center()
        .justify_between()
        .border_t_1()
        .border_color(rgb(theme.border))
        .child(
            div()
                .text_size(px(11.0))
                .text_color(rgb(theme.muted))
                .child("输入链接地址，回车确认"),
        )
        .child(
            div()
                .id("link-edit-clear")
                .size(px(22.0))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.0))
                .cursor_pointer()
                .hover(|style| style.bg(rgb(theme.hover_surface)))
                .child(
                    SvgIcon::new("link-edit-clear-icon", ICON_DELETE)
                        .color(rgb(theme.muted))
                        .size(px(14.0)),
                )
                .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
                    clear_view.update(cx, |view, cx| view.clear_link_from_popup(cx));
                    cx.stop_propagation();
                }),
        );

    div()
        .absolute()
        .left(px(x))
        .top(px(y))
        .w(px(width))
        .rounded(px(8.0))
        .border_1()
        .border_color(rgb(theme.border))
        .bg(rgb(theme.panel))
        .shadow_md()
        .occlude()
        .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
            cx.stop_propagation();
        })
        .on_mouse_down_out({
            let view = view.clone();
            move |_event, _window, cx| {
                let _ = view.update(cx, |view, cx| view.commit_link_edit(cx));
            }
        })
        .child(text_row)
        .child(div().w_full().h(px(1.0)).bg(rgb(theme.border)))
        .child(url_row)
        .child(footer)
        .into_any_element()
}

#[expect(clippy::too_many_arguments, reason = "popup row context aggregate")]
fn link_input_row(
    id: &'static str,
    edit: &LinkEditState,
    field: LinkEditField,
    value: &str,
    placeholder: &str,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
    focus: FocusHandle,
    trailing: Option<AnyElement>,
) -> AnyElement {
    let active = edit.focused_field == field;
    let focus_view = view.clone();
    div()
        .id(id)
        .w_full()
        .h(px(LINK_POPUP_ROW_HEIGHT_PX))
        .px(px(10.0))
        .flex()
        .items_center()
        .gap(px(6.0))
        .cursor_text()
        .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
            focus_view.update(cx, |view, cx| view.focus_link_edit_field(field, cx));
            cx.stop_propagation();
        })
        .child(
            div()
                .min_w(px(0.0))
                .h(px(20.0))
                .flex_1()
                .child(SingleLineTextInputElement {
                    handler: view,
                    focus,
                    value: value.to_owned(),
                    placeholder: Some(placeholder.to_owned()),
                    caret_offset: active.then_some(edit.caret_offset),
                    marked_range: active.then(|| edit.marked_range.clone()).flatten(),
                    text_color: theme.text,
                    placeholder_color: theme.muted,
                    caret_color: theme.focused,
                    font_size: px(SINGLE_LINE_INPUT_FONT_SIZE_PX),
                }),
        )
        .children(trailing)
        .into_any_element()
}
