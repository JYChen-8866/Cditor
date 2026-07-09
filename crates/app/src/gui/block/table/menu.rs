use cditor_core::rich_text::TableCellAlign;

use super::selection::{TableAxis, TableAxisSelection, TableCellRangeSelection};

pub(crate) const TABLE_MENU_WIDTH_PX: f32 = 264.0;
pub(crate) const TABLE_MENU_ROW_HEIGHT_PX: f32 = 32.0;
pub(crate) const TABLE_MENU_MAX_VISIBLE_ROWS: usize = 8;
pub(crate) const TABLE_MENU_VIEWPORT_MARGIN_PX: f32 = 8.0;
pub(crate) const TABLE_MENU_GAP_PX: f32 = 8.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TableMenuAction {
    InsertRowAbove,
    InsertRowBelow,
    DeleteRow,
    DuplicateRow,
    InsertColumnLeft,
    InsertColumnRight,
    DeleteColumn,
    DuplicateColumn,
    Align(TableCellAlign),
    MergeCells,
    SplitCell,
    BackgroundColor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TableMenuItem {
    pub action: TableMenuAction,
    pub label: &'static str,
    pub keywords: &'static [&'static str],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TableMenuPosition {
    pub x: f32,
    pub y: f32,
    pub height: f32,
    pub placed_above: bool,
}

pub(crate) fn table_axis_menu_items(selection: TableAxisSelection) -> Vec<TableMenuItem> {
    let mut items = match selection.axis {
        TableAxis::Row => vec![
            table_menu_item(
                TableMenuAction::InsertRowAbove,
                "Insert above",
                &["row", "above"],
            ),
            table_menu_item(
                TableMenuAction::InsertRowBelow,
                "Insert below",
                &["row", "below"],
            ),
            table_menu_item(TableMenuAction::DeleteRow, "Delete row", &["row", "remove"]),
            table_menu_item(
                TableMenuAction::DuplicateRow,
                "Duplicate row",
                &["row", "copy"],
            ),
        ],
        TableAxis::Column => vec![
            table_menu_item(
                TableMenuAction::InsertColumnLeft,
                "Insert left",
                &["column", "left"],
            ),
            table_menu_item(
                TableMenuAction::InsertColumnRight,
                "Insert right",
                &["column", "right"],
            ),
            table_menu_item(
                TableMenuAction::DeleteColumn,
                "Delete column",
                &["column", "remove"],
            ),
            table_menu_item(
                TableMenuAction::DuplicateColumn,
                "Duplicate column",
                &["column", "copy"],
            ),
        ],
    };
    items.extend([
        table_menu_item(
            TableMenuAction::Align(TableCellAlign::Left),
            "Align left",
            &["align"],
        ),
        table_menu_item(
            TableMenuAction::Align(TableCellAlign::Center),
            "Align center",
            &["align"],
        ),
        table_menu_item(
            TableMenuAction::Align(TableCellAlign::Right),
            "Align right",
            &["align"],
        ),
        table_menu_item(TableMenuAction::MergeCells, "Merge cells", &["merge"]),
        table_menu_item(TableMenuAction::SplitCell, "Split cell", &["split"]),
        table_menu_item(
            TableMenuAction::BackgroundColor,
            "Background color",
            &["color", "fill"],
        ),
    ]);
    items
}

pub(crate) fn table_range_menu_items(_selection: TableCellRangeSelection) -> Vec<TableMenuItem> {
    vec![
        table_menu_item(
            TableMenuAction::Align(TableCellAlign::Left),
            "Align left",
            &["align"],
        ),
        table_menu_item(
            TableMenuAction::Align(TableCellAlign::Center),
            "Align center",
            &["align"],
        ),
        table_menu_item(
            TableMenuAction::Align(TableCellAlign::Right),
            "Align right",
            &["align"],
        ),
        table_menu_item(TableMenuAction::MergeCells, "Merge cells", &["merge"]),
        table_menu_item(TableMenuAction::SplitCell, "Split cell", &["split"]),
        table_menu_item(
            TableMenuAction::BackgroundColor,
            "Background color",
            &["color", "fill"],
        ),
    ]
}

pub(crate) fn filter_table_menu_items(items: &[TableMenuItem], query: &str) -> Vec<TableMenuItem> {
    let query = query.trim().to_ascii_lowercase();
    if query.is_empty() {
        return items.to_vec();
    }
    items
        .iter()
        .copied()
        .filter(|item| {
            item.label.to_ascii_lowercase().contains(&query)
                || item
                    .keywords
                    .iter()
                    .any(|keyword| keyword.to_ascii_lowercase().contains(&query))
        })
        .collect()
}

pub(crate) fn table_menu_action_enabled(action: TableMenuAction) -> bool {
    let _ = action;
    true
}

pub(crate) fn table_menu_panel_height(item_count: usize) -> f32 {
    let rows = item_count.min(TABLE_MENU_MAX_VISIBLE_ROWS).max(1);
    rows as f32 * TABLE_MENU_ROW_HEIGHT_PX
}

pub(crate) fn table_menu_position(
    anchor_x: f32,
    anchor_y: f32,
    anchor_height: f32,
    item_count: usize,
    viewport_width: f32,
    viewport_height: f32,
) -> TableMenuPosition {
    let height = table_menu_panel_height(item_count);
    let max_x = (viewport_width - TABLE_MENU_WIDTH_PX - TABLE_MENU_VIEWPORT_MARGIN_PX)
        .max(TABLE_MENU_VIEWPORT_MARGIN_PX);
    let x = anchor_x.clamp(TABLE_MENU_VIEWPORT_MARGIN_PX, max_x);
    let below_y = anchor_y + anchor_height + TABLE_MENU_GAP_PX;
    let above_y = anchor_y - height - TABLE_MENU_GAP_PX;
    let should_place_above = below_y + height > viewport_height - TABLE_MENU_VIEWPORT_MARGIN_PX
        && above_y >= TABLE_MENU_VIEWPORT_MARGIN_PX;
    let y = if should_place_above {
        above_y
    } else {
        below_y.clamp(
            TABLE_MENU_VIEWPORT_MARGIN_PX,
            (viewport_height - height - TABLE_MENU_VIEWPORT_MARGIN_PX)
                .max(TABLE_MENU_VIEWPORT_MARGIN_PX),
        )
    };
    TableMenuPosition {
        x,
        y,
        height,
        placed_above: should_place_above,
    }
}

fn table_menu_item(
    action: TableMenuAction,
    label: &'static str,
    keywords: &'static [&'static str],
) -> TableMenuItem {
    TableMenuItem {
        action,
        label,
        keywords,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_axis_menu_items_include_axis_specific_and_shared_actions() {
        let row_items = table_axis_menu_items(TableAxisSelection::new(7, TableAxis::Row, 1));
        let column_items = table_axis_menu_items(TableAxisSelection::new(7, TableAxis::Column, 1));

        assert!(
            row_items
                .iter()
                .any(|item| item.action == TableMenuAction::InsertRowAbove)
        );
        assert!(
            row_items
                .iter()
                .any(|item| item.action == TableMenuAction::DeleteRow)
        );
        assert!(
            column_items
                .iter()
                .any(|item| item.action == TableMenuAction::InsertColumnLeft)
        );
        assert!(
            column_items
                .iter()
                .any(|item| item.action == TableMenuAction::DeleteColumn)
        );
        assert!(
            row_items
                .iter()
                .any(|item| item.action == TableMenuAction::Align(TableCellAlign::Center))
        );
        assert!(
            column_items
                .iter()
                .any(|item| item.action == TableMenuAction::MergeCells)
        );
    }

    #[test]
    fn table_menu_filter_matches_labels_and_keywords() {
        let items = table_axis_menu_items(TableAxisSelection::new(7, TableAxis::Column, 1));

        assert_eq!(filter_table_menu_items(&items, "right").len(), 2);
        assert!(
            filter_table_menu_items(&items, "fill")
                .iter()
                .any(|item| item.action == TableMenuAction::BackgroundColor)
        );
        assert!(filter_table_menu_items(&items, "zzz").is_empty());
    }

    #[test]
    fn table_menu_height_is_scroll_limited() {
        assert_eq!(table_menu_panel_height(0), TABLE_MENU_ROW_HEIGHT_PX);
        assert_eq!(table_menu_panel_height(3), TABLE_MENU_ROW_HEIGHT_PX * 3.0);
        assert_eq!(
            table_menu_panel_height(30),
            TABLE_MENU_ROW_HEIGHT_PX * TABLE_MENU_MAX_VISIBLE_ROWS as f32
        );
    }

    #[test]
    fn table_menu_position_clamps_and_flips_to_viewport() {
        let clamped = table_menu_position(900.0, 100.0, 16.0, 3, 960.0, 640.0);
        assert_eq!(clamped.x, 688.0);
        assert!(!clamped.placed_above);

        let flipped = table_menu_position(120.0, 610.0, 16.0, 10, 960.0, 640.0);
        assert!(flipped.placed_above);
        assert!(flipped.y < 610.0);
    }
}
