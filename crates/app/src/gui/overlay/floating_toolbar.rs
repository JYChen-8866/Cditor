use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Entity, FocusHandle, FontWeight, InteractiveElement, IntoElement, MouseButton,
    ParentElement, ScrollHandle, StatefulInteractiveElement, Styled, deferred, div, px, rgb,
};

use crate::gui::GuiTheme;
use crate::gui::app::CditorV2View;
use crate::gui::input::{
    AiPromptState, SINGLE_LINE_INPUT_FONT_SIZE_PX, SingleLineTextInputElement,
};
use cditor_core::ids::BlockId;

use super::block_transform_menu::{
    BlockTransformAction, BlockTransformAvailability, render_block_transform_menu,
};
use super::color_menu::{ActiveColor, ColorMenuAction, PaletteColor, render_color_menu};

const TOOLBAR_WIDTH_PX: f32 = 194.0;
const TOOLBAR_HEIGHT_PX: f32 = 324.0;
const VIEWPORT_MARGIN_PX: f32 = 10.0;

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

pub fn left_aligned_floating_toolbar_position(
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

fn clamp_toolbar_x(x: f32, viewport_width: f32) -> f32 {
    let max_x = (viewport_width - TOOLBAR_WIDTH_PX - VIEWPORT_MARGIN_PX).max(VIEWPORT_MARGIN_PX);
    x.clamp(VIEWPORT_MARGIN_PX, max_x)
}

fn floating_toolbar_y(anchor_top: f32, anchor_bottom: f32, viewport_height: f32) -> f32 {
    const ANCHOR_GAP_PX: f32 = 8.0;
    let above = anchor_top - TOOLBAR_HEIGHT_PX - ANCHOR_GAP_PX;
    let below = anchor_bottom + ANCHOR_GAP_PX;
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
                        let _ = view.update(cx, |view, cx| {
                            view.dismiss_gutter_toolbar_from_gui(cx);
                        });
                    }
                })
            },
        )
        .child(render_block_format_header(state, theme, view.clone()))
        .when(state.show_color, |this| {
            this.child(render_color_trigger(
                state,
                theme,
                view.clone(),
                color_scroll_handle,
            ))
        })
        .when(state.show_inline_format || state.show_delete, |this| {
            this.child(render_inline_format_row(state, theme, view.clone()))
        })
        .child(toolbar_divider(theme))
        .child(
            div()
                .text_size(px(12.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(theme.muted))
                .child("AI"),
        )
        .child(render_ai_actions(theme, view.clone(), state.ai_enabled))
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
                ))
        });

    deferred(panel).with_priority(130).into_any_element()
}

fn render_color_trigger(
    state: FloatingToolbarState,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
    scroll_handle: &ScrollHandle,
) -> AnyElement {
    let row = div()
        .id("floating-toolbar-color-trigger")
        .h(px(28.0))
        .w_full()
        .px(px(6.0))
        .flex()
        .items_center()
        .gap(px(8.0))
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
                        let _ = view.update(cx, |view, cx| {
                            view.set_color_menu_hovered(*hovered, cx);
                        });
                    }
                })
                .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
                    let _ = click_view.update(cx, |view, cx| view.open_color_menu_from_gui(cx));
                    cx.stop_propagation();
                })
        })
        .child(render_current_color_swatch(state, theme))
        .child(div().flex_1().text_size(px(13.0)).child("颜色"))
        .child(
            div()
                .text_size(px(13.0))
                .text_color(rgb(theme.muted))
                .child("›"),
        );
    div()
        .relative()
        .child(row)
        .when(state.color_enabled && state.color_menu_open, |this| {
            this.child(render_color_menu(state, theme, view, scroll_handle))
        })
        .into_any_element()
}

fn render_current_color_swatch(state: FloatingToolbarState, theme: GuiTheme) -> AnyElement {
    let text_color = match state.text_color {
        ActiveColor::Palette(color) => palette_text_swatch(color),
        ActiveColor::Default | ActiveColor::Mixed => theme.text,
    };
    let background = match state.background_color {
        ActiveColor::Palette(color) => palette_background_swatch(color),
        ActiveColor::Default | ActiveColor::Mixed => theme.panel,
    };
    div()
        .size(px(22.0))
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
) -> AnyElement {
    div()
        .id("floating-toolbar-delete")
        .h(px(28.0))
        .w_full()
        .px(px(7.0))
        .flex()
        .items_center()
        .rounded(px(4.0))
        .text_size(px(13.0))
        .text_color(rgb(if enabled { theme.danger } else { theme.muted }))
        .when(!enabled, |row| row.opacity(0.45))
        .when(enabled, |row| {
            row.cursor_pointer()
                .hover(|style| style.bg(rgb(theme.hover_surface)))
                .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
                    if let Some(block_id) = block_id {
                        let _ = view.update(cx, |view, cx| {
                            view.delete_block_from_gui(block_id, cx);
                        });
                    }
                    cx.stop_propagation();
                })
        })
        .child("删除")
        .into_any_element()
}

