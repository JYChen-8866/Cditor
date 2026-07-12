use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Entity, FocusHandle, FontWeight, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Styled, deferred, div, px, rgb,
};

use crate::gui::GuiTheme;
use crate::gui::app::CditorV2View;
use crate::gui::input::{
    AiPromptState, SINGLE_LINE_INPUT_FONT_SIZE_PX, SingleLineTextInputElement,
};
use cditor_core::ids::BlockId;

const TOOLBAR_WIDTH_PX: f32 = 194.0;
const TOOLBAR_HEIGHT_PX: f32 = 270.0;
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
    pub show_delete: bool,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub code: bool,
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
}

pub fn floating_toolbar_position(
    selection_left: f32,
    selection_top: f32,
    selection_right: f32,
    selection_bottom: f32,
    viewport_width: f32,
    viewport_height: f32,
) -> (f32, f32) {
    const SELECTION_GAP_PX: f32 = 8.0;
    let selection_center = (selection_left + selection_right) / 2.0;
    let max_x = (viewport_width - TOOLBAR_WIDTH_PX - VIEWPORT_MARGIN_PX).max(VIEWPORT_MARGIN_PX);
    let x = (selection_center - TOOLBAR_WIDTH_PX / 2.0).clamp(VIEWPORT_MARGIN_PX, max_x);
    let above = selection_top - TOOLBAR_HEIGHT_PX - SELECTION_GAP_PX;
    let below = selection_bottom + SELECTION_GAP_PX;
    let max_y = (viewport_height - TOOLBAR_HEIGHT_PX - VIEWPORT_MARGIN_PX).max(VIEWPORT_MARGIN_PX);
    let y = if above >= VIEWPORT_MARGIN_PX {
        above
    } else {
        below.clamp(VIEWPORT_MARGIN_PX, max_y)
    };
    (x, y)
}

pub fn render_floating_toolbar(
    state: FloatingToolbarState,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
    prompt: Option<&AiPromptState>,
    prompt_focus: FocusHandle,
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
        .child(render_block_format_header(theme))
        .when(state.has_text_selection || state.show_delete, |this| {
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
        .child(render_ai_actions(theme, view.clone()))
        .child(render_custom_ai_button(
            theme,
            view.clone(),
            state.x,
            state.y,
            prompt,
            prompt_focus,
        ))
        .when(state.show_delete, |this| {
            this.child(toolbar_divider(theme))
                .child(render_delete_action(theme, view.clone(), state.block_id))
        });

    deferred(panel).with_priority(130).into_any_element()
}

fn render_delete_action(
    theme: GuiTheme,
    view: Entity<CditorV2View>,
    block_id: Option<BlockId>,
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
        .text_color(rgb(theme.danger))
        .cursor_pointer()
        .hover(|style| style.bg(rgb(theme.hover_surface)))
        .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
            if let Some(block_id) = block_id {
                let _ = view.update(cx, |view, cx| {
                    view.delete_block_from_gui(block_id, cx);
                });
            }
            cx.stop_propagation();
        })
        .child("删除")
        .into_any_element()
}

fn render_block_format_header(theme: GuiTheme) -> AnyElement {
    div()
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
            render_format_button(InlineFormatAction::Code, "</>", state, theme, view),
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
        .text_color(rgb(if state.action_active(action) {
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
        .cursor_pointer()
        .hover(|style| style.bg(rgb(theme.hover_surface)))
        .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
            let _ = view.update(cx, |view, cx| {
                view.apply_inline_format_from_toolbar(action, state.has_text_selection, cx);
            });
            cx.stop_propagation();
        })
        .child(label)
        .into_any_element()
}

fn render_ai_actions(theme: GuiTheme, view: Entity<CditorV2View>) -> AnyElement {
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
                .text_color(rgb(theme.text))
                .cursor_pointer()
                .hover(|style| style.bg(rgb(theme.hover_surface)))
                .on_mouse_down(MouseButton::Left, {
                    let view = view.clone();
                    let instruction = (*instruction).to_owned();
                    move |_event, _window, cx| {
                        let _ = view.update(cx, |view, cx| {
                            view.submit_ai_prompt_instruction_from_gui(instruction.clone(), cx)
                        });
                        cx.stop_propagation();
                    }
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
    if let Some(prompt) = prompt {
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
mod tests {
    use super::*;

    #[test]
    fn toolbar_prefers_above_selection_and_clamps_to_viewport() {
        assert_eq!(
            floating_toolbar_position(100.0, 420.0, 180.0, 444.0, 800.0, 600.0),
            (43.0, 142.0),
        );
        assert_eq!(
            floating_toolbar_position(0.0, 12.0, 20.0, 32.0, 200.0, 100.0),
            (10.0, 10.0),
        );
    }

    #[test]
    fn toolbar_state_reports_each_active_action() {
        let state = FloatingToolbarState {
            x: 0.0,
            y: 0.0,
            block_id: None,
            has_text_selection: true,
            show_delete: false,
            bold: true,
            italic: false,
            underline: true,
            strike: false,
            code: false,
        };
        assert!(state.action_active(InlineFormatAction::Bold));
        assert!(!state.action_active(InlineFormatAction::Italic));
        assert!(state.action_active(InlineFormatAction::Underline));
        assert!(!state.action_active(InlineFormatAction::Strike));
    }
}
