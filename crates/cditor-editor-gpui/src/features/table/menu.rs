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
pub(crate) const TABLE_MENU_MAX_ROW_INSERT_COUNT: usize = 99;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TableRowInsertDirection {
    Above,
    Below,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TableMenuUiState {
    pub query: String,
    pub caret_offset: usize,
    pub marked_range: Option<Range<usize>>,
    pub color_submenu_open: bool,
    row_insert_above_count: usize,
    row_insert_below_count: usize,
}

impl Default for TableMenuUiState {
    fn default() -> Self {
        Self {
            query: String::new(),
            caret_offset: 0,
            marked_range: None,
            color_submenu_open: false,
            row_insert_above_count: 1,
            row_insert_below_count: 1,
        }
    }
}

impl TableMenuUiState {
    pub(crate) const fn row_insert_count(&self, direction: TableRowInsertDirection) -> usize {
        match direction {
            TableRowInsertDirection::Above => self.row_insert_above_count,
            TableRowInsertDirection::Below => self.row_insert_below_count,
        }
    }

    pub(crate) fn decrement_row_insert_count(
        &mut self,
        direction: TableRowInsertDirection,
    ) -> bool {
        self.set_row_insert_count(
            direction,
            self.row_insert_count(direction).saturating_sub(1),
        )
    }

    pub(crate) fn increment_row_insert_count(
        &mut self,
        direction: TableRowInsertDirection,
    ) -> bool {
        self.set_row_insert_count(
            direction,
            self.row_insert_count(direction).saturating_add(1),
        )
    }

    pub(crate) fn set_row_insert_count(
        &mut self,
        direction: TableRowInsertDirection,
        count: usize,
    ) -> bool {
        let count = count.clamp(1, TABLE_MENU_MAX_ROW_INSERT_COUNT);
        let current = match direction {
            TableRowInsertDirection::Above => &mut self.row_insert_above_count,
            TableRowInsertDirection::Below => &mut self.row_insert_below_count,
        };
        if count == *current {
            return false;
        }
        *current = count;
        true
    }

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
    let rows = item_count.clamp(1, TABLE_MENU_MAX_VISIBLE_ROWS);
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
#[path = "menu/tests.rs"]
mod tests;
