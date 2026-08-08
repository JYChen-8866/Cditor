//! Code-block language and copy controls.

use crate::editor_view::CditorV2View;
use crate::input::platform_adapter::{mobile_manual_focus, on_text_activation};
use crate::input::{
    CODE_LANGUAGE_VISIBLE_SUGGESTIONS, CodeLanguageEditState, CodeLanguagePopupPlacement,
    SINGLE_LINE_INPUT_FONT_SIZE_PX, SingleLineTextInputElement,
};
use crate::theme::GuiTheme;
use cditor_component::{Combobox, ComboboxItem, ComboboxPlacement, ComboboxStyle, SvgIcon};
use cditor_core::ids::BlockId;
use gpui::InteractiveElement;
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Entity, FocusHandle, IntoElement, MouseButton, ParentElement, Styled, div, px, rgb,
};

pub const V1_CODE_TOOLBAR_TOP_PX: f32 = 3.0;
pub const V1_CODE_TOOLBAR_LEFT_PX: f32 = 6.0;
pub const V1_CODE_TOOLBAR_RIGHT_PX: f32 = 6.0;
pub const V1_CODE_TOOLBAR_OPACITY: f32 = 1.0;
pub const V1_CODE_TOOLBAR_HEIGHT_PX: f32 = 30.0;
pub const V1_CODE_TOOLBAR_RADIUS_PX: f32 = 4.0;
pub const V1_CODE_TOOLBAR_PADDING_PX: f32 = 2.0;
pub const V1_CODE_TOOLBAR_BUTTON_SIZE_PX: f32 = 26.0;
pub const V1_CODE_TOOLBAR_BUTTON_RADIUS_PX: f32 = 4.0;
pub const V1_CODE_LANGUAGE_BUTTON_MIN_WIDTH_PX: f32 = 160.0;
pub const V1_CODE_LANGUAGE_BUTTON_MAX_WIDTH_PX: f32 = 160.0;
pub const V1_CODE_LANGUAGE_EDIT_WIDTH_PX: f32 = 160.0;
pub const V1_CODE_TOOLBAR_GAP_PX: f32 = 2.0;
pub const V1_CODE_LANGUAGE_POPUP_GAP_PX: f32 = 6.0;
#[cfg(test)]
pub const V1_CODE_LANGUAGE_POPUP_MAX_HEIGHT_PX: f32 = 300.0;
pub const V1_CODE_LANGUAGE_SEARCH_HEIGHT_PX: f32 = 32.0;
pub const V1_CODE_COPY_ICON_SIZE_PX: f32 = 16.0;
pub const V1_CODE_LANGUAGE_ICON_SIZE_PX: f32 = 14.0;

#[expect(clippy::too_many_arguments, reason = "P4-002 render context 聚合")]
pub fn render_code_toolbar(
    block_id: BlockId,
    theme: GuiTheme,
    language: Option<&str>,
    language_edit: Option<&CodeLanguageEditState>,
    _code_theme_menu_open: bool,
    _code_highlight_theme: &'static str,
    view: Entity<CditorV2View>,
    code_language_focus: FocusHandle,
) -> AnyElement {
    div()
        .absolute()
        .top(px(V1_CODE_TOOLBAR_TOP_PX))
        .left(px(V1_CODE_TOOLBAR_LEFT_PX))
        .right(px(V1_CODE_TOOLBAR_RIGHT_PX))
        .opacity(V1_CODE_TOOLBAR_OPACITY)
        .flex()
        .flex_col()
        .items_end()
        .gap(px(4.0))
        .child(
            div()
                .w_full()
                .h(px(V1_CODE_TOOLBAR_HEIGHT_PX))
                .flex()
                .items_center()
                .justify_between()
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
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(V1_CODE_TOOLBAR_GAP_PX))
                        .child(render_collapse_button(theme, block_id, view.clone()))
                        .child(render_copy_button(theme, block_id, view)),
                ),
        )
        .into_any_element()
}

fn render_collapse_button(
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
        .child(render_collapse_icon(theme, block_id, view.clone()))
        .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
            view.update(cx, |view, cx| {
                super::actions::toggle_code_block_collapsed_from_gui(view, block_id, cx);
            });
            cx.stop_propagation();
        })
        .into_any_element()
}

