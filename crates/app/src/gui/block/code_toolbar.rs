use crate::gui::GuiTheme;
use crate::gui::app::CditorV2View;
use crate::gui::input::{
    CODE_LANGUAGE_VISIBLE_SUGGESTIONS, CodeLanguageEditState, CodeLanguageItem,
    CodeLanguagePopupPlacement, SINGLE_LINE_INPUT_FONT_SIZE_PX, SingleLineTextInputElement,
};
use cditor_core::ids::BlockId;
use gpui::InteractiveElement;
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Entity, FocusHandle, IntoElement, MouseButton, ParentElement, Styled, deferred,
    div, px, rgb,
};

pub const V1_CODE_TOOLBAR_TOP_PX: f32 = 6.0;
pub const V1_CODE_TOOLBAR_RIGHT_PX: f32 = 6.0;
pub const V1_CODE_TOOLBAR_HEIGHT_PX: f32 = 30.0;
pub const V1_CODE_TOOLBAR_RADIUS_PX: f32 = 3.0;
pub const V1_CODE_TOOLBAR_PADDING_PX: f32 = 2.0;
pub const V1_CODE_TOOLBAR_BUTTON_SIZE_PX: f32 = 26.0;
pub const V1_CODE_TOOLBAR_BUTTON_RADIUS_PX: f32 = 3.0;
pub const V1_CODE_LANGUAGE_BUTTON_WIDTH_PX: f32 = 72.0;
pub const V1_CODE_LANGUAGE_EDIT_WIDTH_PX: f32 = 132.0;
pub const V1_CODE_TOOLBAR_GAP_PX: f32 = 2.0;
pub const V1_CODE_LANGUAGE_POPUP_GAP_PX: f32 = 6.0;
pub const V1_CODE_LANGUAGE_POPUP_MAX_HEIGHT_PX: f32 = 260.0;
pub const V1_CODE_COPY_ICON_SIZE_PX: f32 = 16.0;
pub const V1_CODE_COPY_ICON_RECT_SIZE_PX: f32 = 10.0;
pub const V1_CODE_COPY_ICON_OFFSET_PX: f32 = 4.0;

pub fn render_code_toolbar(
    block_id: BlockId,
    theme: GuiTheme,
    language: Option<&str>,
    language_edit: Option<&CodeLanguageEditState>,
    view: Entity<CditorV2View>,
    code_language_focus: FocusHandle,
) -> AnyElement {
    div()
        .absolute()
        .top(px(V1_CODE_TOOLBAR_TOP_PX))
        .right(px(V1_CODE_TOOLBAR_RIGHT_PX))
        .opacity(0.0)
        .group_hover("notion-code-block", |style| style.opacity(1.0))
        .when(language_edit.is_some(), |this| this.opacity(1.0))
        .flex()
        .flex_col()
        .items_end()
        .gap(px(4.0))
        .child(
            div()
                .h(px(V1_CODE_TOOLBAR_HEIGHT_PX))
                .flex()
                .items_center()
                .gap(px(V1_CODE_TOOLBAR_GAP_PX))
                .rounded(px(V1_CODE_TOOLBAR_RADIUS_PX))
                .p(px(V1_CODE_TOOLBAR_PADDING_PX))
                .text_size(px(12.0))
                .text_color(rgb(theme.code_toolbar_text))
                .child(render_language_editor(
                    block_id,
                    theme,
                    language,
                    language_edit,
                    view.clone(),
                    code_language_focus,
                ))
                .child(render_copy_button(theme, block_id, view.clone()))
                .child(render_toolbar_icon_button(theme, "...")),
        )
        .into_any_element()
}

