use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, App, InteractiveElement, IntoElement, MouseButton, MouseMoveEvent, ParentElement,
    Styled, div, px, rgb,
};

use crate::block::chrome::{
    BLOCK_ROW_GAP_PX, BLOCK_SHELL_BORDER_WIDTH_PX, BLOCK_SHELL_OUTER_PADDING_X_PX,
    BlockChromeStyle,
};
use crate::block::gutter::{GutterAddHandler, GutterMouseDownHandler, render_block_gutter};
use crate::block::prefix::{
    FoldToggleHandler, TodoToggleHandler, render_block_content_prefix, render_block_prefix,
};
use crate::diagnostics::block_color::trace_render;
use crate::theme::GuiTheme;
use cditor_core::rich_text::RichBlockKind;
use cditor_runtime::ViewBlockSnapshot;

const NOTION_QUOTE_BAR_WIDTH_PX: f32 = 3.0;
const BLOCK_SELECTION_EXPAND_X_PX: f32 = 2.0;
const BLOCK_SELECTION_EXPAND_Y_PX: f32 = 3.0;
const BLOCK_SELECTION_RADIUS_PX: f32 = 4.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BlockActionState {
    pub action_active: bool,
    pub action_root: bool,
    pub dragging: bool,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlockShellStyle {
    pub indent_px: f32,
    pub border_color: u32,
    pub background_color: u32,
}

#[cfg(test)]
impl BlockShellStyle {
    pub fn from_snapshot(block: &ViewBlockSnapshot, theme: GuiTheme) -> Self {
        let chrome = BlockChromeStyle::from_snapshot(block, theme);
        Self {
            indent_px: chrome.indent_px,
            border_color: chrome.content_border,
            background_color: chrome.outer_background,
        }
    }
}

pub type BlockMouseMoveHandler =
    Box<dyn Fn(&MouseMoveEvent, &mut gpui::Window, &mut App) + 'static>;

#[expect(clippy::too_many_arguments, reason = "P4-002 render context 聚合")]
pub fn block_shell(
    block: &ViewBlockSnapshot,
    theme: GuiTheme,
    content: AnyElement,
    hovered: bool,
    action: BlockActionState,
    collapsed_code_block: bool,
    on_mouse_down: impl Fn(&gpui::MouseDownEvent, &mut gpui::Window, &mut App) + 'static,
    on_mouse_move: Option<BlockMouseMoveHandler>,
    on_gutter_add: Option<GutterAddHandler>,
    on_gutter_mouse_down: Option<GutterMouseDownHandler>,
    on_todo_toggle: Option<TodoToggleHandler>,
    on_fold_toggle: Option<FoldToggleHandler>,
) -> AnyElement {
    let chrome = BlockChromeStyle::from_snapshot(block, theme);
    let horizontal = chrome.horizontal_geometry();
    let gutter_visible = should_show_gutter(hovered, action.action_root);
    let outer_background = outer_background_for_action(chrome.outer_background, theme, action);
    let content_background = content_background_for_action(
        chrome.content_background,
        theme,
        action,
        block.attrs.background_color.is_some(),
    );
    let shell_border = border_for_action(theme.surface, theme, action);
    let content_border = border_for_action(chrome.content_border, theme, action);
    let content_min_height_px =
        content_min_height_px(collapsed_code_block, chrome.content_min_height_px);
    trace_render(
        block.block_id,
        &block.attrs,
        chrome.text_color,
        content_background,
        action.action_active,
    );
    div()
        .id(("v2-block", block.block_id))
        .w_full()
        .border(px(BLOCK_SHELL_BORDER_WIDTH_PX))
        .border_color(rgb(shell_border))
        .bg(rgb(outer_background))
        .text_color(rgb(chrome.text_color))
        .px(px(BLOCK_SHELL_OUTER_PADDING_X_PX))
        .pt(px(chrome.outer_padding_top_px))
        .pb(px(chrome.outer_padding_bottom_px))
        .on_mouse_down(MouseButton::Left, on_mouse_down)
        .when_some(on_mouse_move, |this, handler| this.on_mouse_move(handler))
        .child(
            div().pl(px(horizontal.indent_px)).child(
                div()
                    .id(("v2-block-row", block.block_id))
                    .w_full()
                    .flex()
                    .items_start()
                    .gap(px(BLOCK_ROW_GAP_PX))
                    .child(render_block_gutter(
                        theme,
                        gutter_visible,
                        action.action_active,
                        gutter_control_top_px(&block.kind, chrome.content_min_height_px),
                        on_gutter_add,
                        on_gutter_mouse_down,
                    ))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .w_full()
                            .flex()
                            .items_start()
                            .child(render_block_prefix(
                                &block.chrome.prefix,
                                chrome.marker_lane_width_px,
                                theme,
                                true,
                                on_fold_toggle,
                                block.focused,
                                chrome.content_min_height_px,
                                hovered,
                                heading_level(&block.kind),
                            ))
                            .child(
                                div()
                                    .relative()
                                    .min_w(px(0.0))
                                    .w_full()
                                    .when_some(content_min_height_px, |this, height| {
                                        this.min_h(px(height))
                                    })
                                    .rounded(px(chrome.content_radius_px))
                                    .bg(rgb(content_background))
                                    .when(chrome.content_border_width_px > 0.0, |this| {
                                        this.border(px(chrome.content_border_width_px))
                                    })
                                    .border_color(rgb(content_border))
                                    // Keep the historical 4px quote geometry slot so caret/hit-test
                                    // origins stay stable, while drawing the visible Notion bar at 3px.
                                    .border_l(px(chrome.content_border_left_px()))
                                    .pl(px(chrome.content_padding_left_px))
                                    .pr(px(chrome.content_padding_right_px))
                                    .py(px(chrome.content_padding_y_px))
                                    .flex()
                                    .items_start()
                                    .when(action.action_active, |this| {
                                        this.child(render_block_selection_decoration(theme))
                                    })
                                    .when_some(chrome.quote_bar, |this, color| {
                                        this.child(render_quote_bar(color))
                                    })
                                    .when_some(
                                        render_block_content_prefix(
                                            &block.chrome.prefix,
                                            theme,
                                            true,
                                            on_todo_toggle,
                                        ),
                                        |this, prefix| this.child(prefix),
                                    )
                                    .child(div().min_w(px(0.0)).w_full().child(content)),
                            ),
                    ),
            ),
        )
        .into_any_element()
}