fn render_block_format_header(
    state: FloatingToolbarState,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
) -> AnyElement {
    let row = div()
        .h(px(28.0))
        .w_full()
        .px(px(6.0))
        .flex()
        .items_center()
        .gap(px(8.0))
        .rounded(px(4.0))
        .bg(rgb(theme.panel))
        .text_color(rgb(theme.text))
        .child(div().text_size(px(15.0)).child("T"))
        .child(div().flex_1().text_size(px(13.0)).child("文字格式"))
        .when(state.show_delete, |row| {
            row.child(
                div()
                    .text_size(px(13.0))
                    .text_color(rgb(theme.muted))
                    .child("›"),
            )
        });
    let Some(block_id) = state.block_id.filter(|_| state.show_delete) else {
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
                        let _ = view.update(cx, |view, cx| {
                            view.open_block_transform_menu_from_gui(cx);
                        });
                    }
                })
                .on_mouse_down(MouseButton::Left, {
                    let view = view.clone();
                    move |_event, _window, cx| {
                        let _ = view.update(cx, |view, cx| {
                            view.open_block_transform_menu_from_gui(cx);
                        });
                        cx.stop_propagation();
                    }
                }),
        )
        .when(state.block_transform_menu_open, |this| {
            this.child(render_block_transform_menu(
                theme,
                view,
                block_id,
                state.block_transform,
                state.block_transform_availability,
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
) -> AnyElement {
    div()
        .flex()
        .gap(px(4.0))
        .children([
            render_format_button(InlineFormatAction::Bold, "B", state, theme, view.clone()),
            render_format_button(InlineFormatAction::Italic, "I", state, theme, view.clone()),
            render_format_button(
                InlineFormatAction::Underline,
                "U",
                state,
                theme,
                view.clone(),
            ),
            render_format_button(InlineFormatAction::Strike, "Tˣ", state, theme, view.clone()),
            render_format_button(InlineFormatAction::Code, "<>", state, theme, view),
        ])
        .into_any_element()
}

fn render_format_button(
    action: InlineFormatAction,
    label: &'static str,
    state: FloatingToolbarState,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
) -> AnyElement {
    let enabled = state.action_enabled(action);
    div()
        .id(("inline-format", action_index(action)))
        .size(px(30.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(4.0))
        .bg(rgb(if state.action_active(action) {
            theme.action_background
        } else {
            theme.panel
        }))
        .text_color(rgb(if !enabled {
            theme.muted
        } else if state.action_active(action) {
            theme.action_accent
        } else {
            theme.text
        }))
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
                    let _ = view.update(cx, |view, cx| {
                        view.apply_inline_format_from_toolbar(action, state.has_text_selection, cx);
                    });
                    cx.stop_propagation();
                })
        })
        .child(label)
        .into_any_element()
}

fn render_ai_actions(theme: GuiTheme, view: Entity<CditorV2View>, enabled: bool) -> AnyElement {
    const ACTIONS: &[(&str, &str)] = &[
        ("改进写作", "Improve writing"),
        ("校对", "Fix spelling and grammar"),
        ("缩短", "Make shorter"),
        ("扩写", "Make longer"),
        ("解释", "Explain this text"),
        ("翻译", "Translate this text"),
    ];
    div()
        .relative()
        .h(px(110.0))
        .w_full()
        .overflow_hidden()
        .children(ACTIONS.iter().take(4).map(|(label, instruction)| {
            div()
                .h(px(25.0))
                .w_full()
                .px(px(7.0))
                .flex()
                .items_center()
                .rounded(px(4.0))
                .text_size(px(13.0))
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
                .child(*label)
                .into_any_element()
        }))
        .child(
            div()
                .absolute()
                .right(px(3.0))
                .top(px(4.0))
                .w(px(4.0))
                .h(px(48.0))
                .rounded(px(2.0))
                .bg(rgb(theme.scrollbar)),
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
                let _ = view.update(cx, |view, cx| {
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