fn render_language_editor(
    block_id: BlockId,
    theme: GuiTheme,
    language: Option<&str>,
    language_edit: Option<&CodeLanguageEditState>,
    view: Entity<CditorV2View>,
    code_language_focus: FocusHandle,
) -> AnyElement {
    let label = language_edit
        .map(|edit| {
            if edit.draft.is_empty() {
                "Search language".to_owned()
            } else {
                edit.draft.clone()
            }
        })
        .unwrap_or_else(|| language.unwrap_or("plain text").to_owned());
    let current_language = language.map(ToOwned::to_owned);
    let suggestions = language_edit
        .map(CodeLanguageEditState::matching_items)
        .unwrap_or_default();
    let selected_index = language_edit
        .map(|edit| edit.selected_index)
        .unwrap_or_default();
    let scroll_start = language_edit
        .map(|edit| edit.scroll_start)
        .unwrap_or_default();
    let is_editing = language_edit.is_some();
    let marked_range = language_edit.and_then(|edit| edit.marked_range.clone());
    let caret_offset = language_edit.map(|edit| edit.caret_offset);
    let input_view = view.clone();
    div()
        .relative()
        .h(px(V1_CODE_TOOLBAR_BUTTON_SIZE_PX))
        .min_w(px(V1_CODE_LANGUAGE_BUTTON_WIDTH_PX))
        .flex()
        .items_center()
        .child(
            div()
                .h(px(V1_CODE_TOOLBAR_BUTTON_SIZE_PX))
                .w(px(if is_editing {
                    V1_CODE_LANGUAGE_EDIT_WIDTH_PX
                } else {
                    V1_CODE_LANGUAGE_BUTTON_WIDTH_PX
                }))
                .px(px(8.0))
                .flex()
                .items_center()
                .rounded(px(V1_CODE_TOOLBAR_BUTTON_RADIUS_PX))
                .text_color(rgb(if is_editing {
                    theme.text
                } else {
                    theme.code_toolbar_text
                }))
                .bg(rgb(if is_editing {
                    theme.code_toolbar_hover
                } else {
                    theme.code_background
                }))
                .hover(move |style| style.bg(rgb(theme.code_toolbar_hover)))
                .when(is_editing, |this| {
                    let view = view.clone();
                    this.track_focus(&code_language_focus)
                        .on_key_down(move |event, _window, cx| {
                            let handled = view.update(cx, |view, cx| {
                                let handled = view.apply_code_language_key_from_gui(event, cx);
                                if handled {
                                    cx.notify();
                                }
                                handled
                            });
                            if handled {
                                cx.stop_propagation();
                            }
                        })
                })
                .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                    let _ = input_view.update(cx, |view, cx| {
                        view.start_code_language_edit_from_gui(
                            block_id,
                            current_language.as_deref(),
                            f32::from(event.position.y),
                            window,
                            cx,
                        );
                    });
                    cx.stop_propagation();
                })
                .child(
                    div()
                        .relative()
                        .w_full()
                        .h_full()
                        .flex()
                        .items_center()
                        .overflow_hidden()
                        .when(!is_editing, |this| {
                            this.child(
                                div()
                                    .min_w(px(0.0))
                                    .w_full()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .whitespace_nowrap()
                                    .child(label),
                            )
                        })
                        .when(is_editing, |this| {
                            this.child(SingleLineTextInputElement {
                                handler: view.clone(),
                                focus: code_language_focus.clone(),
                                value: language_edit
                                    .map(|edit| edit.draft.clone())
                                    .unwrap_or_default(),
                                placeholder: Some("Search language".to_owned()),
                                caret_offset,
                                marked_range,
                                text_color: theme.text,
                                placeholder_color: theme.muted,
                                caret_color: theme.focused,
                                font_size: px(SINGLE_LINE_INPUT_FONT_SIZE_PX),
                            })
                        }),
                ),
        )
        .when(is_editing, |this| {
            this.child(render_language_suggestions(
                block_id,
                theme,
                suggestions,
                selected_index,
                scroll_start,
                language_edit
                    .map(|edit| edit.placement)
                    .unwrap_or(CodeLanguagePopupPlacement::Below),
                view.clone(),
            ))
        })
        .into_any_element()
}

