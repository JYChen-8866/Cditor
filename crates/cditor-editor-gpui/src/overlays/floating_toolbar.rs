use cditor_component::{
    InteractiveScrollbar, InteractiveScrollbarStyle, POPUP_MENU_ITEM_FONT_SIZE_PX,
    POPUP_MENU_LABEL_FONT_SIZE_PX, PopupMenu, PopupMenuItem, PopupMenuStyle, ScrollbarAxis,
    SvgIcon,
};
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Context, Entity, FocusHandle, FontWeight, InteractiveElement, IntoElement,
    MouseButton, ParentElement, ScrollHandle, StatefulInteractiveElement, Styled, deferred, div,
    px, rgb,
};

use crate::editor_view::CditorV2View;
use crate::input::{AiPromptState, SINGLE_LINE_INPUT_FONT_SIZE_PX, SingleLineTextInputElement};
use crate::menu_metrics::PRIMARY_MENU_WIDTH_PX;
use crate::theme::GuiTheme;
use cditor_core::ids::BlockId;
use cditor_core::rich_text::CalloutVariant;

use super::block_transform_menu::{
    BlockTransformAction, BlockTransformAvailability, render_block_transform_menu,
};
use super::color_menu::{ActiveColor, ColorMenuAction, PaletteColor, render_color_menu};

pub(crate) const TOOLBAR_WIDTH_PX: f32 = 194.0;
pub(crate) const GUTTER_MENU_WIDTH_PX: f32 = PRIMARY_MENU_WIDTH_PX;
const TOOLBAR_HEIGHT_PX: f32 = 324.0;
const GUTTER_MENU_HEIGHT_PX: f32 = 420.0;
const VIEWPORT_MARGIN_PX: f32 = 10.0;
const TOOLBAR_ANCHOR_GAP_PX: f32 = 8.0;
const AI_ACTIONS_VIEWPORT_HEIGHT_PX: f32 = 110.0;
const AI_ACTION_ROW_HEIGHT_PX: f32 = 36.0;
const AI_ACTION_COUNT: usize = 6;
const FORMAT_ICON_SIZE_PX: f32 = 24.0;
const GUTTER_FORMAT_ROW_PADDING_PX: f32 = 8.0;
const FORMAT_BUTTON_SIZE_PX: f32 = 30.0;
const GUTTER_FORMAT_BUTTON_SIZE_PX: f32 = 36.0;
const TOOLBAR_GROUP_LABEL_HEIGHT_PX: f32 = 26.0;

