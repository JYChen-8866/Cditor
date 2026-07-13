use std::ops::Range;

use cditor_core::rich_text::TableCellMerge;
use cditor_runtime::TableViewState;

use super::selection::{TableAxis, TableAxisSelection};

pub(crate) const TABLE_MENU_WIDTH_PX: f32 = 264.0;
pub(crate) const TABLE_MENU_ROW_HEIGHT_PX: f32 = 29.0;
pub(crate) const TABLE_MENU_SEARCH_HEIGHT_PX: f32 = 30.0;
pub(crate) const TABLE_MENU_SEARCH_FONT_SIZE_PX: f32 = 13.0;
pub(crate) const TABLE_MENU_PADDING_PX: f32 = 6.0;
pub(crate) const TABLE_MENU_SEARCH_GAP_PX: f32 = 6.0;
pub(crate) const TABLE_MENU_MAX_VISIBLE_ROWS: usize = 10;
pub(crate) const TABLE_MENU_VIEWPORT_MARGIN_PX: f32 = 8.0;
pub(crate) const TABLE_MENU_GAP_PX: f32 = 8.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TableMenuUiState {
    pub query: String,
    pub caret_offset: usize,
    pub marked_range: Option<Range<usize>>,
    pub color_submenu_open: bool,
}

impl Default for TableMenuUiState {
    fn default() -> Self {
        Self {
            query: String::new(),
            caret_offset: 0,
            marked_range: None,
            color_submenu_open: false,
        }
    }
}

impl TableMenuUiState {
    pub(crate) fn input_replacement_range(&self) -> Range<usize> {
        self.marked_range
            .clone()
            .unwrap_or(self.caret_offset..self.caret_offset)
    }

    pub(crate) fn replace_range(&mut self, range: Range<usize>, text: &str) {
        let range = safe_query_range(&self.query, range);
        self.query.replace_range(range.clone(), text);
        self.caret_offset = range.start + text.len();
        self.marked_range = None;
        self.color_submenu_open = false;
    }

    pub(crate) fn replace_and_mark_range(
        &mut self,
        range: Range<usize>,
        text: &str,
        selected_range: Option<Range<usize>>,
    ) {
        let range = safe_query_range(&self.query, range);
        self.query.replace_range(range.clone(), text);
        self.marked_range = Some(range.start..range.start + text.len());
        self.caret_offset = selected_range
            .map(|selection| range.start + selection.end.min(text.len()))
            .unwrap_or(range.start + text.len());
        self.color_submenu_open = false;
    }

    pub(crate) fn unmark(&mut self) {
        self.marked_range = None;
    }

    pub(crate) fn move_left(&mut self) {
        if let Some(previous) = previous_query_char_boundary(&self.query, self.caret_offset) {
            self.caret_offset = previous;
        }
        self.marked_range = None;
    }

    pub(crate) fn move_right(&mut self) {
        self.caret_offset = next_query_char_boundary(&self.query, self.caret_offset);
        self.marked_range = None;
    }

    pub(crate) fn move_to_start(&mut self) {
        self.caret_offset = 0;
        self.marked_range = None;
    }

    pub(crate) fn move_to_end(&mut self) {
        self.caret_offset = self.query.len();
        self.marked_range = None;
    }

    pub(crate) fn delete_backward(&mut self) {
        if let Some(previous) = previous_query_char_boundary(&self.query, self.caret_offset) {
            self.query.replace_range(previous..self.caret_offset, "");
            self.caret_offset = previous;
        }
        self.marked_range = None;
        self.color_submenu_open = false;
    }

    pub(crate) fn delete_forward(&mut self) {
        let next = next_query_char_boundary(&self.query, self.caret_offset);
        if next > self.caret_offset {
            self.query.replace_range(self.caret_offset..next, "");
        }
        self.marked_range = None;
        self.color_submenu_open = false;
    }
}

fn safe_query_range(text: &str, range: Range<usize>) -> Range<usize> {
    let start = clamp_query_char_boundary(text, range.start.min(text.len()));
    let end = clamp_query_char_boundary(text, range.end.min(text.len())).max(start);
    start..end
}