fn gutter_control_top_px(kind: &RichBlockKind, first_line_height_px: f32) -> f32 {
    if matches!(kind, RichBlockKind::Heading { .. }) {
        ((first_line_height_px - 28.0) / 2.0).max(0.0)
    } else {
        0.0
    }
}

fn heading_level(kind: &RichBlockKind) -> Option<u8> {
    match kind {
        RichBlockKind::Heading { level } => Some(*level),
        _ => None,
    }
}

fn render_block_selection_decoration(theme: GuiTheme) -> AnyElement {
    div()
        .absolute()
        .left(px(-BLOCK_SELECTION_EXPAND_X_PX))
        .right(px(-BLOCK_SELECTION_EXPAND_X_PX))
        .top(px(-BLOCK_SELECTION_EXPAND_Y_PX))
        .bottom(px(-BLOCK_SELECTION_EXPAND_Y_PX))
        .rounded(px(BLOCK_SELECTION_RADIUS_PX))
        .bg(rgb(theme.action_background))
        .into_any_element()
}

pub fn should_show_gutter(hovered: bool, action_root: bool) -> bool {
    action_root || hovered
}

pub fn content_background_for_action(
    default_content_background: u32,
    theme: GuiTheme,
    action: BlockActionState,
    has_custom_background: bool,
) -> u32 {
    if action.action_active && !has_custom_background {
        theme.action_background
    } else {
        default_content_background
    }
}

pub fn outer_background_for_action(
    default_outer_background: u32,
    _theme: GuiTheme,
    _action: BlockActionState,
) -> u32 {
    // Outer shell (which includes gutter) never changes background on selection.
    // Only the content container shows the action/selection color.
    default_outer_background
}

pub fn border_for_action(default_border: u32, theme: GuiTheme, action: BlockActionState) -> u32 {
    if action.dragging {
        // Only show distinct border when actively dragging
        theme.action_background
    } else {
        default_border
    }
}

fn render_quote_bar(color: u32) -> AnyElement {
    div()
        .absolute()
        .left(px(0.0))
        .top_0()
        .bottom_0()
        .w(px(NOTION_QUOTE_BAR_WIDTH_PX))
        .bg(rgb(color))
        .into_any_element()
}

fn content_min_height_px(collapsed_code_block: bool, chrome_min_height_px: f32) -> Option<f32> {
    if collapsed_code_block {
        None
    } else {
        Some(chrome_min_height_px)
    }
}

