use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, App, InteractiveElement, IntoElement, MouseButton, MouseDownEvent, ParentElement,
    PathBuilder, Styled, Window, canvas, div, point, px, rgb,
};

use crate::block::chrome::{
    BLOCK_PREFIX_WIDTH_PX, CALLOUT_PREFIX_WIDTH_PX, NOTION_LIST_PREFIX_WIDTH_PX,
};
use crate::theme::GuiTheme;
use cditor_component::SvgIcon;
use cditor_core::block::BlockPrefixSnapshot;
use cditor_core::rich_text::CalloutVariant;

pub type TodoToggleHandler = Box<dyn Fn(&MouseDownEvent, &mut Window, &mut App) + 'static>;
pub type FoldToggleHandler = Box<dyn Fn(&MouseDownEvent, &mut Window, &mut App) + 'static>;

const NOTION_PREFIX_LINE_HEIGHT_PX: f32 = 24.0;
const NOTION_LIST_MARKER_SLOT_HEIGHT_PX: f32 = 24.0;
const NOTION_CHECKBOX_SIZE_PX: f32 = 16.0;
const NOTION_CHECKBOX_RADIUS_PX: f32 = 2.0;
const NOTION_FOLD_HOVER_SIZE_PX: f32 = 22.0;
const NOTION_FOLD_ICON_SIZE_PX: f32 = 10.0;
const NOTION_FOLD_ICON_STROKE_PX: f32 = 1.5;
const NOTION_FOLD_HOVER_RADIUS_PX: f32 = 3.0;
const NOTION_BULLET_OUTER_SIZE_PX: f32 = 5.0;
const NOTION_BULLET_CANVAS_SIZE_PX: f32 = NOTION_BULLET_OUTER_SIZE_PX;
const NOTION_BULLET_STROKE_WIDTH_PX: f32 = 1.25;
const NOTION_HEADING_LABEL_SIZE_PX: f32 = 16.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BulletMarkerShape {
    SolidCircle,
    HollowCircle,
    SolidSquare,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct BulletMarkerStyle {
    shape: BulletMarkerShape,
    size_px: f32,
    stroke_width_px: f32,
}

impl BulletMarkerStyle {
    const fn path_size_px(self) -> f32 {
        self.size_px - self.stroke_width_px
    }

    #[cfg(test)]
    const fn painted_outer_size_px(self) -> f32 {
        self.path_size_px() + self.stroke_width_px
    }
}

const fn bullet_marker_style_for_depth(depth: usize) -> BulletMarkerStyle {
    match depth % 3 {
        0 => BulletMarkerStyle {
            shape: BulletMarkerShape::SolidCircle,
            size_px: NOTION_BULLET_OUTER_SIZE_PX,
            stroke_width_px: 0.0,
        },
        1 => BulletMarkerStyle {
            shape: BulletMarkerShape::HollowCircle,
            size_px: NOTION_BULLET_OUTER_SIZE_PX,
            stroke_width_px: NOTION_BULLET_STROKE_WIDTH_PX,
        },
        _ => BulletMarkerStyle {
            shape: BulletMarkerShape::SolidSquare,
            size_px: NOTION_BULLET_OUTER_SIZE_PX,
            stroke_width_px: 0.0,
        },
    }
}

pub fn render_block_prefix(
    prefix: &BlockPrefixSnapshot,
    marker_lane_width_px: f32,
    theme: GuiTheme,
    editable: bool,
    on_fold_toggle: Option<FoldToggleHandler>,
    focused: bool,
    block_line_height_px: f32,
    hovered: bool,
    heading_level: Option<u8>,
) -> AnyElement {
    match prefix {
        BlockPrefixSnapshot::None => div()
            .w(px(marker_lane_width_px))
            .flex_shrink_0()
            .into_any_element(),
        BlockPrefixSnapshot::Bullet { .. } | BlockPrefixSnapshot::Number { .. } => div()
            .w(px(marker_lane_width_px))
            .flex_shrink_0()
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
        BlockPrefixSnapshot::Heading { collapsed } => {
            let on_fold_toggle = if hovered { on_fold_toggle } else { None };
            div()
                .w(px(BLOCK_PREFIX_WIDTH_PX))
                .h(px(fold_prefix_line_height_px(prefix, block_line_height_px)))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .text_color(rgb(theme.text))
                .when(editable && hovered, |this| this.cursor_pointer())
                .when_some(on_fold_toggle, |this, handler| {
                    this.on_mouse_down(MouseButton::Left, handler)
                })
                .child(if hovered {
                    render_fold_indicator(*collapsed, true, theme)
                } else {
                    render_heading_label(heading_level, theme)
                })
                .into_any_element()
        }
        BlockPrefixSnapshot::Toggle { collapsed } => {
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

fn heading_label_source(level: Option<u8>) -> (&'static str, &'static [u8]) {
    const HEADING_1: &[u8] = include_bytes!("../../../../assets/icons/heading-1.svg");
    const HEADING_2: &[u8] = include_bytes!("../../../../assets/icons/heading-2.svg");
    const HEADING_3: &[u8] = include_bytes!("../../../../assets/icons/heading-3.svg");
    match level {
        Some(1) => ("heading-label-1", HEADING_1),
        Some(2) => ("heading-label-2", HEADING_2),
        Some(3) => ("heading-label-3", HEADING_3),
        _ => ("heading-label-1", HEADING_1),
    }
}

fn render_heading_label(level: Option<u8>, theme: GuiTheme) -> AnyElement {
    let (key, bytes) = heading_label_source(level);
    div()
        .size(px(NOTION_FOLD_HOVER_SIZE_PX))
        .flex()
        .items_center()
        .justify_center()
        .child(
            SvgIcon::new(key, bytes)
                .color(rgb(theme.muted))
                .size(px(NOTION_HEADING_LABEL_SIZE_PX)),
        )
        .into_any_element()
}

fn render_bullet_marker(depth: usize, theme: GuiTheme) -> AnyElement {
    let style = bullet_marker_style_for_depth(depth);
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let center_x = bounds.origin.x + bounds.size.width / 2.0;
            let center_y = bounds.origin.y + bounds.size.height / 2.0;
            let path_size_px = style.path_size_px();
            let half = px(path_size_px / 2.0);
            let color = rgb(theme.text);
            let path = match style.shape {
                BulletMarkerShape::SolidCircle | BulletMarkerShape::HollowCircle => {
                    let mut path = if style.shape == BulletMarkerShape::HollowCircle {
                        PathBuilder::stroke(px(style.stroke_width_px))
                    } else {
                        PathBuilder::fill()
                    };
                    path.move_to(point(center_x + half, center_y));
                    path.arc_to(
                        point(half, half),
                        px(0.0),
                        false,
                        true,
                        point(center_x - half, center_y),
                    );
                    path.arc_to(
                        point(half, half),
                        px(0.0),
                        false,
                        true,
                        point(center_x + half, center_y),
                    );
                    path.close();
                    path.build()
                }
                BulletMarkerShape::SolidSquare => {
                    let mut path = PathBuilder::fill();
                    path.add_polygon(
                        &[
                            point(center_x - half, center_y - half),
                            point(center_x + half, center_y - half),
                            point(center_x + half, center_y + half),
                            point(center_x - half, center_y + half),
                        ],
                        true,
                    );
                    path.build()
                }
            };
            if let Ok(path) = path {
                window.paint_path(path, color);
            }
        },
    )
    .size(px(NOTION_BULLET_CANVAS_SIZE_PX))
    .into_any_element()
}

pub fn render_block_content_prefix(
    prefix: &BlockPrefixSnapshot,
    theme: GuiTheme,
    editable: bool,
    on_todo_toggle: Option<TodoToggleHandler>,
) -> Option<AnyElement> {
    match prefix {
        BlockPrefixSnapshot::Bullet { depth } => Some(
            div()
                .w(px(NOTION_LIST_PREFIX_WIDTH_PX))
                .h(px(NOTION_LIST_MARKER_SLOT_HEIGHT_PX))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_start()
                .child(render_bullet_marker(*depth, theme))
                .into_any_element(),
        ),
        BlockPrefixSnapshot::Number { ordinal } => Some(
            div()
                .w(px(NOTION_LIST_PREFIX_WIDTH_PX))
                .h(px(NOTION_LIST_MARKER_SLOT_HEIGHT_PX))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_start()
                .text_color(rgb(theme.text))
                .child(format!("{ordinal}."))
                .into_any_element(),
        ),
        BlockPrefixSnapshot::Todo { checked } => Some(
            div()
                .w(px(NOTION_LIST_PREFIX_WIDTH_PX))
                .h(px(NOTION_LIST_MARKER_SLOT_HEIGHT_PX))
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
        BlockPrefixSnapshot::Heading { .. } => true,
        BlockPrefixSnapshot::Toggle { collapsed } => *collapsed || focused,
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
                    let mut path = PathBuilder::stroke(px(NOTION_FOLD_ICON_STROKE_PX));
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
        [(2.5, 1.5), (7.0, 5.0), (2.5, 8.5)]
    } else {
        [(1.5, 2.5), (5.0, 7.0), (8.5, 2.5)]
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
        assert_eq!(BLOCK_PREFIX_WIDTH_PX, 22.0);
        assert_eq!(NOTION_LIST_PREFIX_WIDTH_PX, 24.0);
        assert_eq!(CALLOUT_PREFIX_WIDTH_PX, 36.0);
        assert_eq!(NOTION_PREFIX_LINE_HEIGHT_PX, 24.0);
        assert_eq!(NOTION_LIST_MARKER_SLOT_HEIGHT_PX, 24.0);
        assert_eq!(NOTION_CHECKBOX_SIZE_PX, 16.0);
        assert_eq!(NOTION_CHECKBOX_RADIUS_PX, 2.0);
        assert_eq!(NOTION_FOLD_HOVER_SIZE_PX, 22.0);
        assert_eq!(NOTION_FOLD_ICON_SIZE_PX, 10.0);
        assert_eq!(NOTION_FOLD_ICON_STROKE_PX, 1.5);
        assert_eq!(NOTION_FOLD_HOVER_RADIUS_PX, 3.0);
        assert_eq!(NOTION_HEADING_LABEL_SIZE_PX, 16.0);
        assert_eq!(NOTION_BULLET_CANVAS_SIZE_PX, 5.0);
    }

    #[test]
    fn bullet_markers_cycle_notion_geometry_by_depth() {
        let solid_circle = bullet_marker_style_for_depth(0);
        let hollow_circle = bullet_marker_style_for_depth(1);
        let solid_square = bullet_marker_style_for_depth(2);

        assert_eq!(solid_circle.shape, BulletMarkerShape::SolidCircle);
        assert_eq!(solid_circle.size_px, 5.0);
        assert_eq!(hollow_circle.shape, BulletMarkerShape::HollowCircle);
        assert_eq!(hollow_circle.size_px, 5.0);
        assert_eq!(hollow_circle.stroke_width_px, 1.25);
        assert_eq!(solid_square.shape, BulletMarkerShape::SolidSquare);
        assert_eq!(solid_square.size_px, 5.0);
        assert_eq!(solid_circle.size_px, hollow_circle.size_px);
        assert_eq!(hollow_circle.size_px, solid_square.size_px);
        assert_eq!(solid_circle.painted_outer_size_px(), 5.0);
        assert_eq!(hollow_circle.painted_outer_size_px(), 5.0);
        assert_eq!(solid_square.painted_outer_size_px(), 5.0);
        assert_eq!(bullet_marker_style_for_depth(3), solid_circle);
        assert_eq!(bullet_marker_style_for_depth(4), hollow_circle);
        assert_eq!(bullet_marker_style_for_depth(5), solid_square);
    }

    #[test]
    fn fold_indicator_uses_a_rotated_open_chevron() {
        assert_eq!(
            fold_indicator_points(true),
            [(2.5, 1.5), (7.0, 5.0), (2.5, 8.5)]
        );
        assert_eq!(
            fold_indicator_points(false),
            [(1.5, 2.5), (5.0, 7.0), (8.5, 2.5)]
        );
    }

    #[test]
    fn toggle_fold_control_follows_collapsed_state_and_focus() {
        let toggle = BlockPrefixSnapshot::Toggle { collapsed: false };

        assert!(!fold_control_visible(&toggle, false));
        assert!(fold_control_visible(&toggle, true));
        assert!(fold_control_visible(
            &BlockPrefixSnapshot::Toggle { collapsed: true },
            false
        ));
        assert!(!fold_control_visible(&BlockPrefixSnapshot::None, true));
    }

    #[test]
    fn heading_label_source_maps_levels_with_a_first_level_fallback() {
        let (key1, bytes1) = heading_label_source(Some(1));
        let (key2, bytes2) = heading_label_source(Some(2));
        let (key3, bytes3) = heading_label_source(Some(3));
        let (fallback_key, fallback_bytes) = heading_label_source(None);

        assert_eq!(key1, "heading-label-1");
        assert_eq!(key2, "heading-label-2");
        assert_eq!(key3, "heading-label-3");
        assert_eq!(fallback_key, key1);
        assert!(bytes1.starts_with(b"<svg"));
        assert!(bytes2.starts_with(b"<svg"));
        assert!(bytes3.starts_with(b"<svg"));
        assert!(fallback_bytes == bytes1);
        assert_eq!(heading_label_source(Some(4)).0, key1);
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