fn render_collapse_icon(
    theme: GuiTheme,
    block_id: BlockId,
    view: Entity<CditorV2View>,
) -> AnyElement {
    const CHEVRON_UP: &[u8] = include_bytes!("../../../../../../assets/icons/chevron-up.svg");
    const CHEVRON_DOWN: &[u8] = include_bytes!("../../../../../../assets/icons/chevron-down.svg");

    SvgIcon::dynamic(move |cx| {
        if view
            .read(cx)
            .overlay
            .collapsed_code_blocks
            .contains(&block_id)
        {
            ("code-toolbar-expand", CHEVRON_DOWN)
        } else {
            ("code-toolbar-collapse", CHEVRON_UP)
        }
    })
    .color(rgb(theme.code_toolbar_icon))
    .size(px(V1_CODE_COPY_ICON_SIZE_PX))
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
    let label = language.unwrap_or("plain text").to_owned();
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
    let draft = language_edit
        .map(|edit| edit.draft.clone())
        .unwrap_or_default();
    let total_suggestions = suggestions.len();
    let scroll_start = scroll_start.min(total_suggestions.saturating_sub(1));
    let checked_language = current_language
        .as_deref()
        .unwrap_or("plain text")
        .to_owned();
    let language_icon = render_language_icon(language);
    let items = suggestions
        .into_iter()
        .enumerate()
        .skip(scroll_start)
        .take(CODE_LANGUAGE_VISIBLE_SUGGESTIONS)
        .map(|(index, item)| {
            let value = item.value.clone();
            let checked = item.value.eq_ignore_ascii_case(&checked_language);
            let select_view = view.clone();
            ComboboxItem::new(item.label, move |_window, cx| {
                let value = value.clone();
                select_view.update(cx, |view, cx| {
                    view.select_code_language_from_gui(block_id, value, cx);
                });
            })
            .selected(index == selected_index)
            .checked(checked)
        })
        .collect();
    let toggle_view = view.clone();
    let dismiss_view = view.clone();
    let wheel_view = view.clone();
    let scrollbar_view = view.clone();
    let placement = match language_edit.map(|edit| edit.placement) {
        Some(CodeLanguagePopupPlacement::Above) => ComboboxPlacement::Above,
        _ => ComboboxPlacement::Below,
    };
    Combobox::new(
        label,
        is_editing,
        code_language_combobox_style(theme),
        move |pointer_y, window, cx| {
            toggle_view.update(cx, |view, cx| {
                view.toggle_code_language_dropdown_from_gui(
                    block_id,
                    current_language.as_deref(),
                    pointer_y,
                    window,
                    cx,
                );
            });
        },
        move |window, cx| {
            let _ = dismiss_view.update(cx, |view, cx| {
                view.dismiss_code_language_dropdown_from_gui(window, cx)
            });
        },
    )
    .placement(placement)
    .when_some(language_icon, |combobox, icon| combobox.trigger_icon(icon))
    .search(render_language_search_input(
        block_id,
        theme,
        draft,
        caret_offset,
        marked_range,
        code_language_focus,
        view,
    ))
    .items(items, total_suggestions)
    .scroll(
        scroll_start,
        CODE_LANGUAGE_VISIBLE_SUGGESTIONS,
        move |delta_rows, _window, cx| {
            wheel_view.update(cx, |view, cx| {
                view.scroll_code_language_suggestions_from_gui(delta_rows, cx)
            });
        },
        move |target_start, _window, cx| {
            scrollbar_view.update(cx, |view, cx| {
                view.set_code_language_scroll_start_from_gui(target_start, cx);
            });
        },
    )
    .popup_right_offset(px(0.0))
    .into_any_element()
}

fn render_language_icon(language: Option<&str>) -> Option<SvgIcon> {
    let (key, bytes) = language_icon_source(language?)?;
    Some(
        SvgIcon::new(key, bytes)
            .colored()
            .size(px(V1_CODE_LANGUAGE_ICON_SIZE_PX)),
    )
}

