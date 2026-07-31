use crate::editor_view::CditorV2View;
use crate::input::CodeLanguageEditState;
use crate::interaction::scrollbar::{
    InternalScrollbarTrackStyle, render_internal_vertical_scrollbar,
};
use crate::platform::EDITOR_MONO_FONT_FAMILY;
use crate::theme::GuiTheme;
use cditor_core::ids::BlockId;
use cditor_core::layout::{
    STRUCTURED_BLOCK_CONTENT_VIEWPORT_MAX_HEIGHT_PX, V1_CODE_CONTENT_PADDING_BOTTOM_PX,
    V1_CODE_CONTENT_PADDING_TOP_PX, V1_CODE_FRAME_BORDER_WIDTH_PX, V1_CODE_INNER_MIN_HEIGHT_PX,
    V1_CODE_SCROLL_END_SPACER_PX, V1_CODE_SURFACE_GAP_PX, V1_CODE_TOOLBAR_SURFACE_HEIGHT_PX,
};
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Entity, FocusHandle, InteractiveElement, IntoElement, ParentElement, ScrollHandle,
    StatefulInteractiveElement, Styled, div, px, rgb,
};

mod actions;
pub(crate) mod highlight;
mod language_actions;
mod toolbar;

use highlight::code_theme_item;
use toolbar::render_code_toolbar;

pub const V1_CODE_BLOCK_MIN_HEIGHT_PX: f32 = V1_CODE_INNER_MIN_HEIGHT_PX as f32;
pub const V1_CODE_BLOCK_RADIUS_PX: f32 = 6.0;
pub const V1_CODE_CONTENT_PADDING_TOP: f32 = V1_CODE_CONTENT_PADDING_TOP_PX as f32;
pub const V1_CODE_CONTENT_PADDING_X_PX: f32 = 16.0;
pub const V1_CODE_CONTENT_PADDING_BOTTOM: f32 = V1_CODE_CONTENT_PADDING_BOTTOM_PX as f32;
pub const V1_CODE_SCROLL_END_SPACER: f32 = V1_CODE_SCROLL_END_SPACER_PX as f32;
pub const V1_CODE_TEXT_OFFSET_TOP_PX: f32 = (V1_CODE_FRAME_BORDER_WIDTH_PX
    + V1_CODE_TOOLBAR_SURFACE_HEIGHT_PX
    + V1_CODE_SURFACE_GAP_PX
    + V1_CODE_CONTENT_PADDING_TOP_PX) as f32;
pub const V1_CODE_TEXT_OFFSET_X_PX: f32 =
    (V1_CODE_FRAME_BORDER_WIDTH_PX + V1_CODE_CONTENT_PADDING_X_PX as f64) as f32;

#[expect(clippy::too_many_arguments, reason = "P4-002 render context 聚合")]
pub fn render_code_block(
    block_id: BlockId,
    content: AnyElement,
    theme: GuiTheme,
    language: Option<&str>,
    language_edit: Option<&CodeLanguageEditState>,
    code_theme_menu_open: bool,
    code_highlight_theme: &'static str,
    action_active: bool,
    estimated_content_height_px: f32,
    scroll_handle: Option<ScrollHandle>,
    view: Entity<CditorV2View>,
    code_language_focus: FocusHandle,
) -> AnyElement {
    let code_theme = code_theme_item(code_highlight_theme);
    let viewport_height_px =
        estimated_content_height_px.min(STRUCTURED_BLOCK_CONTENT_VIEWPORT_MAX_HEIGHT_PX as f32);
    let vertical_overflow = estimated_content_height_px > viewport_height_px + 0.5;
    let scroll_content_height_px =
        code_scroll_content_height_px(estimated_content_height_px, vertical_overflow);
    let mut code_content = div()
        .id(("code-scroll-container", block_id))
        .flex_1()
        .min_w(px(0.0))
        .h(px(viewport_height_px))
        .bg(rgb(if action_active {
            blend_rgb(code_theme.background, theme.focused, 0.12)
        } else {
            code_theme.background
        }))
        .overflow_scroll()
        .child(render_code_content(
            content,
            code_theme.foreground,
            vertical_overflow,
        ));
    if let Some(scroll_handle) = &scroll_handle {
        code_content = code_content.track_scroll(scroll_handle);
    }
    let scrollbar = vertical_overflow
        .then_some(scroll_handle.as_ref())
        .flatten()
        .map(|scroll_handle| {
            render_internal_vertical_scrollbar(
                ("code-vertical-scrollbar", block_id).into(),
                scroll_handle,
                viewport_height_px,
                scroll_content_height_px,
                theme,
                InternalScrollbarTrackStyle {
                    background: theme.code_toolbar_background,
                    separator: theme.code_toolbar_border,
                },
            )
        });
    let code_viewport = div()
        .relative()
        .w_full()
        .h(px(viewport_height_px))
        .flex()
        .bg(rgb(code_theme.background))
        .when(vertical_overflow, |viewport| {
            viewport.on_scroll_wheel(|_event, _window, cx| cx.stop_propagation())
        })
        .child(code_content)
        .children(scrollbar);

    div()
        .relative()
        .w_full()
        .min_h(px(V1_CODE_BLOCK_MIN_HEIGHT_PX))
        .rounded(px(V1_CODE_BLOCK_RADIUS_PX))
        .border(px(V1_CODE_FRAME_BORDER_WIDTH_PX as f32))
        .border_color(rgb(theme.code_toolbar_border))
        .bg(rgb(theme.code_toolbar_border))
        .overflow_hidden()
        .flex()
        .flex_col()
        .gap(px(V1_CODE_SURFACE_GAP_PX as f32))
        .font_family(EDITOR_MONO_FONT_FAMILY)
        .child(
            div()
                .relative()
                .w_full()
                .h(px(V1_CODE_TOOLBAR_SURFACE_HEIGHT_PX as f32))
                .flex_none()
                .bg(rgb(theme.code_toolbar_background))
                .child(render_code_toolbar(
                    block_id,
                    theme,
                    language,
                    language_edit,
                    code_theme_menu_open,
                    code_highlight_theme,
                    view,
                    code_language_focus,
                )),
        )
        .child(code_viewport)
        .into_any_element()
}