const ICON_COLOR: &[u8] = include_bytes!("../../../../assets/icons/color.svg");
const ICON_BOLD: &[u8] = include_bytes!("../../../../assets/icons/bold.svg");
const ICON_ITALIC: &[u8] = include_bytes!("../../../../assets/icons/itaic.svg");
const ICON_UNDERLINE: &[u8] = include_bytes!("../../../../assets/icons/underline.svg");
const ICON_STRIKETHROUGH: &[u8] = include_bytes!("../../../../assets/icons/strikethrough.svg");
const ICON_INLINE_CODE: &[u8] = include_bytes!("../../../../assets/icons/inlie-code.svg");
const ICON_SUBMENU_ARROW: &[u8] = include_bytes!("../../../../assets/icons/jiantou.svg");
const ICON_AI_IMPROVE: &[u8] = include_bytes!("../../../../assets/icons/ai-improve.svg");
const ICON_AI_PROOFREAD: &[u8] = include_bytes!("../../../../assets/icons/ai-proofread.svg");
const ICON_AI_SHORTEN: &[u8] = include_bytes!("../../../../assets/icons/ai-shorten.svg");
const ICON_AI_EXPAND: &[u8] = include_bytes!("../../../../assets/icons/ai-expand.svg");
const ICON_AI_EXPLAIN: &[u8] = include_bytes!("../../../../assets/icons/ai-explain.svg");
const ICON_AI_TRANSLATE: &[u8] = include_bytes!("../../../../assets/icons/ai-translate.svg");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InlineFormatAction {
    Bold,
    Italic,
    Underline,
    Strike,
    Code,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FloatingToolbarState {
    pub x: f32,
    pub y: f32,
    pub block_id: Option<BlockId>,
    pub has_text_selection: bool,
    pub show_inline_format: bool,
    pub show_color: bool,
    pub show_delete: bool,
    pub inline_format_enabled: bool,
    pub color_enabled: bool,
    pub ai_enabled: bool,
    pub delete_enabled: bool,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub code: bool,
    pub block_transform: Option<BlockTransformAction>,
    pub callout_variant: Option<CalloutVariant>,
    pub block_transform_availability: BlockTransformAvailability,
    pub transform_menu_opens_left: bool,
    pub transform_menu_top_offset: f32,
    pub block_transform_menu_open: bool,
    pub text_color: ActiveColor,
    pub background_color: ActiveColor,
    pub color_menu_opens_left: bool,
    pub color_menu_top_offset: f32,
    pub color_menu_height: f32,
    pub color_menu_open: bool,
    pub last_color_action: Option<ColorMenuAction>,
}

impl FloatingToolbarState {
    pub fn action_active(self, action: InlineFormatAction) -> bool {
        match action {
            InlineFormatAction::Bold => self.bold,
            InlineFormatAction::Italic => self.italic,
            InlineFormatAction::Underline => self.underline,
            InlineFormatAction::Strike => self.strike,
            InlineFormatAction::Code => self.code,
        }
    }

    pub fn action_enabled(self, _action: InlineFormatAction) -> bool {
        self.inline_format_enabled
    }
}

pub fn floating_toolbar_position(
    selection_left: f32,
    selection_top: f32,
    selection_right: f32,
    selection_bottom: f32,
    viewport_width: f32,
    viewport_height: f32,
) -> (f32, f32) {
    let selection_center = (selection_left + selection_right) / 2.0;
    let x = clamp_toolbar_x(selection_center - TOOLBAR_WIDTH_PX / 2.0, viewport_width);
    let y = floating_toolbar_y(selection_top, selection_bottom, viewport_height);
    (x, y)
}

#[cfg(test)]
fn left_aligned_floating_toolbar_position(
    anchor_left: f32,
    anchor_top: f32,
    anchor_bottom: f32,
    viewport_width: f32,
    viewport_height: f32,
) -> (f32, f32) {
    let x = clamp_toolbar_x(anchor_left, viewport_width);
    let y = floating_toolbar_y(anchor_top, anchor_bottom, viewport_height);
    (x, y)
}

pub fn gutter_floating_toolbar_position(
    gutter_left: f32,
    gutter_top: f32,
    viewport_width: f32,
    viewport_height: f32,
) -> (f32, f32) {
    let x = clamp_gutter_menu_x(
        gutter_left - TOOLBAR_ANCHOR_GAP_PX - GUTTER_MENU_WIDTH_PX,
        viewport_width,
    );
    let max_y =
        (viewport_height - GUTTER_MENU_HEIGHT_PX - VIEWPORT_MARGIN_PX).max(VIEWPORT_MARGIN_PX);
    let y = gutter_top.clamp(VIEWPORT_MARGIN_PX, max_y);
    (x, y)
}

fn clamp_toolbar_x(x: f32, viewport_width: f32) -> f32 {
    let max_x = (viewport_width - TOOLBAR_WIDTH_PX - VIEWPORT_MARGIN_PX).max(VIEWPORT_MARGIN_PX);
    x.clamp(VIEWPORT_MARGIN_PX, max_x)
}

fn clamp_gutter_menu_x(x: f32, viewport_width: f32) -> f32 {
    let max_x =
        (viewport_width - GUTTER_MENU_WIDTH_PX - VIEWPORT_MARGIN_PX).max(VIEWPORT_MARGIN_PX);
    x.clamp(VIEWPORT_MARGIN_PX, max_x)
}

fn floating_toolbar_y(anchor_top: f32, anchor_bottom: f32, viewport_height: f32) -> f32 {
    let above = anchor_top - TOOLBAR_HEIGHT_PX - TOOLBAR_ANCHOR_GAP_PX;
    let below = anchor_bottom + TOOLBAR_ANCHOR_GAP_PX;
    let max_y = (viewport_height - TOOLBAR_HEIGHT_PX - VIEWPORT_MARGIN_PX).max(VIEWPORT_MARGIN_PX);
    if above >= VIEWPORT_MARGIN_PX {
        above
    } else {
        below.clamp(VIEWPORT_MARGIN_PX, max_y)
    }
}

pub fn render_floating_toolbar(
    state: FloatingToolbarState,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
    prompt: Option<&AiPromptState>,
    prompt_focus: FocusHandle,
    color_scroll_handle: &ScrollHandle,
    ai_actions_scroll_handle: &ScrollHandle,
) -> AnyElement {
    let panel = div()
        .absolute()
        .left(px(state.x))
        .top(px(state.y))
        .w(px(TOOLBAR_WIDTH_PX))
        .h(px(TOOLBAR_HEIGHT_PX))
        .p(px(8.0))
        .flex()
        .flex_col()
        .gap(px(4.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(rgb(theme.border))
        .bg(rgb(theme.panel))
        .shadow_lg()
        .occlude()
        .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
            cx.stop_propagation();
        })
        .when(
            floating_toolbar_dismisses_on_mouse_down_out(state),
            |panel| {
                panel.on_mouse_down_out({
                    let view = view.clone();
                    move |_event, _window, cx| {
                        view.update(cx, |view, cx| {
                            dismiss_gutter_toolbar_from_gui(view, cx);
                        });
                    }
                })
            },
        )
        .child(render_block_format_header(
            state,
            theme,
            view.clone(),
            None,
            false,
        ))
        .when(state.show_color, |this| {
            this.child(render_color_trigger(
                state,
                theme,
                view.clone(),
                color_scroll_handle,
                false,
            ))
        })
        .when(state.show_inline_format || state.show_delete, |this| {
            this.child(render_inline_format_row(state, theme, view.clone(), false))
        })
        .child(toolbar_divider(theme))
        .child(
            div()
                .text_size(px(POPUP_MENU_LABEL_FONT_SIZE_PX))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(theme.muted))
                .child("AI"),
        )
        .child(render_ai_actions(
            theme,
            view.clone(),
            state.ai_enabled,
            ai_actions_scroll_handle,
        ))
        .child(render_custom_ai_button(
            theme,
            view.clone(),
            state.x,
            state.y,
            state.ai_enabled,
            prompt,
            prompt_focus,
        ))
        .when(state.show_delete, |this| {
            this.child(toolbar_divider(theme))
                .child(render_delete_action(
                    theme,
                    view.clone(),
                    state.block_id,
                    state.delete_enabled,
                    false,
                ))
        });

    deferred(panel).with_priority(130).into_any_element()
}

