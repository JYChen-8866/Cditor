use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, App, InteractiveElement, IntoElement, MouseButton, MouseDownEvent, ParentElement,
    PathBuilder, Styled, Window, canvas, div, point, px, rgb,
};

use crate::gui::GuiTheme;
use crate::gui::block::chrome::{BLOCK_PREFIX_WIDTH_PX, CALLOUT_PREFIX_WIDTH_PX};
use cditor_core::block::{BlockPrefixSnapshot, bullet_marker_for_depth};
use cditor_core::rich_text::CalloutVariant;

pub type TodoToggleHandler = Box<dyn Fn(&MouseDownEvent, &mut Window, &mut App) + 'static>;
pub type FoldToggleHandler = Box<dyn Fn(&MouseDownEvent, &mut Window, &mut App) + 'static>;

const NOTION_PREFIX_LINE_HEIGHT_PX: f32 = 24.0;
const NOTION_CHECKBOX_SIZE_PX: f32 = 16.0;
const NOTION_CHECKBOX_RADIUS_PX: f32 = 2.0;
const NOTION_FOLD_HOVER_SIZE_PX: f32 = 20.0;
const NOTION_FOLD_ICON_SIZE_PX: f32 = 10.0;
const NOTION_FOLD_HOVER_RADIUS_PX: f32 = 3.0;

pub fn render_block_prefix(
    prefix: &BlockPrefixSnapshot,
    marker_lane_width_px: f32,
    theme: GuiTheme,
    editable: bool,
    on_fold_toggle: Option<FoldToggleHandler>,
    focused: bool,
    block_line_height_px: f32,
) -> AnyElement {
    match prefix {
        BlockPrefixSnapshot::None => div()
            .w(px(marker_lane_width_px))
            .flex_shrink_0()
            .into_any_element(),
        BlockPrefixSnapshot::Bullet { depth } => div()
            .w(px(BLOCK_PREFIX_WIDTH_PX))
            .h(px(NOTION_PREFIX_LINE_HEIGHT_PX))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .text_color(rgb(theme.text))
            .child(bullet_marker_for_depth(*depth))
            .into_any_element(),
        BlockPrefixSnapshot::Number { ordinal } => div()
            .w(px(BLOCK_PREFIX_WIDTH_PX))
            .h(px(NOTION_PREFIX_LINE_HEIGHT_PX))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_end()
            .pr(px(4.0))
            .text_color(rgb(theme.text))
            .child(format!("{ordinal}."))
            .into_any_element(),
        // A todo checkbox is content, not gutter chrome. Keep the shared
        // marker lane empty and render the checkbox at the block surface start.
        BlockPrefixSnapshot::Todo { .. } => div()
            .w(px(marker_lane_width_px))
            .flex_shrink_0()
            .into_any_element(),
        BlockPrefixSnapshot::Callout { .. } => div()
            .w(px(marker_lane_width_px))
            .flex_shrink_0()
            .into_any_element(),
        BlockPrefixSnapshot::Heading { collapsed } | BlockPrefixSnapshot::Toggle { collapsed } => {
            let control_visible = fold_control_visible(prefix, focused);
            let on_fold_toggle = if control_visible {
                on_fold_toggle
            } else {
                None
            };
            div()
                .w(px(BLOCK_PREFIX_WIDTH_PX))
                .h(px(fold_prefix_line_height_px(prefix, block_line_height_px)))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .text_color(rgb(theme.text))
                .when(editable && control_visible, |this| this.cursor_pointer())
                .when_some(on_fold_toggle, |this, handler| {
                    this.on_mouse_down(MouseButton::Left, handler)
                })
                .child(render_fold_indicator(*collapsed, control_visible, theme))
                .into_any_element()
        }
    }
}

pub fn render_block_content_prefix(
    prefix: &BlockPrefixSnapshot,
    theme: GuiTheme,
    editable: bool,
    on_todo_toggle: Option<TodoToggleHandler>,
) -> Option<AnyElement> {
    match prefix {
        BlockPrefixSnapshot::Todo { checked } => Some(
            div()
                .w(px(BLOCK_PREFIX_WIDTH_PX))
                .h(px(NOTION_PREFIX_LINE_HEIGHT_PX))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_start()
                .when(editable, |this| this.cursor_pointer())
                .when_some(on_todo_toggle, |this, handler| {
                    this.on_mouse_down(MouseButton::Left, handler)
                })
                .child(render_task_checkbox(*checked, theme))
                .into_any_element(),
        ),
        BlockPrefixSnapshot::Callout { variant } => {
            Some(render_callout_content_prefix(*variant, theme))
        }
        _ => None,
    }
}

