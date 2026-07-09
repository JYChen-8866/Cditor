use gpui::{Pixels, px};

use crate::gui::GuiTheme;

pub(super) const V1_TABLE_RADIUS_PX: f32 = 8.0;
#[cfg(test)]
pub(super) const V1_TABLE_CELL_MIN_WIDTH_PX: f32 = 120.0;
pub(super) const V1_TABLE_CELL_PADDING_X_PX: f32 = 10.0;
pub(super) const V1_TABLE_CELL_PADDING_Y_PX: f32 = 8.0;
pub(super) const V1_TABLE_EMPTY_PADDING_PX: f32 = 8.0;
pub(super) const TABLE_AXIS_HANDLE_SIZE_PX: f32 = 16.0;
pub(super) const TABLE_AXIS_HANDLE_OFFSET_PX: f32 = -9.0;
pub(super) const TABLE_ACTIVE_CELL_BORDER_WIDTH_PX: f32 = 2.0;
pub(super) const TABLE_RESIZE_HANDLE_THICKNESS_PX: f32 = 8.0;
pub(super) const TABLE_RESIZE_HANDLE_LINE_THICKNESS_PX: f32 = 2.0;
pub(super) const TABLE_RESIZE_INDICATOR_THICKNESS_PX: f32 = 2.0;

pub(super) fn table_cell_text_size() -> Pixels {
    px(14.0)
}

pub(super) fn table_cell_line_height() -> Pixels {
    table_cell_text_size() * 1.25
}

pub(super) fn table_cell_background(
    theme: GuiTheme,
    header: bool,
    background_color: Option<&str>,
) -> u32 {
    if let Some(color) = background_color.and_then(|color| table_style_color(theme, color)) {
        color
    } else if header {
        table_header_background(theme)
    } else {
        table_surface_background(theme)
    }
}

pub(super) fn table_style_color(theme: GuiTheme, color: &str) -> Option<u32> {
    match color {
        "action_background" => Some(theme.action_background),
        "table_header_background" => Some(theme.table_header_background),
        _ => color
            .strip_prefix('#')
            .and_then(|hex| u32::from_str_radix(hex, 16).ok()),
    }
}

pub(super) fn table_selected_cell_background(theme: GuiTheme) -> u32 {
    theme.action_background
}

pub(super) fn table_surface_background(theme: GuiTheme) -> u32 {
    theme.surface
}

pub(super) fn table_border_color(theme: GuiTheme) -> u32 {
    theme.border
}

pub(super) fn table_cell_border_color(theme: GuiTheme, selected: bool) -> u32 {
    if selected {
        table_selected_cell_background(theme)
    } else {
        table_border_color(theme)
    }
}

pub(super) fn table_header_background(theme: GuiTheme) -> u32 {
    theme.table_header_background
}

pub(super) fn table_active_border_color(theme: GuiTheme) -> u32 {
    theme.table_active_border
}