pub fn gutter_popup_menu_style(theme: GuiTheme) -> PopupMenuStyle {
    PopupMenuStyle {
        background: rgb(theme.panel).into(),
        foreground: rgb(theme.text).into(),
        muted_foreground: rgb(theme.muted).into(),
        border: rgb(theme.border).into(),
        selected_background: rgb(theme.hover_surface).into(),
        selected_foreground: rgb(theme.text).into(),
        radius: px(4.0),
    }
}

pub fn update_gutter_popup_menu(
    menu: &mut PopupMenu,
    state: FloatingToolbarState,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
    prompt: Option<AiPromptState>,
    prompt_focus: FocusHandle,
    color_scroll_handle: ScrollHandle,
    ai_actions_scroll_handle: ScrollHandle,
    block_transform_popup_menu: Option<Entity<PopupMenu>>,
) {
    menu.set_style(gutter_popup_menu_style(theme));
    menu.replace_items([PopupMenuItem::element(move |_window, _cx| {
        render_gutter_popup_content(
            state,
            theme,
            view.clone(),
            prompt.as_ref(),
            prompt_focus.clone(),
            &color_scroll_handle,
            &ai_actions_scroll_handle,
            block_transform_popup_menu.clone(),
        )
    })]);
}

pub fn render_gutter_popup_menu(
    state: FloatingToolbarState,
    menu: Entity<PopupMenu>,
) -> AnyElement {
    deferred(
        div()
            .absolute()
            .left(px(state.x))
            .top(px(state.y))
            .w(px(GUTTER_MENU_WIDTH_PX))
            .child(menu),
    )
    .with_priority(130)
    .into_any_element()
}

