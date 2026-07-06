use gpui::{AnyElement, IntoElement, ParentElement, Styled, div, px};

use crate::gui::GuiTheme;
use crate::gui::skeleton::{SkeletonItem, SkeletonRows, SkeletonVariant};

const MAX_WINDOW_SKELETON_BLOCKS: usize = 12;
const ESTIMATED_SKELETON_BLOCK_HEIGHT_PX: f64 = 56.0;

pub fn render_document_skeleton_window(height_px: f64, theme: GuiTheme) -> AnyElement {
    let count = skeleton_block_count(height_px);
    div()
        .h(px(height_px.max(1.0) as f32))
        .px_2()
        .py_1()
        .flex()
        .flex_col()
        .gap_4()
        .children((0..count).map(|index| render_window_skeleton_block(index, theme)))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skeleton_window_count_is_bounded() {
        assert_eq!(skeleton_block_count(1.0), 1);
        assert_eq!(skeleton_block_count(10_000.0), MAX_WINDOW_SKELETON_BLOCKS);
    }
}