fn language_icon_source(language: &str) -> Option<(&'static str, &'static [u8])> {
    Some(match language.to_ascii_lowercase().as_str() {
        "c" => (
            "language-icon-c",
            include_bytes!("../../../../../../assets/icons/c.svg").as_slice(),
        ),
        "rust" | "rs" => (
            "language-icon-rust",
            include_bytes!("../../../../../../assets/icons/rust.svg").as_slice(),
        ),
        "typescript" | "ts" => (
            "language-icon-typescript",
            include_bytes!("../../../../../../assets/icons/typescript.svg").as_slice(),
        ),
        "javascript" | "js" | "jsx" => (
            "language-icon-javascript",
            include_bytes!("../../../../../../assets/icons/javascript.svg").as_slice(),
        ),
        "python" | "py" => (
            "language-icon-python",
            include_bytes!("../../../../../../assets/icons/python.svg").as_slice(),
        ),
        "go" | "golang" => (
            "language-icon-go",
            include_bytes!("../../../../../../assets/icons/go.svg").as_slice(),
        ),
        "cpp" | "c++" => (
            "language-icon-cpp",
            include_bytes!("../../../../../../assets/icons/cpp.svg").as_slice(),
        ),
        "csharp" | "c#" | "cs" => (
            "language-icon-csharp",
            include_bytes!("../../../../../../assets/icons/csharp.svg").as_slice(),
        ),
        "css" => (
            "language-icon-css",
            include_bytes!("../../../../../../assets/icons/css.svg").as_slice(),
        ),
        "haskell" => (
            "language-icon-haskell",
            include_bytes!("../../../../../../assets/icons/haskell.svg").as_slice(),
        ),
        "html" | "htm" => (
            "language-icon-html",
            include_bytes!("../../../../../../assets/icons/html.svg").as_slice(),
        ),
        "java" => (
            "language-icon-java",
            include_bytes!("../../../../../../assets/icons/java.svg").as_slice(),
        ),
        "kotlin" => (
            "language-icon-kotlin",
            include_bytes!("../../../../../../assets/icons/kotlin.svg").as_slice(),
        ),
        "lua" => (
            "language-icon-lua",
            include_bytes!("../../../../../../assets/icons/lua.svg").as_slice(),
        ),
        "php" => (
            "language-icon-php",
            include_bytes!("../../../../../../assets/icons/php.svg").as_slice(),
        ),
        "r" => (
            "language-icon-r",
            include_bytes!("../../../../../../assets/icons/r.svg").as_slice(),
        ),
        "ruby" => (
            "language-icon-ruby",
            include_bytes!("../../../../../../assets/icons/ruby.svg").as_slice(),
        ),
        "swift" => (
            "language-icon-swift",
            include_bytes!("../../../../../../assets/icons/swift.svg").as_slice(),
        ),
        "json" => (
            "language-icon-json",
            include_bytes!("../../../../../../assets/icons/json.svg").as_slice(),
        ),
        "yaml" | "yml" => (
            "language-icon-yaml",
            include_bytes!("../../../../../../assets/icons/yaml.svg").as_slice(),
        ),
        "sql" => (
            "language-icon-sql",
            include_bytes!("../../../../../../assets/icons/sql.svg").as_slice(),
        ),
        "diff" | "patch" => (
            "language-icon-diff",
            include_bytes!("../../../../../../assets/icons/diff.svg").as_slice(),
        ),
        "bash" | "shell" | "zsh" => (
            "language-icon-bash",
            include_bytes!("../../../../../../assets/icons/bash.svg").as_slice(),
        ),
        "toml" => (
            "language-icon-toml",
            include_bytes!("../../../../../../assets/icons/toml.svg").as_slice(),
        ),
        _ => return None,
    })
}

fn code_language_combobox_style(theme: GuiTheme) -> ComboboxStyle {
    ComboboxStyle {
        background: theme.code_toolbar_background,
        hover: theme.code_toolbar_hover,
        border: theme.code_toolbar_border,
        text: theme.code_toolbar_text,
        muted: theme.muted,
        accent: theme.code_toolbar_hover,
        focused_border: theme.focused,
        trigger_height: px(V1_CODE_TOOLBAR_BUTTON_SIZE_PX),
        trigger_min_width: px(V1_CODE_LANGUAGE_BUTTON_MIN_WIDTH_PX),
        trigger_max_width: px(V1_CODE_LANGUAGE_BUTTON_MAX_WIDTH_PX),
        trigger_radius: px(V1_CODE_TOOLBAR_BUTTON_RADIUS_PX),
        popup_width: px(code_language_popup_width()),
        popup_gap: px(V1_CODE_LANGUAGE_POPUP_GAP_PX),
        popup_radius: px(8.0),
        search_height: px(V1_CODE_LANGUAGE_SEARCH_HEIGHT_PX),
        row_height: px(code_language_suggestion_row_height()),
    }
}