fn render_gutter_popup_content(
    state: FloatingToolbarState,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
    prompt: Option<&AiPromptState>,
    prompt_focus: FocusHandle,
    color_scroll_handle: &ScrollHandle,
    ai_actions_scroll_handle: &ScrollHandle,
    block_transform_popup_menu: Option<Entity<PopupMenu>>,
) -> AnyElement {
    div()
        .w_full()
        .p(px(4.0))
        .flex()
        .flex_col()
        .gap(px(4.0))
        .child(toolbar_group_label("文字样式", theme))
        .child(render_inline_format_row(state, theme, view.clone(), true))
        .child(toolbar_divider(theme))
        .child(render_block_format_header(
            state,
            theme,
            view.clone(),
            block_transform_popup_menu,
            true,
        ))
        .when(state.show_color, |this| {
            this.child(render_color_trigger(
                state,
                theme,
                view.clone(),
                color_scroll_handle,
                true,
            ))
        })
        .child(toolbar_divider(theme))
        .child(toolbar_group_label("AI", theme))
        .child(render_ai_actions(
            theme,
            view.clone(),
            state.ai_enabled,
            ai_actions_scroll_handle,
        ))
        .child(render_custom_ai_button(
            theme,
            view.clone(),
            state.x,
            state.y,
            state.ai_enabled,
            prompt,
            prompt_focus,
        ))
        .child(toolbar_divider(theme))
        .child(render_delete_action(
            theme,
            view,
            state.block_id,
            state.delete_enabled,
            true,
        ))
        .into_any_element()
}

fn render_color_trigger(
    state: FloatingToolbarState,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
    scroll_handle: &ScrollHandle,
    slash_style: bool,
) -> AnyElement {
    let row = div()
        .id("floating-toolbar-color-trigger")
        .h(px(if slash_style { 48.0 } else { 28.0 }))
        .w_full()
        .px(px(if slash_style { 8.0 } else { 6.0 }))
        .flex()
        .items_center()
        .gap(px(if slash_style { 10.0 } else { 8.0 }))
        .rounded(px(4.0))
        .bg(rgb(if state.color_menu_open {
            theme.action_background
        } else {
            theme.panel
        }))
        .text_color(rgb(if state.color_enabled {
            theme.text
        } else {
            theme.muted
        }))
        .when(!state.color_enabled, |row| row.opacity(0.45))
        .when(state.color_enabled, |row| {
            let click_view = view.clone();
            row.cursor_pointer()
                .hover(|style| style.bg(rgb(theme.hover_surface)))
                .on_hover({
                    let view = view.clone();
                    move |hovered, _window, cx| {
                        view.update(cx, |view, cx| {
                            view.set_color_menu_hovered(*hovered, cx);
                        });
                    }
                })
                .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
                    let _ = click_view.update(cx, |view, cx| view.open_color_menu_from_gui(cx));
                    cx.stop_propagation();
                })
        })
        .child(if slash_style {
            div()
                .flex_none()
                .size(px(36.0))
                .rounded(px(4.0))
                .border_1()
                .border_color(rgb(theme.border))
                .flex()
                .items_center()
                .justify_center()
                .child(
                    SvgIcon::new("gutter-menu-color", ICON_COLOR)
                        .color(rgb(if state.color_enabled {
                            theme.text
                        } else {
                            theme.muted
                        }))
                        .size(px(FORMAT_ICON_SIZE_PX)),
                )
                .into_any_element()
        } else {
            render_current_color_swatch(state, theme, 22.0)
        })
        .child(if slash_style {
            div()
                .min_w(px(0.0))
                .flex_1()
                .flex()
                .flex_col()
                .gap(px(1.0))
                .child(div().text_size(px(14.0)).child("颜色"))
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(rgb(theme.muted))
                        .child("设置文字与背景颜色"),
                )
                .into_any_element()
        } else {
            div()
                .flex_1()
                .text_size(px(13.0))
                .child("颜色")
                .into_any_element()
        })
        .child(render_submenu_arrow("gutter-color-arrow", theme));
    div()
        .relative()
        .child(row)
        .when(state.color_enabled && state.color_menu_open, |this| {
            this.child(render_color_menu(state, theme, view, scroll_handle))
        })
        .into_any_element()
}

