mod cell;
pub(crate) mod menu;
mod render;
mod reorder;
mod resize;
mod selection;
mod style;
mod text;
mod toolbar;

pub(crate) use render::render_table_block;
pub(crate) use reorder::TableReorderPreview;
pub(crate) use resize::TableResizePreview;
pub(crate) use selection::{TableAxis, TableAxisSelection, TableCellRangeSelection};

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
        table_border_color, table_cell_background, table_cell_border_color, table_cell_line_height,
        table_cell_text_size, table_header_background, table_selected_cell_background,
        table_style_color, table_surface_background,
    };

    #[test]
    fn v1_table_geometry_constants_match_editor2() {
        assert_eq!(V1_TABLE_RADIUS_PX, 8.0);
        assert_eq!(V1_TABLE_CELL_MIN_WIDTH_PX, 120.0);
        assert_eq!(V1_TABLE_CELL_PADDING_X_PX, 10.0);
        assert_eq!(V1_TABLE_CELL_PADDING_Y_PX, 8.0);
        assert_eq!(GuiTheme::light().table_header_background, 0xf1f5f9);
        assert_eq!(GuiTheme::light().table_active_border, 0x60a5fa);
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
    fn table_selected_cell_border_uses_selection_background_to_avoid_gaps() {
        let theme = GuiTheme::light();

        assert_eq!(table_cell_border_color(theme, false), theme.border);
        assert_eq!(
            table_cell_border_color(theme, true),
            table_selected_cell_background(theme)
        );
    }

    #[test]
    fn table_cell_line_height_is_stable_for_empty_active_cells() {
        assert_eq!(table_cell_text_size(), px(14.0));
        assert_eq!(table_cell_line_height(), px(17.5));
    }

    #[test]
    fn table_active_cell_border_is_overlay_style_from_theme() {
        let theme = GuiTheme::light();

        assert_eq!(TABLE_ACTIVE_CELL_BORDER_WIDTH_PX, 2.0);
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
}