fn clamp_query_char_boundary(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while offset > 0 && !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

fn previous_query_char_boundary(text: &str, offset: usize) -> Option<usize> {
    let offset = clamp_query_char_boundary(text, offset);
    text[..offset]
        .char_indices()
        .next_back()
        .map(|(index, _)| index)
}

fn next_query_char_boundary(text: &str, offset: usize) -> usize {
    let offset = clamp_query_char_boundary(text, offset);
    text[offset..]
        .char_indices()
        .nth(1)
        .map(|(index, _)| offset + index)
        .unwrap_or(text.len())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TableBackgroundColor {
    Default,
    Gray,
    Brown,
    Orange,
    Yellow,
    Green,
    Blue,
    Purple,
    Pink,
    Red,
}

impl TableBackgroundColor {
    pub(crate) const ALL: [Self; 10] = [
        Self::Default,
        Self::Gray,
        Self::Brown,
        Self::Orange,
        Self::Yellow,
        Self::Green,
        Self::Blue,
        Self::Purple,
        Self::Pink,
        Self::Red,
    ];

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Default => "默认背景",
            Self::Gray => "灰色背景",
            Self::Brown => "棕色背景",
            Self::Orange => "橙色背景",
            Self::Yellow => "黄色背景",
            Self::Green => "绿色背景",
            Self::Blue => "蓝色背景",
            Self::Purple => "紫色背景",
            Self::Pink => "粉色背景",
            Self::Red => "红色背景",
        }
    }

    pub(crate) const fn value(self) -> Option<&'static str> {
        match self {
            Self::Default => None,
            Self::Gray => Some("#f1f1ef"),
            Self::Brown => Some("#f4eeee"),
            Self::Orange => Some("#fbecdd"),
            Self::Yellow => Some("#fbf3db"),
            Self::Green => Some("#edf3ec"),
            Self::Blue => Some("#e7f3f8"),
            Self::Purple => Some("#f4f0f7"),
            Self::Pink => Some("#f9eef3"),
            Self::Red => Some("#fdebec"),
        }
    }

    pub(crate) const fn swatch(self, panel: u32) -> u32 {
        match self {
            Self::Default => panel,
            Self::Gray => 0xf1f1ef,
            Self::Brown => 0xf4eeee,
            Self::Orange => 0xfbecdd,
            Self::Yellow => 0xfbf3db,
            Self::Green => 0xedf3ec,
            Self::Blue => 0xe7f3f8,
            Self::Purple => 0xf4f0f7,
            Self::Pink => 0xf9eef3,
            Self::Red => 0xfdebec,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TableMenuAction {
    ToggleHeader,
    BackgroundColor,
    InsertRowAbove,
    InsertRowBelow,
    DeleteRow,
    DuplicateRow,
    InsertColumnLeft,
    InsertColumnRight,
    DeleteColumn,
    DuplicateColumn,
    ClearContents,
}

impl TableMenuAction {
    pub(crate) const fn icon(self) -> &'static str {
        match self {
            Self::ToggleHeader => "▦",
            Self::BackgroundColor => "◉",
            Self::InsertRowAbove => "↑",
            Self::InsertRowBelow => "↓",
            Self::InsertColumnLeft => "←",
            Self::InsertColumnRight => "→",
            Self::DuplicateRow | Self::DuplicateColumn => "▢",
            Self::ClearContents => "⊗",
            Self::DeleteRow | Self::DeleteColumn => "⌫",
        }
    }

    pub(crate) const fn shortcut(self) -> Option<&'static str> {
        match self {
            Self::DuplicateRow | Self::DuplicateColumn => Some("⌘D"),
            _ => None,
        }
    }
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
    match selection.axis {
        TableAxis::Row => vec![
            table_menu_item(
                TableMenuAction::ToggleHeader,
                "标题行",
                &["header", "title"],
            ),
            table_menu_item(
                TableMenuAction::BackgroundColor,
                "颜色",
                &["color", "background", "fill"],
            ),
            table_menu_item(
                TableMenuAction::InsertRowAbove,
                "在上方插入",
                &["row", "above", "insert"],
            ),
            table_menu_item(
                TableMenuAction::InsertRowBelow,
                "在下方插入",
                &["row", "below", "insert"],
            ),
            table_menu_item(
                TableMenuAction::DuplicateRow,
                "创建副本",
                &["row", "copy", "duplicate"],
            ),
            table_menu_item(
                TableMenuAction::ClearContents,
                "清除内容",
                &["clear", "empty", "content"],
            ),
            table_menu_item(
                TableMenuAction::DeleteRow,
                "删除",
                &["row", "remove", "delete"],
            ),
        ],
        TableAxis::Column => vec![
            table_menu_item(
                TableMenuAction::ToggleHeader,
                "标题列",
                &["header", "title"],
            ),
            table_menu_item(
                TableMenuAction::BackgroundColor,
                "颜色",
                &["color", "background", "fill"],
            ),
            table_menu_item(
                TableMenuAction::InsertColumnLeft,
                "在左侧插入",
                &["column", "left", "insert"],
            ),
            table_menu_item(
                TableMenuAction::InsertColumnRight,
                "在右侧插入",
                &["column", "right", "insert"],
            ),
            table_menu_item(
                TableMenuAction::DuplicateColumn,
                "创建副本",
                &["column", "copy", "duplicate"],
            ),
            table_menu_item(
                TableMenuAction::ClearContents,
                "清除内容",
                &["clear", "empty", "content"],
            ),
            table_menu_item(
                TableMenuAction::DeleteColumn,
                "删除",
                &["column", "remove", "delete"],
            ),
        ],
    }
}