fn render_current_color_swatch(
    state: FloatingToolbarState,
    theme: GuiTheme,
    size: f32,
) -> AnyElement {
    let text_color = match state.text_color {
        ActiveColor::Palette(color) => palette_text_swatch(color),
        ActiveColor::Default | ActiveColor::Mixed => theme.text,
    };
    let background = match state.background_color {
        ActiveColor::Palette(color) => palette_background_swatch(color),
        ActiveColor::Default | ActiveColor::Mixed => theme.panel,
    };
    div()
        .size(px(size))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(4.0))
        .border_1()
        .border_color(rgb(theme.border))
        .bg(rgb(background))
        .text_size(px(14.0))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(text_color))
        .child("A")
        .into_any_element()
}

const fn palette_text_swatch(color: PaletteColor) -> u32 {
    match color {
        PaletteColor::Gray => 0x787774,
        PaletteColor::Brown => 0x9f6b53,
        PaletteColor::Orange => 0xd9730d,
        PaletteColor::Yellow => 0xcb912f,
        PaletteColor::Green => 0x448361,
        PaletteColor::Blue => 0x337ea9,
        PaletteColor::Purple => 0x9065b0,
        PaletteColor::Pink => 0xc14c8a,
        PaletteColor::Red => 0xd44c47,
    }
}

const fn palette_background_swatch(color: PaletteColor) -> u32 {
    match color {
        PaletteColor::Gray => 0xf1f1ef,
        PaletteColor::Brown => 0xf4eeee,
        PaletteColor::Orange => 0xfbecdd,
        PaletteColor::Yellow => 0xfbf3db,
        PaletteColor::Green => 0xedf3ec,
        PaletteColor::Blue => 0xe7f3f8,
        PaletteColor::Purple => 0xf4f0f7,
        PaletteColor::Pink => 0xf9eef3,
        PaletteColor::Red => 0xfdebec,
    }
}

fn render_delete_action(
    theme: GuiTheme,
    view: Entity<CditorV2View>,
    block_id: Option<BlockId>,
    enabled: bool,
    slash_style: bool,
) -> AnyElement {
    let row = div()
        .id("floating-toolbar-delete")
        .h(px(if slash_style { 48.0 } else { 28.0 }))
        .w_full()
        .px(px(if slash_style { 8.0 } else { 7.0 }))
        .flex()
        .items_center()
        .gap(px(if slash_style { 10.0 } else { 0.0 }))
        .rounded(px(4.0))
        .text_color(rgb(if enabled { theme.danger } else { theme.muted }))
        .when(!enabled, |row| row.opacity(0.45))
        .when(enabled, |row| {
            row.cursor_pointer()
                .hover(|style| style.bg(rgb(theme.hover_surface)))
                .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
                    if let Some(block_id) = block_id {
                        view.update(cx, |view, cx| {
                            view.delete_block_from_gui(block_id, cx);
                        });
                    }
                    cx.stop_propagation();
                })
        });
    if slash_style {
        row.child(
            div()
                .flex_none()
                .size(px(36.0))
                .rounded(px(4.0))
                .border_1()
                .border_color(rgb(theme.border))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(16.0))
                .child("×"),
        )
        .child(
            div()
                .min_w(px(0.0))
                .flex_1()
                .flex()
                .flex_col()
                .gap(px(1.0))
                .child(div().text_size(px(14.0)).child("删除"))
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(rgb(theme.muted))
                        .child("删除当前区块"),
                ),
        )
        .into_any_element()
    } else {
        row.text_size(px(13.0)).child("删除").into_any_element()
    }
}

