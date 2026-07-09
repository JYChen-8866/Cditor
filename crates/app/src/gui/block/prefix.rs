use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, App, InteractiveElement, IntoElement, MouseButton, MouseDownEvent, ParentElement,
    Styled, Window, div, px, rgb,
};

use crate::gui::GuiTheme;
use crate::gui::block::chrome::{BLOCK_PREFIX_WIDTH_PX, CALLOUT_PREFIX_WIDTH_PX};
use cditor_core::block::{BlockPrefixSnapshot, bullet_marker_for_depth};
use cditor_core::rich_text::CalloutVariant;

pub type TodoToggleHandler = Box<dyn Fn(&MouseDownEvent, &mut Window, &mut App) + 'static>;

pub fn render_block_prefix(
    prefix: &BlockPrefixSnapshot,
    theme: GuiTheme,
    editable: bool,
    on_todo_toggle: Option<TodoToggleHandler>,
) -> AnyElement {
    match prefix {
        BlockPrefixSnapshot::None => div().w(px(0.0)).flex_shrink_0().into_any_element(),
        BlockPrefixSnapshot::Bullet { depth } => div()
            .w(px(BLOCK_PREFIX_WIDTH_PX))
            .flex_shrink_0()
            .flex()
            .justify_center()
            .text_color(rgb(theme.prefix_text))
            .child(bullet_marker_for_depth(*depth))
            .into_any_element(),
        BlockPrefixSnapshot::Number { ordinal } => div()
            .w(px(BLOCK_PREFIX_WIDTH_PX))
            .flex_shrink_0()
            .flex()
            .justify_center()
            .text_color(rgb(theme.prefix_text))
            .child(format!("{ordinal}."))
            .into_any_element(),
        BlockPrefixSnapshot::Todo { checked } => div()
            .w(px(BLOCK_PREFIX_WIDTH_PX))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .when(editable, |this| this.cursor_pointer())
            .when_some(on_todo_toggle, |this, handler| {
                this.on_mouse_down(MouseButton::Left, handler)
            })
            .child(render_task_checkbox(*checked, theme))
            .into_any_element(),
        BlockPrefixSnapshot::Callout { variant } => div()
            .w(px(CALLOUT_PREFIX_WIDTH_PX))
            .flex_shrink_0()
            .flex()
            .items_start()
            .justify_center()
            .pt(px(1.0))
            .child(
                div()
                    .size(px(24.0))
                    .rounded(px(6.0))
                    .bg(rgb(theme.callout_icon_background))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(15.0))
                    .text_color(rgb(theme.muted))
                    .child(callout_icon(*variant)),
            )
            .into_any_element(),
        BlockPrefixSnapshot::Toggle { collapsed } => div()
            .w(px(BLOCK_PREFIX_WIDTH_PX))
            .flex_shrink_0()
            .flex()
            .justify_center()
            .text_color(rgb(theme.prefix_text))
            .child(if *collapsed { "▸" } else { "▾" })
            .into_any_element(),
    }
}

pub fn render_task_checkbox(checked: bool, theme: GuiTheme) -> AnyElement {
    let border_color = if checked {
        theme.action_accent
    } else {
        theme.checkbox_border
    };
    let background = if checked {
        theme.checkbox_checked_background
    } else {
        theme.page
    };
    div()
        .size(px(16.0))
        .rounded(px(4.0))
        .border_1()
        .border_color(rgb(border_color))
        .bg(rgb(background))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(12.0))
        .text_color(rgb(theme.checkbox_checked_text))
        .child(if checked { "✓" } else { "" })
        .into_any_element()
}

pub fn callout_icon(variant: CalloutVariant) -> &'static str {
    match variant {
        CalloutVariant::Note | CalloutVariant::Info => "ⓘ",
        CalloutVariant::Tip => "💡",
        CalloutVariant::Important => "❗",
        CalloutVariant::Warning => "⚠",
        CalloutVariant::Caution | CalloutVariant::Danger => "⛔",
        CalloutVariant::Success => "✓",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn callout_icons_cover_all_variants() {
        assert_eq!(callout_icon(CalloutVariant::Note), "ⓘ");
        assert_eq!(callout_icon(CalloutVariant::Tip), "💡");
        assert_eq!(callout_icon(CalloutVariant::Warning), "⚠");
        assert_eq!(callout_icon(CalloutVariant::Success), "✓");
    }

    #[test]
    fn prefix_width_constants_match_v1() {
        assert_eq!(BLOCK_PREFIX_WIDTH_PX, 38.0);
        assert_eq!(CALLOUT_PREFIX_WIDTH_PX, 34.0);
    }
}