fn render_language_suggestions(
    block_id: BlockId,
    theme: GuiTheme,
    suggestions: Vec<CodeLanguageItem>,
    selected_index: usize,
    scroll_start: usize,
    placement: CodeLanguagePopupPlacement,
    view: Entity<CditorV2View>,
) -> AnyElement {
    let total_suggestions = suggestions.len();
    let scroll_start = scroll_start.min(total_suggestions.saturating_sub(1));
    let mut panel = div()
        .absolute()
        .right(px(-(code_language_popup_right_overhang())))
        .w(px(code_language_popup_width()))
        .h(px(code_language_panel_height(total_suggestions)))
        .rounded(px(8.0))
        .border_1()
        .border_color(rgb(theme.code_toolbar_border))
        .bg(rgb(theme.code_toolbar_background))
        .shadow_lg()
        .occlude()
        .overflow_hidden()
        .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
            cx.stop_propagation();
        })
        .on_scroll_wheel({
            let view = view.clone();
            move |event, _window, cx| {
                let delta_y = f32::from(
                    event
                        .delta
                        .pixel_delta(px(code_language_suggestion_row_height()))
                        .y,
                );
                let delta_rows = scroll_delta_rows(delta_y);
                if delta_rows != 0 {
                    let changed = view.update(cx, |view, cx| {
                        view.scroll_code_language_suggestions_from_gui(delta_rows, cx)
                    });
                    if changed {
                        cx.stop_propagation();
                    }
                } else {
                    cx.stop_propagation();
                }
            }
        })
        .when(placement == CodeLanguagePopupPlacement::Below, |panel| {
            panel.top(px(
                V1_CODE_TOOLBAR_BUTTON_SIZE_PX + V1_CODE_LANGUAGE_POPUP_GAP_PX
            ))
        })
        .when(placement == CodeLanguagePopupPlacement::Above, |panel| {
            panel.bottom(px(
                V1_CODE_TOOLBAR_BUTTON_SIZE_PX + V1_CODE_LANGUAGE_POPUP_GAP_PX
            ))
        })
        .on_mouse_down_out({
            let view = view.clone();
            move |_event, _window, cx| {
                let _ = view.update(cx, |view, cx| {
                    view.cancel_code_language_edit(cx);
                });
            }
        });

    if suggestions.is_empty() {
        panel = panel.child(
            div()
                .px(px(12.0))
                .py(px(10.0))
                .text_size(px(12.0))
                .text_color(rgb(theme.muted))
                .child("No matching suggestions"),
        );
    } else {
        panel = panel.child(
            div()
                .w_full()
                .h_full()
                .bg(rgb(theme.code_toolbar_background))
                .children(
                    suggestions
                        .into_iter()
                        .enumerate()
                        .skip(scroll_start)
                        .take(CODE_LANGUAGE_VISIBLE_SUGGESTIONS)
                        .map(|(index, item)| {
                            render_language_suggestion_row(
                                block_id,
                                theme,
                                item,
                                index == selected_index,
                                view.clone(),
                            )
                        }),
                ),
        );
        if total_suggestions > CODE_LANGUAGE_VISIBLE_SUGGESTIONS {
            panel = panel.child(render_language_scrollbar(
                theme,
                total_suggestions,
                scroll_start,
            ));
        }
    }
    deferred(panel).with_priority(100).into_any_element()
}

fn code_language_popup_width() -> f32 {
    V1_CODE_LANGUAGE_EDIT_WIDTH_PX
        + V1_CODE_TOOLBAR_BUTTON_SIZE_PX * 2.0
        + V1_CODE_TOOLBAR_GAP_PX * 2.0
        + V1_CODE_TOOLBAR_PADDING_PX * 2.0
}

fn code_language_popup_max_height() -> f32 {
    CODE_LANGUAGE_VISIBLE_SUGGESTIONS as f32 * code_language_suggestion_row_height()
}

fn code_language_panel_height(total_suggestions: usize) -> f32 {
    total_suggestions
        .min(CODE_LANGUAGE_VISIBLE_SUGGESTIONS)
        .max(1) as f32
        * code_language_suggestion_row_height()
}