fn render_block_format_header(
    state: FloatingToolbarState,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
    block_transform_popup_menu: Option<Entity<PopupMenu>>,
    slash_style: bool,
) -> AnyElement {
    let row = div()
        .h(px(if slash_style { 48.0 } else { 28.0 }))
        .w_full()
        .px(px(if slash_style { 8.0 } else { 6.0 }))
        .flex()
        .items_center()
        .gap(px(if slash_style { 10.0 } else { 8.0 }))
        .rounded(px(4.0))
        .bg(rgb(theme.panel))
        .text_color(rgb(theme.text))
        .child(
            div()
                .flex_none()
                .w(px(if slash_style { 36.0 } else { 15.0 }))
                .h(px(if slash_style { 36.0 } else { 20.0 }))
                .when(slash_style, |icon| {
                    icon.rounded(px(4.0))
                        .border_1()
                        .border_color(rgb(theme.border))
                })
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(15.0))
                .child("T"),
        )
        .child(if slash_style {
            div()
                .min_w(px(0.0))
                .flex_1()
                .flex()
                .flex_col()
                .gap(px(1.0))
                .child(div().text_size(px(14.0)).child("文字格式"))
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(rgb(theme.muted))
                        .child("转换当前区块类型"),
                )
                .into_any_element()
        } else {
            div()
                .flex_1()
                .text_size(px(13.0))
                .child("文字格式")
                .into_any_element()
        })
        .when(state.show_delete, |row| {
            row.child(render_submenu_arrow("gutter-transform-arrow", theme))
        });
    let Some(_block_id) = state.block_id.filter(|_| state.show_delete) else {
        return row.into_any_element();
    };
    div()
        .relative()
        .child(
            row.cursor_pointer()
                .hover(|style| style.bg(rgb(theme.hover_surface)))
                .on_mouse_move({
                    let view = view.clone();
                    move |_event, _window, cx| {
                        view.update(cx, |view, cx| {
                            view.open_block_transform_menu_from_gui(cx);
                        });
                    }
                })
                .on_mouse_down(MouseButton::Left, {
                    let view = view.clone();
                    move |_event, _window, cx| {
                        view.update(cx, |view, cx| {
                            view.open_block_transform_menu_from_gui(cx);
                        });
                        cx.stop_propagation();
                    }
                }),
        )
        .when_some(block_transform_popup_menu, |this, menu| {
            this.child(render_block_transform_menu(
                menu,
                state.transform_menu_opens_left,
                state.transform_menu_top_offset,
            ))
        })
        .into_any_element()
}

fn render_inline_format_row(
    state: FloatingToolbarState,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
    use_svg_icons: bool,
) -> AnyElement {
    div()
        .w_full()
        .flex()
        .when(use_svg_icons, |row| {
            row.px(px(GUTTER_FORMAT_ROW_PADDING_PX)).justify_between()
        })
        .when(!use_svg_icons, |row| row.gap(px(4.0)))
        .children([
            render_format_button(
                InlineFormatAction::Bold,
                "B",
                state,
                theme,
                view.clone(),
                use_svg_icons,
            ),
            render_format_button(
                InlineFormatAction::Italic,
                "I",
                state,
                theme,
                view.clone(),
                use_svg_icons,
            ),
            render_format_button(
                InlineFormatAction::Underline,
                "U",
                state,
                theme,
                view.clone(),
                use_svg_icons,
            ),
            render_format_button(
                InlineFormatAction::Strike,
                "Tˣ",
                state,
                theme,
                view.clone(),
                use_svg_icons,
            ),
            render_format_button(
                InlineFormatAction::Code,
                "<>",
                state,
                theme,
                view,
                use_svg_icons,
            ),
        ])
        .into_any_element()
}

fn render_format_button(
    action: InlineFormatAction,
    label: &'static str,
    state: FloatingToolbarState,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
    use_svg_icon: bool,
) -> AnyElement {
    let enabled = state.action_enabled(action);
    let foreground = if !enabled {
        theme.muted
    } else if state.action_active(action) {
        theme.action_accent
    } else {
        theme.text
    };
    div()
        .id(("inline-format", action_index(action)))
        .h(px(if use_svg_icon {
            GUTTER_FORMAT_BUTTON_SIZE_PX
        } else {
            FORMAT_BUTTON_SIZE_PX
        }))
        .when(use_svg_icon, |button| {
            button.w(px(GUTTER_FORMAT_BUTTON_SIZE_PX)).flex_none()
        })
        .when(!use_svg_icon, |button| {
            button.w(px(FORMAT_BUTTON_SIZE_PX)).flex_none()
        })
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(4.0))
        .bg(rgb(if state.action_active(action) {
            theme.action_background
        } else {
            theme.panel
        }))
        .text_color(rgb(foreground))
        .text_size(px(if action == InlineFormatAction::Code {
            11.0
        } else {
            14.0
        }))
        .when(action == InlineFormatAction::Bold, |this| {
            this.font_weight(FontWeight::BOLD)
        })
        .when(action == InlineFormatAction::Italic, |this| this.italic())
        .when(matches!(action, InlineFormatAction::Underline), |this| {
            this.text_decoration_1()
        })
        .when(matches!(action, InlineFormatAction::Strike), |this| {
            this.line_through()
        })
        .when(!enabled, |button| button.opacity(0.45))
        .when(enabled, |button| {
            button
                .cursor_pointer()
                .hover(|style| style.bg(rgb(theme.hover_surface)))
                .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
                    view.update(cx, |view, cx| {
                        view.apply_inline_format_from_toolbar(action, state.has_text_selection, cx);
                    });
                    cx.stop_propagation();
                })
        })
        .child(
            format_icon_source(action)
                .filter(|_| use_svg_icon)
                .map(|(key, source)| {
                    SvgIcon::new(key, source)
                        .color(rgb(foreground))
                        .size(px(FORMAT_ICON_SIZE_PX))
                        .into_any_element()
                })
                .unwrap_or_else(|| div().child(label).into_any_element()),
        )
        .into_any_element()
}

