mod active_border;
mod axis_grip;
mod cell;
mod cell_gutter;
mod cell_handle;
mod cell_menu;
mod chrome;
mod grid;
pub(crate) mod menu;
mod render;
mod reorder;
mod resize;
mod selection;
mod style;
mod text;
mod toolbar;

pub(crate) use cell_menu::render_table_cell_menu;
pub(crate) use chrome::{
    TableChromeOverlays, render_table_axis_overlays, render_table_chrome_viewport,
    table_chrome_viewport_origins,
};
pub(crate) use render::render_table_block;
pub(crate) use reorder::{
    TableReorderPreview, table_axis_track_sizes, table_reorder_indicator_edge_px_for_preview,
};
pub(crate) use resize::{
    TableResizePreview, render_table_resize_overlays, table_resize_indicator_edge_px,
};
pub(crate) use selection::{
    TableAxis, TableAxisSelection, TableCellRangeSelection, TableCellSelection,
};
pub(crate) use style::TABLE_RESIZE_INDICATOR_THICKNESS_PX;
pub(crate) use toolbar::{
    TableToolbarEditorOrigin, render_table_axis_toolbar, table_content_editor_origin,
    table_toolbar_editor_origin,
};

fn table_trace_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("CDITOR_TRACE_TABLE")
            .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
            .unwrap_or(false)
    })
}

fn trace_table(event: &str, details: impl std::fmt::Display) {
    if table_trace_enabled() {
        eprintln!("[cditor][table][gui][{event}] {details}");
    }
}

#[cfg(test)]
mod tests {
    use gpui::px;

    use crate::gui::GuiTheme;
    use cditor_core::rich_text::{TableCellPayload, TablePayload, TableRowPayload};
    use cditor_runtime::TableCellPosition;

    use super::cell::{is_active_cell, is_header_cell};
    use super::selection::{
        TableAxis, TableAxisSelection, TableCellRangeSelection, cell_selected,
        column_handle_selected, row_handle_selected,
    };
    use super::style::{
        TABLE_ACTIVE_CELL_BORDER_WIDTH_PX, V1_TABLE_CELL_MIN_WIDTH_PX, V1_TABLE_CELL_PADDING_X_PX,
        V1_TABLE_CELL_PADDING_Y_PX, V1_TABLE_RADIUS_PX, table_active_border_color,
        table_border_color, table_cell_background, table_cell_line_height, table_cell_text_size,
        table_header_background, table_style_color, table_surface_background,
    };

    #[test]
    fn v1_table_geometry_constants_are_stable() {
        assert_eq!(V1_TABLE_RADIUS_PX, 0.0);
        assert_eq!(V1_TABLE_CELL_MIN_WIDTH_PX, 120.0);
        assert_eq!(V1_TABLE_CELL_PADDING_X_PX, 10.0);
        assert_eq!(V1_TABLE_CELL_PADDING_Y_PX, 7.0);
        // H-014: Updated to Notion-like colors
        assert_eq!(GuiTheme::light().table_header_background, 0xf7f6f4);
        assert_eq!(GuiTheme::light().table_active_border, 0x2383e2);
        assert_eq!(GuiTheme::light().border, 0xe9e9e7);
        assert_eq!(
            table_surface_background(GuiTheme::light()),
            GuiTheme::light().surface
        );
        assert_eq!(
            table_border_color(GuiTheme::light()),
            GuiTheme::light().border
        );
        assert_eq!(
            table_header_background(GuiTheme::light()),
            GuiTheme::light().table_header_background
        );
        assert_eq!(
            table_active_border_color(GuiTheme::light()),
            GuiTheme::light().table_active_border
        );
    }

    #[test]
    fn table_header_detection_follows_payload_header_rows_and_cols() {
        let table = TablePayload {
            rows: vec![TableRowPayload {
                cells: vec![TableCellPayload::plain("A"), TableCellPayload::plain("B")],
                height: Default::default(),
            }],
            columns: Vec::new(),
            header_rows: 1,
            header_cols: 1,
            header_style: Default::default(),
        };

        assert!(is_header_cell(&table, 0, 0));
        assert!(is_header_cell(&table, 0, 1));
        assert!(is_header_cell(&table, 1, 0));
        assert!(!is_header_cell(&table, 1, 1));
    }

    #[test]
    fn table_active_cell_detection_follows_projection_position() {
        let focused = Some(TableCellPosition { row: 2, col: 1 });

        assert!(is_active_cell(focused, 2, 1));
        assert!(!is_active_cell(focused, 1, 1));
        assert!(!is_active_cell(None, 2, 1));
    }

    #[test]
    fn table_cell_background_does_not_add_editing_focus_wash() {
        let theme = GuiTheme::light();

        assert_eq!(
            table_cell_background(theme, true, None),
            theme.table_header_background
        );
        assert_eq!(table_cell_background(theme, false, None), theme.surface);
        assert_eq!(
            table_cell_background(theme, false, Some("action_background")),
            theme.action_background
        );
        assert_eq!(table_style_color(theme, "#ff00aa"), Some(0xff00aa));
    }

    #[test]
    fn projected_table_grid_keeps_the_theme_border_color() {
        let theme = GuiTheme::light();

        assert_eq!(table_border_color(theme), theme.border);
    }

    #[test]
    fn table_cell_line_height_is_stable_for_empty_active_cells() {
        assert_eq!(table_cell_text_size(), px(14.0));
        assert_eq!(
            table_cell_line_height(),
            px(cditor_core::layout::NOTION_TABLE_CELL_LINE_HEIGHT_PX as f32)
        );
    }

