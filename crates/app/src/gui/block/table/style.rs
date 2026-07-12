use gpui::{Pixels, px};

use crate::gui::GuiTheme;
use cditor_core::layout::{NOTION_TABLE_CELL_LINE_HEIGHT_PX, NOTION_TABLE_CELL_PADDING_Y_PX};

pub(super) const V1_TABLE_RADIUS_PX: f32 = 0.0;
#[cfg(test)]
pub(super) const V1_TABLE_CELL_MIN_WIDTH_PX: f32 = 120.0;
pub(super) const V1_TABLE_CELL_PADDING_X_PX: f32 = 10.0;
pub(super) const V1_TABLE_CELL_PADDING_Y_PX: f32 = NOTION_TABLE_CELL_PADDING_Y_PX as f32;
pub(super) const V1_TABLE_EMPTY_PADDING_PX: f32 = 8.0;
pub(super) const TABLE_AXIS_HANDLE_SIZE_PX: f32 = 16.0;
pub(super) const TABLE_AXIS_ROW_HANDLE_LEFT_PX: f32 = -28.0;
pub(super) const TABLE_AXIS_COLUMN_HANDLE_TOP_PX: f32 = -15.0;
pub(super) const TABLE_CELL_GUTTER_SIZE_PX: f32 = 14.0;
pub(super) const TABLE_CELL_GUTTER_THICKNESS_PX: f32 = 2.0;
pub(super) const TABLE_ACTIVE_CELL_BORDER_WIDTH_PX: f32 = 2.0;
pub(super) const TABLE_ACTIVE_CELL_RADIUS_PX: f32 = 0.0;
#[allow(dead_code)]
pub(super) const TABLE_RESIZE_HANDLE_THICKNESS_PX: f32 = 8.0;
#[allow(dead_code)]
pub(super) const TABLE_RESIZE_HANDLE_LINE_THICKNESS_PX: f32 = 2.0;
#[allow(dead_code)]
pub(crate) const TABLE_RESIZE_INDICATOR_THICKNESS_PX: f32 = 2.0;
pub(super) const TABLE_AXIS_OUTLINE_THICKNESS_PX: f32 = 2.0;

pub(super) fn table_cell_text_size() -> Pixels {
    px(14.0)
}

pub(super) fn table_cell_line_height() -> Pixels {
    px(NOTION_TABLE_CELL_LINE_HEIGHT_PX as f32)
}

pub(super) fn table_cell_hover_background(theme: GuiTheme, header: bool) -> u32 {
    if header {
        0xefeeeb
    } else {
        theme.hover_surface
    }
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

pub(super) fn table_axis_handle_background(theme: GuiTheme, selected: bool) -> u32 {
    if selected {
        theme.table_active_border
    } else {
        theme.surface
    }
}

pub(super) fn table_axis_handle_foreground(theme: GuiTheme, selected: bool) -> u32 {
    if selected { theme.surface } else { theme.muted }
}

pub(super) fn table_surface_background(theme: GuiTheme) -> u32 {
    theme.surface
}

pub(super) fn table_border_color(theme: GuiTheme) -> u32 {
    theme.border
}

pub(super) fn table_cell_border_color(theme: GuiTheme, _selected: bool) -> u32 {
    table_border_color(theme)
}

pub(super) fn table_header_background(theme: GuiTheme) -> u32 {
    theme.table_header_background
}

pub(super) fn table_active_border_color(theme: GuiTheme) -> u32 {
    theme.table_active_border
}