fn format_icon_source(action: InlineFormatAction) -> Option<(&'static str, &'static [u8])> {
    match action {
        InlineFormatAction::Bold => Some(("gutter-format-bold", ICON_BOLD)),
        InlineFormatAction::Italic => Some(("gutter-format-italic", ICON_ITALIC)),
        InlineFormatAction::Underline => Some(("gutter-format-underline", ICON_UNDERLINE)),
        InlineFormatAction::Strike => Some(("gutter-format-strikethrough", ICON_STRIKETHROUGH)),
        InlineFormatAction::Code => Some(("gutter-format-inline-code", ICON_INLINE_CODE)),
    }
}

fn render_submenu_arrow(key: &'static str, theme: GuiTheme) -> AnyElement {
    div()
        .flex_none()
        .size(px(16.0))
        .flex()
        .items_center()
        .justify_center()
        .child(
            SvgIcon::new(key, ICON_SUBMENU_ARROW)
                .color(rgb(theme.muted))
                .size(px(16.0)),
        )
        .into_any_element()
}

fn render_ai_actions(
    theme: GuiTheme,
    view: Entity<CditorV2View>,
    enabled: bool,
    scroll_handle: &ScrollHandle,
) -> AnyElement {
    const ACTIONS: &[(&str, &str, &[u8])] = &[
        ("改进写作", "Improve writing", ICON_AI_IMPROVE),
        ("校对", "Fix spelling and grammar", ICON_AI_PROOFREAD),
        ("缩短", "Make shorter", ICON_AI_SHORTEN),
        ("扩写", "Make longer", ICON_AI_EXPAND),
        ("解释", "Explain this text", ICON_AI_EXPLAIN),
        ("翻译", "Translate this text", ICON_AI_TRANSLATE),
    ];
    debug_assert_eq!(ACTIONS.len(), AI_ACTION_COUNT);
    let estimated_content_height = ACTIONS.len() as f32 * AI_ACTION_ROW_HEIGHT_PX;
    let scroll_view = view.clone();
    let content = div()
        .id("floating-toolbar-ai-actions-scroll")
        .size_full()
        .pr(px(10.0))
        .overflow_y_scroll()
        .track_scroll(scroll_handle)
        .on_scroll_wheel(move |_event, _window, cx| {
            scroll_view.update(cx, |_view, cx| cx.notify());
        })
        .children(ACTIONS.iter().map(|(label, instruction, icon)| {
            div()
                .h(px(AI_ACTION_ROW_HEIGHT_PX))
                .flex_none()
                .w_full()
                .px(px(8.0))
                .flex()
                .items_center()
                .gap(px(10.0))
                .rounded(px(4.0))
                .text_size(px(POPUP_MENU_ITEM_FONT_SIZE_PX))
                .text_color(rgb(if enabled { theme.text } else { theme.muted }))
                .when(!enabled, |row| row.opacity(0.45))
                .when(enabled, |row| {
                    let view = view.clone();
                    let instruction = (*instruction).to_owned();
                    row.cursor_pointer()
                        .hover(|style| style.bg(rgb(theme.hover_surface)))
                        .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
                            let _ = view.update(cx, |view, cx| {
                                view.submit_ai_prompt_instruction_from_gui(instruction.clone(), cx)
                            });
                            cx.stop_propagation();
                        })
                })
                .child(
                    div()
                        .flex_none()
                        .size(px(36.0))
                        .rounded(px(4.0))
                        .border_1()
                        .border_color(rgb(theme.border))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            SvgIcon::new("ai-action-icon", *icon)
                                .color(rgb(if enabled { theme.text } else { theme.muted }))
                                .size(px(FORMAT_ICON_SIZE_PX)),
                        ),
                )
                .child(*label)
                .into_any_element()
        }));

    div()
        .relative()
        .h(px(AI_ACTIONS_VIEWPORT_HEIGHT_PX))
        .w_full()
        .overflow_hidden()
        .child(content)
        .child(
            div()
                .absolute()
                .right_0()
                .top_0()
                .w(px(10.0))
                .h(px(AI_ACTIONS_VIEWPORT_HEIGHT_PX))
                .child(InteractiveScrollbar::for_scroll_handle(
                    ScrollbarAxis::Vertical,
                    scroll_handle.clone(),
                    AI_ACTIONS_VIEWPORT_HEIGHT_PX,
                    estimated_content_height,
                    InteractiveScrollbarStyle::notion(theme.scrollbar, theme.scrollbar_hover),
                )),
        )
        .into_any_element()
}

