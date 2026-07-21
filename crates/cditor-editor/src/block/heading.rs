use gpui::{AnyElement, FontWeight, IntoElement, ParentElement, Styled, div, px};

pub const NOTION_HEADING_1_TEXT_SIZE_PX: f32 = 30.0;
pub const NOTION_HEADING_2_TEXT_SIZE_PX: f32 = 24.0;
pub const NOTION_HEADING_3_TEXT_SIZE_PX: f32 = 20.0;
pub const HEADING_4_TEXT_SIZE_PX: f32 = 18.0;
pub const HEADING_5_TEXT_SIZE_PX: f32 = 16.0;
pub const HEADING_6_TEXT_SIZE_PX: f32 = 14.0;

pub fn render_heading(level: u8, content: AnyElement) -> AnyElement {
    div()
        .text_size(px(heading_text_size_px(level)))
        .font_weight(heading_font_weight(level))
        .child(content)
        .into_any_element()
}

fn heading_text_size_px(level: u8) -> f32 {
    match level {
        1 => NOTION_HEADING_1_TEXT_SIZE_PX,
        2 => NOTION_HEADING_2_TEXT_SIZE_PX,
        3 => NOTION_HEADING_3_TEXT_SIZE_PX,
        4 => HEADING_4_TEXT_SIZE_PX,
        5 => HEADING_5_TEXT_SIZE_PX,
        _ => HEADING_6_TEXT_SIZE_PX,
    }
}

fn heading_font_weight(level: u8) -> FontWeight {
    match level {
        1..=3 => FontWeight::SEMIBOLD,
        _ => FontWeight::MEDIUM,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heading_sizes_cover_all_six_levels() {
        assert_eq!(heading_text_size_px(1), 30.0);
        assert_eq!(heading_text_size_px(2), 24.0);
        assert_eq!(heading_text_size_px(3), 20.0);
        assert_eq!(heading_text_size_px(4), 18.0);
        assert_eq!(heading_text_size_px(5), 16.0);
        assert_eq!(heading_text_size_px(6), 14.0);
    }
}