fn render_language_search_input(
    block_id: BlockId,
    theme: GuiTheme,
    draft: String,
    caret_offset: Option<usize>,
    marked_range: Option<std::ops::Range<usize>>,
    code_language_focus: FocusHandle,
    view: Entity<CditorV2View>,
) -> AnyElement {
    const SEARCH: &[u8] = include_bytes!("../../../../../../assets/icons/search.svg");
    let activate_view = view.clone();
    let input = mobile_manual_focus(
        div()
            .id(("code-language-search", block_id))
            .h(px(V1_CODE_LANGUAGE_SEARCH_HEIGHT_PX))
            .px(px(8.0))
            .border_b_1()
            .border_color(rgb(theme.code_toolbar_border))
            .child(
                div()
                    .w_full()
                    .h_full()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .track_focus(&code_language_focus)
                    .child(
                        SvgIcon::new("combobox-search", SEARCH)
                            .color(rgb(theme.muted))
                            .size(px(14.0)),
                    )
                    .child(SingleLineTextInputElement {
                        handler: view,
                        focus: code_language_focus,
                        value: draft,
                        placeholder: Some("搜索语言…".to_owned()),
                        caret_offset,
                        marked_range,
                        text_color: theme.text,
                        placeholder_color: theme.muted,
                        caret_color: theme.focused,
                        font_size: px(SINGLE_LINE_INPUT_FONT_SIZE_PX),
                    }),
            ),
    );
    on_text_activation(input, move |_event, _window, cx| {
        activate_view.update(cx, |view, cx| {
            view.request_code_language_focus_from_gui(block_id, cx);
        });
        cx.stop_propagation();
    })
    .into_any_element()
}

fn code_language_popup_width() -> f32 {
    V1_CODE_LANGUAGE_EDIT_WIDTH_PX + 2.0
}

#[cfg(test)]
fn code_language_popup_max_height() -> f32 {
    V1_CODE_LANGUAGE_SEARCH_HEIGHT_PX
        + CODE_LANGUAGE_VISIBLE_SUGGESTIONS as f32 * code_language_suggestion_row_height()
}

#[cfg(test)]
fn code_language_panel_height(total_suggestions: usize) -> f32 {
    V1_CODE_LANGUAGE_SEARCH_HEIGHT_PX + code_language_list_height(total_suggestions)
}

#[cfg(test)]
fn code_language_list_height(total_suggestions: usize) -> f32 {
    total_suggestions.clamp(1, CODE_LANGUAGE_VISIBLE_SUGGESTIONS) as f32
        * code_language_suggestion_row_height()
}

fn code_language_suggestion_row_height() -> f32 {
    24.0
}

#[cfg(test)]
fn code_language_popup_right_overhang() -> f32 {
    0.0
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
        .child(render_copy_icon(theme, block_id, view.clone()))
        .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
            view.update(cx, |view, cx| {
                super::actions::copy_code_block_from_gui(view, block_id, cx);
            });
            cx.stop_propagation();
        })
        .into_any_element()
}