fn code_language_suggestion_row_height() -> f32 {
    34.0
}

fn scroll_delta_rows(delta_y: f32) -> isize {
    if delta_y.abs() < 1.0 {
        return 0;
    }
    let rows = (delta_y.abs() / code_language_suggestion_row_height())
        .ceil()
        .max(1.0) as isize;
    if delta_y > 0.0 { -rows } else { rows }
}

fn render_language_scrollbar(
    theme: GuiTheme,
    total_suggestions: usize,
    scroll_start: usize,
) -> AnyElement {
    let track_height = code_language_popup_max_height() - 8.0;
    let visible = CODE_LANGUAGE_VISIBLE_SUGGESTIONS.min(total_suggestions);
    let thumb_height = (track_height * visible as f32 / total_suggestions as f32).max(24.0);
    let max_start = total_suggestions.saturating_sub(visible).max(1);
    let max_top = (track_height - thumb_height).max(0.0);
    let thumb_top = 4.0 + max_top * scroll_start.min(max_start) as f32 / max_start as f32;

    div()
        .absolute()
        .right(px(3.0))
        .top(px(4.0))
        .w(px(3.0))
        .h(px(track_height))
        .rounded(px(2.0))
        .bg(rgb(theme.code_toolbar_border))
        .child(
            div()
                .absolute()
                .top(px(thumb_top - 4.0))
                .w(px(3.0))
                .h(px(thumb_height))
                .rounded(px(2.0))
                .bg(rgb(theme.muted)),
        )
        .into_any_element()
}

fn code_language_popup_right_overhang() -> f32 {
    V1_CODE_TOOLBAR_BUTTON_SIZE_PX * 2.0 + V1_CODE_TOOLBAR_GAP_PX * 2.0 + V1_CODE_TOOLBAR_PADDING_PX
}

fn render_language_suggestion_row(
    block_id: BlockId,
    theme: GuiTheme,
    item: CodeLanguageItem,
    selected: bool,
    view: Entity<CditorV2View>,
) -> AnyElement {
    let value = item.value.clone();
    let row_background = if selected {
        theme.code_toolbar_hover
    } else {
        theme.code_toolbar_background
    };
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_between()
        .w_full()
        .h(px(code_language_suggestion_row_height()))
        .gap_3()
        .px(px(12.0))
        .cursor_pointer()
        .bg(rgb(row_background))
        .hover(move |style| style.bg(rgb(theme.code_toolbar_hover)).cursor_pointer())
        .child(
            div()
                .bg(rgb(row_background))
                .text_size(px(13.0))
                .text_color(rgb(theme.text))
                .child(item.label),
        )
        .child(
            div()
                .bg(rgb(row_background))
                .text_size(px(11.0))
                .text_color(rgb(theme.muted))
                .child(item.value),
        )
        .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
            let value = value.clone();
            let _ = view.update(cx, |view, cx| {
                view.select_code_language_from_gui(block_id, value, cx);
            });
            cx.stop_propagation();
        })
        .into_any_element()
}

fn render_copy_button(
    theme: GuiTheme,
    block_id: BlockId,
    view: Entity<CditorV2View>,
) -> AnyElement {
    div()
        .w(px(V1_CODE_TOOLBAR_BUTTON_SIZE_PX))
        .h(px(V1_CODE_TOOLBAR_BUTTON_SIZE_PX))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(V1_CODE_TOOLBAR_BUTTON_RADIUS_PX))
        .text_color(rgb(theme.code_toolbar_icon))
        .hover(move |style| style.bg(rgb(theme.code_toolbar_hover)))
        .child(render_copy_icon(theme))
        .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
            let _ = view.update(cx, |view, cx| {
                view.copy_code_block_from_gui(block_id, cx);
            });
            cx.stop_propagation();
        })
        .into_any_element()
}