pub(crate) fn filter_table_menu_items(items: &[TableMenuItem], query: &str) -> Vec<TableMenuItem> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return items.to_vec();
    }
    items
        .iter()
        .copied()
        .filter(|item| {
            item.label.to_lowercase().contains(&query)
                || item
                    .keywords
                    .iter()
                    .any(|keyword| keyword.to_lowercase().contains(&query))
        })
        .collect()
}

pub(crate) fn table_axis_header_enabled(
    selection: TableAxisSelection,
    table_view: &TableViewState,
) -> bool {
    match selection.axis {
        TableAxis::Row => table_view.table.header_rows > 0,
        TableAxis::Column => table_view.table.header_cols > 0,
    }
}

pub(crate) fn table_menu_action_enabled(
    action: TableMenuAction,
    _selection: TableAxisSelection,
    table_view: &TableViewState,
) -> bool {
    let has_merges = table_view.table.rows.iter().any(|row| {
        row.cells
            .iter()
            .any(|cell| !matches!(cell.merge, TableCellMerge::Unmerged))
    });
    match action {
        TableMenuAction::InsertRowAbove
        | TableMenuAction::InsertRowBelow
        | TableMenuAction::DuplicateRow
        | TableMenuAction::InsertColumnLeft
        | TableMenuAction::InsertColumnRight
        | TableMenuAction::DuplicateColumn => !has_merges,
        TableMenuAction::DeleteRow => !has_merges && table_view.row_count > 1,
        TableMenuAction::DeleteColumn => !has_merges && table_view.col_count > 1,
        TableMenuAction::ToggleHeader
        | TableMenuAction::BackgroundColor
        | TableMenuAction::ClearContents => true,
    }
}