pub(crate) fn estimated_code_content_height_px(text: &str) -> f32 {
    let line_count = text.split('\n').count().max(1) as f32;
    V1_CODE_CONTENT_PADDING_TOP
        + line_count * cditor_core::layout::V1_CODE_TEXT_LINE_HEIGHT_PX as f32
        + V1_CODE_CONTENT_PADDING_BOTTOM
}

pub(crate) fn code_scroll_content_height_px(
    content_height_px: f32,
    vertical_overflow: bool,
) -> f32 {
    content_height_px
        + if vertical_overflow {
            V1_CODE_SCROLL_END_SPACER
        } else {
            0.0
        }
}

fn blend_rgb(background: u32, accent: u32, accent_alpha: f32) -> u32 {
    let alpha = accent_alpha.clamp(0.0, 1.0);
    let channel = |shift: u32| {
        let background = ((background >> shift) & 0xff_u32) as f32;
        let accent = ((accent >> shift) & 0xff_u32) as f32;
        (background * (1.0 - alpha) + accent * alpha).round() as u32
    };
    (channel(16) << 16) | (channel(8) << 8) | channel(0)
}

fn render_code_content(
    content: AnyElement,
    text_color: u32,
    include_scroll_end_spacer: bool,
) -> AnyElement {
    div()
        .w_full()
        .pt(px(V1_CODE_CONTENT_PADDING_TOP))
        .px(px(V1_CODE_CONTENT_PADDING_X_PX))
        .pb(px(V1_CODE_CONTENT_PADDING_BOTTOM))
        .text_color(rgb(text_color))
        .child(content)
        .when(include_scroll_end_spacer, |content| {
            content.child(div().w_full().h(px(V1_CODE_SCROLL_END_SPACER)).flex_none())
        })
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_block_geometry_matches_the_visual_prototype() {
        assert_eq!(V1_CODE_BLOCK_MIN_HEIGHT_PX, 93.0);
        assert_eq!(V1_CODE_BLOCK_RADIUS_PX, 6.0);
        assert_eq!(V1_CODE_CONTENT_PADDING_TOP, 14.0);
        assert_eq!(V1_CODE_CONTENT_PADDING_X_PX, 16.0);
        assert_eq!(V1_CODE_CONTENT_PADDING_BOTTOM, 16.0);
        assert_eq!(V1_CODE_SCROLL_END_SPACER, 24.0);
        assert_eq!(V1_CODE_TEXT_OFFSET_TOP_PX, 52.0);
        assert_eq!(V1_CODE_TEXT_OFFSET_X_PX, 17.0);
    }

    #[test]
    fn action_tint_preserves_most_of_the_code_theme_background() {
        assert_eq!(blend_rgb(0x000000, 0xffffff, 0.0), 0x000000);
        assert_eq!(blend_rgb(0x000000, 0xffffff, 1.0), 0xffffff);
        assert_eq!(blend_rgb(0x282a36, 0x2383e2, 0.12), 0x27354b);
    }

    #[test]
    fn code_content_height_includes_every_source_line_and_body_padding() {
        assert_eq!(estimated_code_content_height_px(""), 54.0);
        assert_eq!(estimated_code_content_height_px("a\nb\nc"), 102.0);
    }

    #[test]
    fn code_scroll_end_spacer_only_extends_overflowing_content() {
        assert_eq!(code_scroll_content_height_px(280.0, false), 280.0);
        assert_eq!(code_scroll_content_height_px(340.0, true), 364.0);
    }
}
