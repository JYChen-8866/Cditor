use gpui::{AnyElement, IntoElement, ParentElement, Styled, div, px, rgb};

use crate::gui::GuiTheme;
use crate::gui::skeleton::{SkeletonItem, SkeletonRows, SkeletonVariant};

const MAX_WINDOW_SKELETON_BLOCKS: usize = 12;
const ESTIMATED_SKELETON_BLOCK_HEIGHT_PX: f64 = 56.0;

pub fn render_document_skeleton_window(
    height_px: f64,
    viewport_offset_px: f64,
    theme: GuiTheme,
) -> AnyElement {
    let count = skeleton_block_count(visible_skeleton_height(height_px));
    div()
        .relative()
        .h(px(height_px.max(1.0) as f32))
        .child(
            div()
                .absolute()
                .left_0()
                .right_0()
                .top(px(
                    skeleton_viewport_offset(height_px, viewport_offset_px) as f32
                ))
                .px_2()
                .py_1()
                .flex()
                .flex_col()
                .gap_4()
                .children((0..count).map(|index| render_window_skeleton_block(index, theme))),
        )
        .into_any_element()
}

pub fn render_document_window_error(
    height_px: f64,
    viewport_offset_px: f64,
    message: &str,
    theme: GuiTheme,
) -> AnyElement {
    div()
        .relative()
        .h(px(height_px.max(1.0) as f32))
        .w_full()
        .child(
            div()
                .absolute()
                .left_0()
                .right_0()
                .top(px(
                    skeleton_viewport_offset(height_px, viewport_offset_px) as f32
                ))
                .min_h(px(120.0))
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .max_w(px(560.0))
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(rgb(theme.border))
                        .bg(rgb(theme.panel))
                        .p_4()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .text_color(rgb(theme.text))
                        .child("无法加载当前内容")
                        .child(
                            div()
                                .text_sm()
                                .text_color(rgb(theme.muted))
                                .child(message.to_owned()),
                        ),
                ),
        )
        .into_any_element()
}

fn render_window_skeleton_block(index: usize, theme: GuiTheme) -> AnyElement {
    match index % 6 {
        0 => SkeletonItem::new(SkeletonVariant::Heading)
            .width(gpui::relative(0.56))
            .height_px(24.0)
            .render(theme),
        3 => div()
            .w_full()
            .rounded(px(8.0))
            .p_3()
            .bg(gpui::rgb(theme.surface))
            .child(
                SkeletonRows::new(3)
                    .row_height_px(12.0)
                    .last_width(gpui::relative(0.42))
                    .render(theme),
            )
            .into_any_element(),
        _ => SkeletonRows::new(2)
            .last_width(if index % 2 == 0 {
                gpui::relative(0.72)
            } else {
                gpui::relative(0.48)
            })
            .render(theme),
    }
}

fn skeleton_block_count(height_px: f64) -> usize {
    let estimated = (height_px / ESTIMATED_SKELETON_BLOCK_HEIGHT_PX).ceil() as usize;
    estimated.clamp(1, MAX_WINDOW_SKELETON_BLOCKS)
}

fn visible_skeleton_height(height_px: f64) -> f64 {
    height_px.clamp(1.0, 720.0)
}

fn skeleton_viewport_offset(height_px: f64, viewport_offset_px: f64) -> f64 {
    viewport_offset_px.clamp(0.0, (height_px - 1.0).max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skeleton_window_count_is_bounded() {
        assert_eq!(skeleton_block_count(1.0), 1);
        assert_eq!(skeleton_block_count(10_000.0), MAX_WINDOW_SKELETON_BLOCKS);
    }

    #[test]
    fn skeleton_tracks_the_visible_part_of_a_large_placeholder() {
        assert_eq!(skeleton_viewport_offset(10_000.0, 1_536.0), 1_536.0);
        assert_eq!(skeleton_viewport_offset(10_000.0, -20.0), 0.0);
        assert_eq!(skeleton_viewport_offset(10_000.0, 12_000.0), 9_999.0);
        assert_eq!(visible_skeleton_height(10_000.0), 720.0);
    }
}
