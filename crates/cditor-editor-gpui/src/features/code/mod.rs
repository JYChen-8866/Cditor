use crate::editor_view::CditorV2View;
use crate::input::CodeLanguageEditState;
use crate::platform::EDITOR_MONO_FONT_FAMILY;
use crate::theme::GuiTheme;
use cditor_core::ids::BlockId;
use cditor_core::layout::{
    BLOCK_SHELL_PADDING_Y_PX, V1_CODE_CONTENT_PADDING_BOTTOM_PX, V1_CODE_CONTENT_PADDING_TOP_PX,
    V1_CODE_FRAME_BORDER_WIDTH_PX, V1_CODE_INNER_MIN_HEIGHT_PX, V1_CODE_SURFACE_GAP_PX,
    V1_CODE_TOOLBAR_SURFACE_HEIGHT_PX,
};
use gpui::prelude::FluentBuilder;
use gpui::{AnyElement, Entity, FocusHandle, IntoElement, ParentElement, Styled, div, px, rgb};

mod actions;
pub(crate) mod highlight;
mod language_actions;
mod toolbar;

use highlight::code_theme_item;
use toolbar::render_code_toolbar;

pub const V1_CODE_BLOCK_MIN_HEIGHT_PX: f32 = V1_CODE_INNER_MIN_HEIGHT_PX as f32;
pub const V1_CODE_COLLAPSED_BLOCK_HEIGHT_PX: f64 = BLOCK_SHELL_PADDING_Y_PX * 2.0
    + V1_CODE_TOOLBAR_SURFACE_HEIGHT_PX
    + V1_CODE_FRAME_BORDER_WIDTH_PX * 2.0;
pub const V1_CODE_BLOCK_RADIUS_PX: f32 = 6.0;
pub const V1_CODE_CONTENT_PADDING_TOP: f32 = V1_CODE_CONTENT_PADDING_TOP_PX as f32;
pub const V1_CODE_CONTENT_PADDING_X_PX: f32 = 16.0;
pub const V1_CODE_CONTENT_PADDING_BOTTOM: f32 = V1_CODE_CONTENT_PADDING_BOTTOM_PX as f32;
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
    view: Entity<CditorV2View>,
    code_language_focus: FocusHandle,
    collapsed: bool,
) -> AnyElement {
    let code_theme = code_theme_item(code_highlight_theme);
    let code_viewport = if collapsed {
        None
    } else {
        let content_background = if action_active {
            blend_rgb(code_theme.background, theme.focused, 0.12)
        } else {
            code_theme.background
        };
        Some(
            div()
                .relative()
                .w_full()
                .rounded_b(px(V1_CODE_BLOCK_RADIUS_PX))
                .border_l(px(V1_CODE_FRAME_BORDER_WIDTH_PX as f32))
                .border_r(px(V1_CODE_FRAME_BORDER_WIDTH_PX as f32))
                .border_b(px(V1_CODE_FRAME_BORDER_WIDTH_PX as f32))
                .border_color(rgb(theme.code_toolbar_border))
                .bg(rgb(content_background))
                .child(
                    div()
                        .w_full()
                        .bg(rgb(content_background))
                        .child(render_code_content(content, code_theme.foreground)),
                )
                .into_any_element(),
        )
    };

    div()
        .relative()
        .w_full()
        .when(!collapsed, |this| {
            this.min_h(px(V1_CODE_BLOCK_MIN_HEIGHT_PX))
        })
        .rounded(px(V1_CODE_BLOCK_RADIUS_PX))
        .flex()
        .flex_col()
        .when(!collapsed, |this| {
            this.gap(px(V1_CODE_SURFACE_GAP_PX as f32))
        })
        .font_family(EDITOR_MONO_FONT_FAMILY)
        .child(
            div()
                .relative()
                .w_full()
                .rounded(px(V1_CODE_BLOCK_RADIUS_PX))
                .when(!collapsed, |this| this.rounded_b_none())
                .border_t(px(V1_CODE_FRAME_BORDER_WIDTH_PX as f32))
                .border_l(px(V1_CODE_FRAME_BORDER_WIDTH_PX as f32))
                .border_r(px(V1_CODE_FRAME_BORDER_WIDTH_PX as f32))
                .when(collapsed, |this| {
                    this.border_b(px(V1_CODE_FRAME_BORDER_WIDTH_PX as f32))
                })
                .border_color(rgb(theme.code_toolbar_border))
                .bg(rgb(theme.code_toolbar_background))
                .flex_none()
                .child(
                    div()
                        .relative()
                        .w_full()
                        .h(px(V1_CODE_TOOLBAR_SURFACE_HEIGHT_PX as f32))
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
                ),
        )
        .when_some(code_viewport, |this, viewport| this.child(viewport))
        .into_any_element()
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

fn render_code_content(content: AnyElement, text_color: u32) -> AnyElement {
    div()
        .w_full()
        .pt(px(V1_CODE_CONTENT_PADDING_TOP))
        .px(px(V1_CODE_CONTENT_PADDING_X_PX))
        .pb(px(V1_CODE_CONTENT_PADDING_BOTTOM))
        .text_color(rgb(text_color))
        .child(content)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_block_geometry_matches_the_visual_prototype() {
        assert_eq!(V1_CODE_BLOCK_MIN_HEIGHT_PX, 93.0);
        assert_eq!(V1_CODE_COLLAPSED_BLOCK_HEIGHT_PX, 46.0);
        assert_eq!(V1_CODE_BLOCK_RADIUS_PX, 6.0);
        assert_eq!(V1_CODE_CONTENT_PADDING_TOP, 14.0);
        assert_eq!(V1_CODE_CONTENT_PADDING_X_PX, 16.0);
        assert_eq!(V1_CODE_CONTENT_PADDING_BOTTOM, 16.0);
        assert_eq!(V1_CODE_TEXT_OFFSET_TOP_PX, 52.0);
        assert_eq!(V1_CODE_TEXT_OFFSET_X_PX, 17.0);
    }

    #[test]
    fn action_tint_preserves_most_of_the_code_theme_background() {
        assert_eq!(blend_rgb(0x000000, 0xffffff, 0.0), 0x000000);
        assert_eq!(blend_rgb(0x000000, 0xffffff, 1.0), 0xffffff);
        assert_eq!(blend_rgb(0x282a36, 0x2383e2, 0.12), 0x27354b);
    }
}
