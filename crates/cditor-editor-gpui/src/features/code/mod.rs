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
pub(crate) use toolbar::language_is_mermaid;
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
    // 折叠意图（不是"稳定折叠态"）：动画途中 chevron 也要立刻反映用户刚点的方向。
    collapse_intent: bool,
    copy_feedback: bool,
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
        // gap 与 header 的底边框（header_draws_bottom_border）做 1px 互换：
        // 动画期间 header 无底边框（37px）+ gap(1px) == 收起态 header 有底边框（38px）。
        // 而 V1_CODE_COLLAPSED_BLOCK_HEIGHT_PX=46 已经按"有底边框"算好——所以 gap 必须
        // 在内容面挂载（含动画中）时都在，否则动画期间少这 1px，落定时凭空多 1px 弹一下。
        // 必须跟 content_surface_mounted 而不是 is_stably_expanded：后者在补间结束前为
        // false，会把 gap 压到落定帧才出现，制造一跳。
        .when(content_surface_mounted, |this| {
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
                            collapse_intent,
                            copy_feedback,
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

    /// 收起高度必须同时容得下两种 header 画法，否则落定那一帧会跳。
    ///
    /// 展开/动画中：header 不画底边框（上边框 + 工具栏），下面接一个 `gap`。
    /// 稳定收起：header 自己画底边框，没有 gap。
    /// `V1_CODE_COLLAPSED_BLOCK_HEIGHT_PX` 是按后者算的，而 `animated_content_height`
    /// 用它做减数——两种画法的高度相等，这个减法才成立。相等的前提就是
    /// `gap == 边框宽`。谁改了其中一个而没改另一个，这条会先炸，而不是等到肉眼
    /// 发现"动画结束又弹了 1px"。
    #[test]
    fn collapsed_height_is_identical_for_both_header_paintings() {
        let open_header = V1_CODE_FRAME_BORDER_WIDTH_PX + V1_CODE_TOOLBAR_SURFACE_HEIGHT_PX;
        let animated = BLOCK_SHELL_PADDING_Y_PX * 2.0 + open_header + V1_CODE_SURFACE_GAP_PX;
        let stably_collapsed = BLOCK_SHELL_PADDING_Y_PX * 2.0
            + V1_CODE_FRAME_BORDER_WIDTH_PX * 2.0
            + V1_CODE_TOOLBAR_SURFACE_HEIGHT_PX;

        assert_eq!(
            V1_CODE_SURFACE_GAP_PX, V1_CODE_FRAME_BORDER_WIDTH_PX,
            "gap 与底边框必须等宽，两种 header 画法才能互换"
        );
        assert_eq!(animated, stably_collapsed);
        assert_eq!(animated, V1_CODE_COLLAPSED_BLOCK_HEIGHT_PX);
    }

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