pub fn render_callout_content_prefix(variant: CalloutVariant, theme: GuiTheme) -> AnyElement {
    div()
        .w(px(CALLOUT_PREFIX_WIDTH_PX))
        .flex_shrink_0()
        .flex()
        .items_start()
        .justify_start()
        .child(
            div()
                .size(px(24.0))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(18.0))
                .text_color(rgb(theme.text))
                .child(callout_icon(variant)),
        )
        .into_any_element()
}

pub fn fold_control_visible(prefix: &BlockPrefixSnapshot, focused: bool) -> bool {
    match prefix {
        BlockPrefixSnapshot::Heading { collapsed } | BlockPrefixSnapshot::Toggle { collapsed } => {
            *collapsed || focused
        }
        _ => false,
    }
}

fn fold_prefix_line_height_px(prefix: &BlockPrefixSnapshot, block_line_height_px: f32) -> f32 {
    if matches!(prefix, BlockPrefixSnapshot::Heading { .. }) {
        block_line_height_px.max(NOTION_PREFIX_LINE_HEIGHT_PX)
    } else {
        NOTION_PREFIX_LINE_HEIGHT_PX
    }
}

fn render_fold_indicator(collapsed: bool, visible: bool, theme: GuiTheme) -> AnyElement {
    let points = fold_indicator_points(collapsed);
    div()
        .size(px(NOTION_FOLD_HOVER_SIZE_PX))
        .rounded(px(NOTION_FOLD_HOVER_RADIUS_PX))
        .flex()
        .items_center()
        .justify_center()
        .when(!visible, |this| this.invisible())
        .hover(move |style| style.bg(rgb(theme.hover_surface)))
        .child(
            canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    let origin = bounds.origin;
                    let mut path = PathBuilder::fill();
                    path.move_to(point(
                        origin.x + px(points[0].0),
                        origin.y + px(points[0].1),
                    ));
                    path.line_to(point(
                        origin.x + px(points[1].0),
                        origin.y + px(points[1].1),
                    ));
                    path.line_to(point(
                        origin.x + px(points[2].0),
                        origin.y + px(points[2].1),
                    ));
                    path.close();
                    if let Ok(path) = path.build() {
                        window.paint_path(path, rgb(theme.text));
                    }
                },
            )
            .size(px(NOTION_FOLD_ICON_SIZE_PX)),
        )
        .into_any_element()
}

const fn fold_indicator_points(collapsed: bool) -> [(f32, f32); 3] {
    if collapsed {
        // Compact right-pointing solid triangle, matching Notion's closed toggle.
        [(2.0, 1.5), (8.0, 5.0), (2.0, 8.5)]
    } else {
        // The open state uses the same optical weight, rotated downward.
        [(1.5, 2.0), (8.5, 2.0), (5.0, 8.0)]
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
        .size(px(NOTION_CHECKBOX_SIZE_PX))
        .rounded(px(NOTION_CHECKBOX_RADIUS_PX))
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
        assert_eq!(BLOCK_PREFIX_WIDTH_PX, 24.0);
        assert_eq!(CALLOUT_PREFIX_WIDTH_PX, 32.0);
        assert_eq!(NOTION_PREFIX_LINE_HEIGHT_PX, 24.0);
        assert_eq!(NOTION_CHECKBOX_SIZE_PX, 16.0);
        assert_eq!(NOTION_CHECKBOX_RADIUS_PX, 2.0);
        assert_eq!(NOTION_FOLD_HOVER_SIZE_PX, 20.0);
        assert_eq!(NOTION_FOLD_ICON_SIZE_PX, 10.0);
        assert_eq!(NOTION_FOLD_HOVER_RADIUS_PX, 3.0);
    }

    #[test]
    fn fold_indicator_uses_balanced_solid_triangle_geometry() {
        assert_eq!(
            fold_indicator_points(true),
            [(2.0, 1.5), (8.0, 5.0), (2.0, 8.5)]
        );
        assert_eq!(
            fold_indicator_points(false),
            [(1.5, 2.0), (8.5, 2.0), (5.0, 8.0)]
        );
    }

    #[test]
    fn fold_control_visibility_follows_focus_until_collapsed() {
        let expanded = BlockPrefixSnapshot::Heading { collapsed: false };
        let collapsed = BlockPrefixSnapshot::Heading { collapsed: true };

        assert!(!fold_control_visible(&expanded, false));
        assert!(fold_control_visible(&expanded, true));
        assert!(fold_control_visible(&collapsed, false));
        assert!(!fold_control_visible(&BlockPrefixSnapshot::None, true));
    }

    #[test]
    fn heading_fold_control_centers_in_the_heading_line_box() {
        assert_eq!(
            fold_prefix_line_height_px(&BlockPrefixSnapshot::Heading { collapsed: false }, 38.0,),
            38.0
        );
        assert_eq!(
            fold_prefix_line_height_px(&BlockPrefixSnapshot::Toggle { collapsed: false }, 38.0,),
            NOTION_PREFIX_LINE_HEIGHT_PX
        );
    }
}