#[cfg(test)]
mod tests {
    use cditor_runtime::DocumentRuntime;

    use super::*;

    #[test]
    fn gutter_visibility_prefers_action_root_or_hover_only() {
        assert!(!should_show_gutter(false, false));
        assert!(should_show_gutter(true, false));
        assert!(should_show_gutter(false, true));
    }

    #[test]
    fn notion_quote_bar_is_three_pixels_without_changing_hit_test_slot() {
        assert_eq!(NOTION_QUOTE_BAR_WIDTH_PX, 3.0);
    }

    #[test]
    fn selection_decoration_is_symmetric_and_does_not_change_layout() {
        assert_eq!(BLOCK_SELECTION_EXPAND_X_PX, 2.0);
        assert_eq!(BLOCK_SELECTION_EXPAND_Y_PX, 3.0);
        assert_eq!(BLOCK_SELECTION_RADIUS_PX, 4.0);
    }

    #[test]
    fn heading_gutter_centers_on_the_first_line_without_moving_other_blocks() {
        assert_eq!(
            gutter_control_top_px(&RichBlockKind::Heading { level: 1 }, 39.0),
            5.5
        );
        assert_eq!(
            gutter_control_top_px(&RichBlockKind::Heading { level: 2 }, 32.0),
            2.0
        );
        assert_eq!(gutter_control_top_px(&RichBlockKind::Paragraph, 24.0), 0.0);
        assert_eq!(
            gutter_control_top_px(&RichBlockKind::Code { language: None }, 93.0),
            0.0
        );
    }

    #[test]
    fn heading_level_is_only_extracted_from_heading_kinds() {
        assert_eq!(heading_level(&RichBlockKind::Heading { level: 1 }), Some(1));
        assert_eq!(heading_level(&RichBlockKind::Heading { level: 2 }), Some(2));
        assert_eq!(heading_level(&RichBlockKind::Heading { level: 3 }), Some(3));
        assert_eq!(heading_level(&RichBlockKind::Paragraph), None);
        assert_eq!(heading_level(&RichBlockKind::Code { language: None }), None);
    }

    #[test]
    fn collapsed_code_block_releases_the_shell_min_height() {
        assert_eq!(content_min_height_px(true, 93.0), None);
        assert_eq!(content_min_height_px(false, 93.0), Some(93.0));
        assert_eq!(
            content_min_height_px(
                false,
                cditor_core::layout::V1_CODE_INNER_MIN_HEIGHT_PX as f32
            ),
            Some(93.0)
        );
    }

    #[test]
    fn action_active_uses_v1_action_background_without_height_change() {
        let theme = GuiTheme::light();
        let action = BlockActionState {
            action_active: true,
            action_root: false,
            dragging: true,
        };
        // Content background changes on action_active
        assert_eq!(
            content_background_for_action(0x123456, theme, action, false),
            theme.action_background
        );
        // Once a block has an explicit background, keep it visible while its
        // gutter menu is active instead of immediately painting over it with
        // the generic action tint.
        assert_eq!(
            content_background_for_action(0x123456, theme, action, true),
            0x123456
        );
        // Outer background does NOT change on action_active (gutter stays uncolored)
        assert_eq!(
            outer_background_for_action(0xabcdef, theme, action),
            0xabcdef
        );
        // Border only changes when dragging
        assert_eq!(
            border_for_action(theme.surface, theme, action),
            theme.action_background
        );
        assert_eq!(
            content_background_for_action(0x123456, theme, BlockActionState::default(), false,),
            0x123456
        );
        assert_eq!(
            outer_background_for_action(0xabcdef, theme, BlockActionState::default()),
            0xabcdef
        );
        assert_eq!(
            border_for_action(theme.surface, theme, BlockActionState::default()),
            theme.surface
        );
    }

    #[test]
    fn block_shell_style_uses_chrome_depth_focus_and_selection() {
        let runtime = DocumentRuntime::demo();
        let projection = runtime.projection_for_window();
        let mut block = projection.blocks[0].clone();
        block.depth = 2;
        block.chrome.list_info.depth = 2;
        block.focused = true;
        block.selected = false;

        let style = BlockShellStyle::from_snapshot(&block, GuiTheme::light());

        assert_eq!(style.indent_px, 48.0);
        assert_eq!(style.background_color, GuiTheme::light().surface);
    }
}