pub(crate) fn table_menu_panel_height(item_count: usize) -> f32 {
    let rows = item_count.min(TABLE_MENU_MAX_VISIBLE_ROWS).max(1);
    TABLE_MENU_PADDING_PX * 2.0
        + TABLE_MENU_SEARCH_HEIGHT_PX
        + TABLE_MENU_SEARCH_GAP_PX
        + rows as f32 * TABLE_MENU_ROW_HEIGHT_PX
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
    let max_x = viewport_width - TABLE_MENU_WIDTH_PX - TABLE_MENU_VIEWPORT_MARGIN_PX;
    // The left edge is deliberately not clamped: row gutters live left of the
    // table surface and the menu must share their exact x coordinate. We only
    // shift left when the menu would overflow the viewport's right edge.
    let x = anchor_x.min(max_x);
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
    use cditor_core::rich_text::{
        TableCellPayload, TableColumnPayload, TablePayload, TableRowPayload,
    };

    #[test]
    fn table_axis_menu_matches_notion_row_and_column_action_order() {
        let row_items = table_axis_menu_items(TableAxisSelection::new(7, TableAxis::Row, 1));
        let column_items = table_axis_menu_items(TableAxisSelection::new(7, TableAxis::Column, 1));

        assert_eq!(row_items.len(), 7);
        assert_eq!(row_items[0].label, "标题行");
        assert_eq!(row_items[1].action, TableMenuAction::BackgroundColor);
        assert_eq!(row_items[6].action, TableMenuAction::DeleteRow);
        assert_eq!(column_items.len(), 7);
        assert_eq!(column_items[0].label, "标题列");
        assert_eq!(column_items[2].action, TableMenuAction::InsertColumnLeft);
        assert_eq!(column_items[6].action, TableMenuAction::DeleteColumn);
    }

    #[test]
    fn table_menu_filter_matches_chinese_labels_and_english_keywords() {
        let items = table_axis_menu_items(TableAxisSelection::new(7, TableAxis::Column, 1));

        assert_eq!(filter_table_menu_items(&items, "右侧").len(), 1);
        assert_eq!(filter_table_menu_items(&items, "insert").len(), 2);
        assert!(
            filter_table_menu_items(&items, "fill")
                .iter()
                .any(|item| item.action == TableMenuAction::BackgroundColor)
        );
        assert!(filter_table_menu_items(&items, "zzz").is_empty());
    }

    #[test]
    fn table_menu_query_edits_unicode_on_character_boundaries() {
        let mut state = TableMenuUiState::default();
        state.replace_range(state.input_replacement_range(), "颜色a");
        assert_eq!(state.caret_offset, "颜色a".len());

        state.move_left();
        state.delete_backward();
        assert_eq!(state.query, "颜a");
        assert_eq!(state.caret_offset, "颜".len());

        state.delete_forward();
        assert_eq!(state.query, "颜");
        state.move_to_end();
        state.delete_backward();
        assert_eq!(state.query, "");
    }

    #[test]
    fn table_menu_query_tracks_ime_marked_range() {
        let mut state = TableMenuUiState::default();
        state.replace_and_mark_range(0..0, "颜", Some("颜".len().."颜".len()));

        assert_eq!(state.query, "颜");
        assert_eq!(state.marked_range, Some(0.."颜".len()));
        assert_eq!(state.input_replacement_range(), 0.."颜".len());

        state.unmark();
        assert_eq!(state.marked_range, None);
    }

    #[test]
    fn table_menu_height_includes_search_and_is_scroll_limited() {
        let fixed =
            TABLE_MENU_PADDING_PX * 2.0 + TABLE_MENU_SEARCH_HEIGHT_PX + TABLE_MENU_SEARCH_GAP_PX;
        assert_eq!(table_menu_panel_height(0), fixed + TABLE_MENU_ROW_HEIGHT_PX);
        assert_eq!(
            table_menu_panel_height(3),
            fixed + TABLE_MENU_ROW_HEIGHT_PX * 3.0
        );
        assert_eq!(
            table_menu_panel_height(30),
            fixed + TABLE_MENU_ROW_HEIGHT_PX * TABLE_MENU_MAX_VISIBLE_ROWS as f32
        );
    }

    #[test]
    fn table_menu_position_keeps_gutter_left_alignment_and_flips_vertically() {
        let aligned = table_menu_position(-28.0, 10.0, 16.0, 3, 960.0, 640.0);
        assert_eq!(aligned.x, -28.0);
        assert!(!aligned.placed_above);

        let right_clamped = table_menu_position(900.0, 100.0, 16.0, 3, 960.0, 640.0);
        assert_eq!(right_clamped.x, 688.0);

        let flipped = table_menu_position(120.0, 610.0, 16.0, 10, 960.0, 640.0);
        assert!(flipped.placed_above);
        assert!(flipped.y < 610.0);
    }

    #[test]
    fn header_toggle_reflects_table_axis_metadata() {
        let mut table_view = basic_table_view();
        table_view.table.header_rows = 1;
        table_view.table.header_cols = 0;

        assert!(table_axis_header_enabled(
            TableAxisSelection::new(7, TableAxis::Row, 1),
            &table_view,
        ));
        assert!(!table_axis_header_enabled(
            TableAxisSelection::new(7, TableAxis::Column, 1),
            &table_view,
        ));
    }

    #[test]
    fn merged_table_disables_only_unsafe_structure_actions() {
        let mut table_view = basic_table_view();
        table_view
            .table
            .merge_cells(cditor_core::rich_text::TableRange::normalized(0, 0, 1, 1))
            .unwrap();
        let selection = TableAxisSelection::new(7, TableAxis::Row, 0);

        assert!(!table_menu_action_enabled(
            TableMenuAction::InsertRowBelow,
            selection,
            &table_view
        ));
        assert!(!table_menu_action_enabled(
            TableMenuAction::DeleteColumn,
            selection,
            &table_view
        ));
        assert!(table_menu_action_enabled(
            TableMenuAction::ClearContents,
            selection,
            &table_view
        ));
        assert!(table_menu_action_enabled(
            TableMenuAction::ToggleHeader,
            selection,
            &table_view
        ));
    }

    fn basic_table_view() -> TableViewState {
        let table = TablePayload {
            rows: vec![
                TableRowPayload {
                    cells: vec![TableCellPayload::plain("A"), TableCellPayload::plain("B")],
                    height: Default::default(),
                },
                TableRowPayload {
                    cells: vec![TableCellPayload::plain("C"), TableCellPayload::plain("D")],
                    height: Default::default(),
                },
            ],
            columns: vec![TableColumnPayload::default(), TableColumnPayload::default()],
            ..TablePayload::default()
        };
        TableViewState {
            table,
            row_count: 2,
            col_count: 2,
            width_px: 240.0,
            height_px: 72.0,
            column_widths_px: vec![120.0, 120.0],
            row_heights_px: vec![36.0, 36.0],
            horizontal_scroll_offset_px: 0.0,
            visible_cells: Vec::new(),
            focused_cell: None,
            focused_cell_offset: None,
            focused_cell_selection_range: None,
        }
    }
}