fn render_copy_icon(theme: GuiTheme, block_id: BlockId, view: Entity<CditorV2View>) -> AnyElement {
    const COPY: &[u8] = include_bytes!("../../../../../../assets/icons/copy.svg");
    const COPY_CHECK: &[u8] = include_bytes!("../../../../../../assets/icons/copy-check.svg");

    SvgIcon::dynamic(move |cx| {
        let copied = view.read(cx).overlay.code_copy_feedback_block_id == Some(block_id);
        if copied {
            ("code-toolbar-copy-check", COPY_CHECK)
        } else {
            ("code-toolbar-copy", COPY)
        }
    })
    .color(rgb(theme.code_toolbar_icon))
    .size(px(V1_CODE_COPY_ICON_SIZE_PX))
    .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v1_code_toolbar_geometry_constants_match_editor2() {
        assert_eq!(V1_CODE_TOOLBAR_TOP_PX, 3.0);
        assert_eq!(V1_CODE_TOOLBAR_LEFT_PX, 6.0);
        assert_eq!(V1_CODE_TOOLBAR_RIGHT_PX, 6.0);
        assert_eq!(V1_CODE_TOOLBAR_HEIGHT_PX, 30.0);
        assert_eq!(V1_CODE_TOOLBAR_RADIUS_PX, 4.0);
        assert_eq!(V1_CODE_TOOLBAR_BUTTON_SIZE_PX, 26.0);
        assert_eq!(V1_CODE_TOOLBAR_BUTTON_RADIUS_PX, 4.0);
        assert_eq!(V1_CODE_LANGUAGE_BUTTON_MIN_WIDTH_PX, 160.0);
        assert_eq!(V1_CODE_LANGUAGE_BUTTON_MAX_WIDTH_PX, 160.0);
        assert_eq!(V1_CODE_LANGUAGE_EDIT_WIDTH_PX, 160.0);
        assert_eq!(V1_CODE_TOOLBAR_GAP_PX, 2.0);
        assert_eq!(V1_CODE_LANGUAGE_POPUP_GAP_PX, 6.0);
        assert_eq!(V1_CODE_LANGUAGE_POPUP_MAX_HEIGHT_PX, 300.0);
        assert_eq!(V1_CODE_LANGUAGE_SEARCH_HEIGHT_PX, 32.0);
        assert_eq!(V1_CODE_COPY_ICON_SIZE_PX, 16.0);
    }

    #[test]
    fn code_toolbar_controls_are_centered_inside_the_reserved_surface() {
        assert_eq!(
            V1_CODE_TOOLBAR_TOP_PX * 2.0 + V1_CODE_TOOLBAR_HEIGHT_PX,
            cditor_core::layout::V1_CODE_TOOLBAR_SURFACE_HEIGHT_PX as f32
        );
        assert_eq!(
            V1_CODE_TOOLBAR_BUTTON_SIZE_PX + V1_CODE_TOOLBAR_PADDING_PX * 2.0,
            V1_CODE_TOOLBAR_HEIGHT_PX
        );
    }

    #[test]
    fn code_toolbar_is_fixed_visible_without_a_hover_gate() {
        assert_eq!(V1_CODE_TOOLBAR_OPACITY, 1.0);
    }

    #[test]
    fn language_combobox_uses_the_basic_single_select_fixed_width() {
        assert_eq!(
            V1_CODE_LANGUAGE_BUTTON_MIN_WIDTH_PX,
            V1_CODE_LANGUAGE_BUTTON_MAX_WIDTH_PX
        );
        assert_eq!(
            code_language_popup_width(),
            V1_CODE_LANGUAGE_BUTTON_MAX_WIDTH_PX + 2.0
        );
    }

    #[test]
    fn language_popup_matches_toolbar_width() {
        assert_eq!(code_language_popup_width(), 162.0);
        assert_eq!(code_language_popup_right_overhang(), 0.0);
    }

    #[test]
    fn language_popup_height_is_bounded_to_visible_rows() {
        assert_eq!(code_language_suggestion_row_height(), 24.0);
        assert_eq!(code_language_popup_max_height(), 296.0);
        assert_eq!(code_language_panel_height(0), 56.0);
        assert_eq!(code_language_panel_height(3), 104.0);
        assert!(code_language_popup_max_height() < V1_CODE_LANGUAGE_POPUP_MAX_HEIGHT_PX);
    }

    #[test]
    fn unknown_code_theme_falls_back_to_github_light() {
        assert_eq!(
            crate::features::code::highlight::code_theme_item("missing").id,
            "github_light"
        );
    }

    #[test]
    fn language_icon_source_maps_available_logos_case_insensitively() {
        assert!(language_icon_source("TypeScript").is_some());
        assert!(language_icon_source("cpp").is_some());
        assert!(language_icon_source("PYTHON").is_some());
        assert!(language_icon_source("rust").is_some());
        assert!(language_icon_source("rs").is_some());
        assert!(language_icon_source("jsx").is_some());
        assert!(language_icon_source("json").is_some());
        assert!(language_icon_source("yaml").is_some());
        assert!(language_icon_source("sql").is_some());
        assert!(language_icon_source("diff").is_some());
        assert!(language_icon_source("java").is_some());
        assert!(language_icon_source("bash").is_some());
        assert!(language_icon_source("shell").is_some());
        assert!(language_icon_source("zsh").is_some());
        assert!(language_icon_source("toml").is_some());
        assert!(language_icon_source("plain text").is_none());
    }

    #[test]
    fn every_selectable_highlight_language_has_an_icon() {
        let missing = crate::input::code_language::code_language_items()
            .into_iter()
            .filter(|item| item.value != "plain text")
            .filter(|item| language_icon_source(&item.value).is_none())
            .map(|item| item.value)
            .collect::<Vec<_>>();

        assert!(missing.is_empty(), "languages without icons: {missing:?}");
    }
}