    #[test]
    fn table_active_cell_border_is_overlay_style_from_theme() {
        let theme = GuiTheme::light();

        assert_eq!(TABLE_ACTIVE_CELL_BORDER_WIDTH_PX, 2.0);
        assert_eq!(
            super::style::TABLE_CELL_GUTTER_THICKNESS_PX,
            TABLE_ACTIVE_CELL_BORDER_WIDTH_PX
        );
        assert_eq!(table_active_border_color(theme), theme.table_active_border);
    }

    #[test]
    fn table_axis_selection_selects_whole_row_or_column() {
        let row = Some(TableAxisSelection::new(7, TableAxis::Row, 2));
        let column = Some(TableAxisSelection::new(7, TableAxis::Column, 1));

        assert!(cell_selected(row, None, 7, 2, 0));
        assert!(cell_selected(row, None, 7, 2, 3));
        assert!(!cell_selected(row, None, 7, 1, 3));
        assert!(!cell_selected(row, None, 8, 2, 3));

        assert!(cell_selected(column, None, 7, 0, 1));
        assert!(cell_selected(column, None, 7, 9, 1));
        assert!(!cell_selected(column, None, 7, 9, 2));
    }

    #[test]
    fn table_range_selection_selects_normalized_cell_rectangle() {
        let range = Some(TableCellRangeSelection::new(7, 2, 2, 1, 1));

        assert!(range.unwrap().is_multi_cell());
        assert!(cell_selected(None, range, 7, 1, 1));
        assert!(cell_selected(None, range, 7, 2, 2));
        assert!(!cell_selected(None, range, 7, 0, 1));
        assert!(!cell_selected(None, range, 8, 1, 1));
        assert!(!TableCellRangeSelection::new(7, 1, 1, 1, 1).is_multi_cell());
    }

    #[test]
    fn table_axis_selection_marks_only_the_matching_handle() {
        let row = Some(TableAxisSelection::new(7, TableAxis::Row, 2));
        let column = Some(TableAxisSelection::new(7, TableAxis::Column, 1));

        assert!(row_handle_selected(row, 7, 2));
        assert!(!row_handle_selected(row, 7, 1));
        assert!(!row_handle_selected(column, 7, 2));

        assert!(column_handle_selected(column, 7, 1));
        assert!(!column_handle_selected(column, 7, 2));
        assert!(!column_handle_selected(row, 7, 1));
    }

    #[test]
    fn table_hscrollbar_hidden_when_content_fits_viewport() {
        use crate::gui::overlay::table::table_hscroll_thumb;

        // Content narrower than viewport → no scrollbar.
        assert!(table_hscroll_thumb(800.0, 600.0, 0.0, 0.0).is_none());
        // Not laid out yet (zero viewport) → no scrollbar.
        assert!(table_hscroll_thumb(0.0, 1200.0, 400.0, 0.0).is_none());
    }

    #[test]
    fn table_hscrollbar_thumb_tracks_scroll_progress() {
        use crate::gui::overlay::table::table_hscroll_thumb;

        // Viewport 600 over content 1200 → thumb covers half the track.
        let max_offset = 600.0; // content - viewport
        let at_start = table_hscroll_thumb(600.0, 1200.0, max_offset, 0.0).unwrap();
        assert_eq!(at_start.width_px, 300.0);
        assert_eq!(at_start.left_px, 0.0);

        // Fully scrolled: offset stored as negative max.
        let at_end = table_hscroll_thumb(600.0, 1200.0, max_offset, -max_offset).unwrap();
        assert_eq!(at_end.width_px, 300.0);
        // Thumb pinned to the right edge: travel = viewport - thumb = 300.
        assert_eq!(at_end.left_px, 300.0);

        // Halfway.
        let mid = table_hscroll_thumb(600.0, 1200.0, max_offset, -max_offset / 2.0).unwrap();
        assert_eq!(mid.left_px, 150.0);
    }

    #[test]
    fn table_hscrollbar_thumb_respects_minimum_width() {
        use crate::gui::overlay::table::table_hscroll_thumb;

        // Very wide content would give a tiny thumb; clamp to the minimum.
        let thumb = table_hscroll_thumb(300.0, 6000.0, 5700.0, 0.0).unwrap();
        assert_eq!(thumb.width_px, 32.0);
    }

    #[test]
    fn table_hscrollbar_drag_travel_is_viewport_minus_thumb() {
        use crate::gui::overlay::table::{table_hscroll_thumb, table_hscroll_thumb_travel};

        let thumb = table_hscroll_thumb(600.0, 1200.0, 600.0, 0.0).unwrap();

        assert_eq!(table_hscroll_thumb_travel(600.0, thumb.width_px), 300.0);
        assert_eq!(table_hscroll_thumb_travel(120.0, 180.0), 0.0);
    }

    #[test]
    fn table_hscrollbar_track_excludes_table_left_gutter() {
        use crate::gui::overlay::table::table_hscroll_track_width;

        assert_eq!(table_hscroll_track_width(628.0, 28.0), 600.0);
        assert_eq!(table_hscroll_track_width(20.0, 28.0), 0.0);
    }

    #[test]
    fn table_hscrollbar_reserves_bottom_space_outside_table() {
        use crate::gui::overlay::table::table_hscroll_block_height;

        assert_eq!(table_hscroll_block_height(144.0), 158.0);
    }
}