fn render_copy_icon(theme: GuiTheme) -> AnyElement {
    let icon_color = rgb(theme.code_toolbar_icon);
    div()
        .relative()
        .w(px(V1_CODE_COPY_ICON_SIZE_PX))
        .h(px(V1_CODE_COPY_ICON_SIZE_PX))
        .child(
            div()
                .absolute()
                .left(px(1.0))
                .top(px(1.0))
                .w(px(V1_CODE_COPY_ICON_RECT_SIZE_PX))
                .h(px(V1_CODE_COPY_ICON_RECT_SIZE_PX))
                .rounded(px(2.0))
                .border_1()
                .border_color(icon_color),
        )
        .child(
            div()
                .absolute()
                .left(px(V1_CODE_COPY_ICON_OFFSET_PX))
                .top(px(V1_CODE_COPY_ICON_OFFSET_PX))
                .w(px(V1_CODE_COPY_ICON_RECT_SIZE_PX))
                .h(px(V1_CODE_COPY_ICON_RECT_SIZE_PX))
                .rounded(px(2.0))
                .border_1()
                .border_color(icon_color)
                .bg(rgb(theme.code_background)),
        )
        .into_any_element()
}

fn render_toolbar_icon_button(theme: GuiTheme, label: &'static str) -> AnyElement {
    div()
        .w(px(V1_CODE_TOOLBAR_BUTTON_SIZE_PX))
        .h(px(V1_CODE_TOOLBAR_BUTTON_SIZE_PX))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(V1_CODE_TOOLBAR_BUTTON_RADIUS_PX))
        .text_color(rgb(theme.code_toolbar_icon))
        .hover(move |style| style.bg(rgb(theme.code_toolbar_hover)))
        .child(label)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v1_code_toolbar_geometry_constants_match_editor2() {
        assert_eq!(V1_CODE_TOOLBAR_TOP_PX, 6.0);
        assert_eq!(V1_CODE_TOOLBAR_RIGHT_PX, 6.0);
        assert_eq!(V1_CODE_TOOLBAR_HEIGHT_PX, 30.0);
        assert_eq!(V1_CODE_TOOLBAR_RADIUS_PX, 3.0);
        assert_eq!(V1_CODE_TOOLBAR_BUTTON_SIZE_PX, 26.0);
        assert_eq!(V1_CODE_TOOLBAR_BUTTON_RADIUS_PX, 3.0);
        assert_eq!(V1_CODE_LANGUAGE_BUTTON_WIDTH_PX, 72.0);
        assert_eq!(V1_CODE_LANGUAGE_EDIT_WIDTH_PX, 132.0);
        assert_eq!(V1_CODE_TOOLBAR_GAP_PX, 2.0);
        assert_eq!(V1_CODE_LANGUAGE_POPUP_GAP_PX, 6.0);
        assert_eq!(V1_CODE_LANGUAGE_POPUP_MAX_HEIGHT_PX, 260.0);
        assert_eq!(V1_CODE_COPY_ICON_SIZE_PX, 16.0);
        assert_eq!(V1_CODE_COPY_ICON_RECT_SIZE_PX, 10.0);
        assert_eq!(V1_CODE_COPY_ICON_OFFSET_PX, 4.0);
    }

    #[test]
    fn language_popup_matches_toolbar_width() {
        assert_eq!(code_language_popup_width(), 192.0);
        assert_eq!(code_language_popup_right_overhang(), 58.0);
    }

    #[test]
    fn language_popup_height_is_bounded_to_visible_rows() {
        assert_eq!(code_language_suggestion_row_height(), 34.0);
        assert_eq!(code_language_popup_max_height(), 238.0);
        assert!(code_language_popup_max_height() < V1_CODE_LANGUAGE_POPUP_MAX_HEIGHT_PX);
    }

    #[test]
    fn language_popup_scroll_delta_maps_to_rows() {
        assert_eq!(scroll_delta_rows(0.2), 0);
        assert_eq!(scroll_delta_rows(1.0), -1);
        assert_eq!(scroll_delta_rows(34.0), -1);
        assert_eq!(scroll_delta_rows(35.0), -2);
        assert_eq!(scroll_delta_rows(-35.0), 2);
    }
}