fn render_custom_ai_button(
    theme: GuiTheme,
    view: Entity<CditorV2View>,
    x: f32,
    y: f32,
    enabled: bool,
    prompt: Option<&AiPromptState>,
    prompt_focus: FocusHandle,
) -> AnyElement {
    let mut input = div()
        .h(px(30.0))
        .w_full()
        .px(px(8.0))
        .flex()
        .items_center()
        .rounded(px(4.0))
        .border_1()
        .border_color(rgb(theme.border))
        .text_size(px(12.0))
        .text_color(rgb(theme.muted));
    if !enabled {
        input = input
            .opacity(0.45)
            .text_color(rgb(theme.muted))
            .child("使用 AI 编辑…");
    } else if let Some(prompt) = prompt {
        input = input
            .track_focus(&prompt_focus)
            .child(SingleLineTextInputElement {
                handler: view,
                focus: prompt_focus,
                value: prompt.draft.clone(),
                placeholder: Some("使用 AI 编辑…".to_owned()),
                caret_offset: Some(prompt.caret_offset),
                marked_range: prompt.marked_range.clone(),
                text_color: theme.text,
                placeholder_color: theme.muted,
                caret_color: theme.focused,
                font_size: px(SINGLE_LINE_INPUT_FONT_SIZE_PX),
            });
    } else {
        input = input
            .cursor_pointer()
            .hover(|style| style.bg(rgb(theme.hover_surface)))
            .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
                view.update(cx, |view, cx| {
                    view.open_ai_prompt_from_gui(x, y, cx);
                });
                cx.stop_propagation();
            })
            .child("使用 AI 编辑…");
    }
    input.into_any_element()
}

fn toolbar_divider(theme: GuiTheme) -> AnyElement {
    div()
        .h(px(1.0))
        .w_full()
        .bg(rgb(theme.border))
        .into_any_element()
}

fn toolbar_group_label(label: &'static str, theme: GuiTheme) -> AnyElement {
    div()
        .h(px(TOOLBAR_GROUP_LABEL_HEIGHT_PX))
        .px(px(8.0))
        .flex()
        .items_center()
        .text_size(px(POPUP_MENU_LABEL_FONT_SIZE_PX))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(theme.muted))
        .child(label)
        .into_any_element()
}

const fn action_index(action: InlineFormatAction) -> usize {
    match action {
        InlineFormatAction::Bold => 0,
        InlineFormatAction::Italic => 1,
        InlineFormatAction::Underline => 2,
        InlineFormatAction::Strike => 3,
        InlineFormatAction::Code => 4,
    }
}

#[cfg(test)]
#[path = "floating_toolbar_tests.rs"]
mod tests;

fn floating_toolbar_dismisses_on_mouse_down_out(state: FloatingToolbarState) -> bool {
    state.show_delete
}

fn dismiss_gutter_toolbar_from_gui(
    view: &mut CditorV2View,
    cx: &mut Context<CditorV2View>,
) -> bool {
    if view.overlay.gutter_toolbar_block_id.is_none() {
        return false;
    }
    view.clear_gutter_action();
    cx.notify();
    true
}
