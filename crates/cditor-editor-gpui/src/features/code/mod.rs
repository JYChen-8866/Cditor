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
mod collapse_motion;
pub(crate) mod highlight;
mod language_actions;
mod toolbar;

pub(crate) use collapse_motion::{CodeCollapseTween, HeightTween};
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
    _action_active: bool,
    view: Entity<CditorV2View>,
    code_language_focus: FocusHandle,
    collapsed: bool,
    // When Some, we are mid-animation for collapse/expand. The value is the
    // desired pixel height for the code content surface (below the toolbar header).
    // The viewport will be emitted (even if logical collapsed) and constrained to
    // this height with overflow hidden, so the visual area shrinks/grows while
    // the layout engine is fed the corresponding total block height.
    animated_content_height: Option<f64>,
) -> AnyElement {
    let code_theme = code_theme_item(code_highlight_theme);

    // Stably collapsed (no active animation): remove the content subtree entirely.
    // While a tween is active we must keep (and clip) the content surface so there is
    // something whose height can visually change while we feed the layout engine the
    // shrinking/expanding total block height.
    let has_animated_content = animated_content_height.map_or(false, |h| h > 0.5);
    let stably_collapsed = collapsed && !has_animated_content;

    // The content surface (the scrollable code area below the toolbar header) is
    // mounted whenever we are not in the final closed visual state.
    let content_surface_mounted = !stably_collapsed;

    let code_viewport = if !content_surface_mounted {
        None
    } else {
        let content_background = code_theme.background;
        let mut vp = div()
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
            );
        if let Some(h) = animated_content_height {
            vp = vp.h(px(h.max(0.0) as f32)).overflow_hidden();
        }
        Some(vp.into_any_element())
    };

    // Only the stable fully-expanded (non-animating, not collapsed) state uses the
    // tall geometry constraints. During animation the tween drives the size.
    let is_stably_expanded = !collapsed && !has_animated_content;

    // Header connection:
    // - While a content surface is mounted (stable expanded or mid-animation with >0 content area),
    //   the header must look "open": no bottom border, bottom corners not rounded.
    // - Only when the final visual is just the header (stably collapsed), the header
    //   draws its own bottom border and has full rounding.
    let header_draws_bottom_border = !content_surface_mounted;
    let header_open_bottom = content_surface_mounted;

    div()
        .relative()
        .w_full()
        .when(is_stably_expanded, |this| {
            this.min_h(px(V1_CODE_BLOCK_MIN_HEIGHT_PX))
        })
        .rounded(px(V1_CODE_BLOCK_RADIUS_PX))
        .flex()
        .flex_col()
        .when(is_stably_expanded, |this| {
            this.gap(px(V1_CODE_SURFACE_GAP_PX as f32))
        })
        .font_family(EDITOR_MONO_FONT_FAMILY)
        .child(
            div()
                .relative()
                .w_full()
                .rounded(px(V1_CODE_BLOCK_RADIUS_PX))
                .when(header_open_bottom, |this| this.rounded_b_none())
                .border_t(px(V1_CODE_FRAME_BORDER_WIDTH_PX as f32))
                .border_l(px(V1_CODE_FRAME_BORDER_WIDTH_PX as f32))
                .border_r(px(V1_CODE_FRAME_BORDER_WIDTH_PX as f32))
                .when(header_draws_bottom_border, |this| {
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
}
