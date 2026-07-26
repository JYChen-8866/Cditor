use crate::block::chrome::BlockChromeStyle;
use crate::document::{DocumentBlockGeometry, DocumentLayoutMetrics};
use cditor_core::block::{BlockChromeSnapshot, BlockListInfo, BlockPrefixSnapshot};
use cditor_core::layout::BlockLayoutMeta;
use cditor_core::rich_text::{
    BlockAttrs, BlockPayloadView, RichBlockKind, TableCellAlign, TablePayload,
};
use cditor_runtime::ViewBlockSnapshot;

use super::*;

#[test]
fn destructive_table_actions_use_danger_text() {
    let theme = GuiTheme::light();
    assert_eq!(
        table_menu_action_color(TableMenuAction::DeleteRow, theme),
        theme.danger
    );
    assert_eq!(
        table_menu_action_color(TableMenuAction::InsertRowAbove, theme),
        theme.text
    );
}

fn table_block_with_depth(depth: u8) -> ViewBlockSnapshot {
    ViewBlockSnapshot {
        block_id: 7,
        visible_index: 0,
        depth: depth as u16,
        chrome: BlockChromeSnapshot {
            list_info: BlockListInfo::with_depth(depth.into()),
            prefix: BlockPrefixSnapshot::None,
            has_children: false,
            collapsed: false,
        },
        kind: RichBlockKind::Table,
        attrs: BlockAttrs::default(),
        payload: BlockPayloadView::Placeholder {
            estimated_height: 96.0,
        },
        layout: BlockLayoutMeta::new(1, 96.0),
        selected: false,
        selection_range: None,
        selection_overlay: false,
        focused: false,
        caret_offset: None,
        caret_affinity: None,
        marked_range: None,
        table_view: Some(table_view_with_two_by_two_cells()),
        focused_table_cell: None,
        focused_table_cell_offset: None,
        pinned: false,
        placeholder: false,
    }
}

fn depth_two_table_editor_x_px() -> f32 {
    BlockChromeStyle::from_snapshot(&table_block_with_depth(2), GuiTheme::light())
        .horizontal_geometry()
        .text_left_px
}

#[test]
fn menu_anchor_left_matches_selected_gutter_left_edge() {
    let table_view = table_view_with_two_by_two_cells();

    assert_eq!(
        table_menu_anchor(
            TableAxisSelection::new(7, TableAxis::Column, 1),
            &table_view,
        ),
        TableMenuAnchor {
            left: 169.0,
            top: TABLE_AXIS_COLUMN_HANDLE_TOP_PX,
            height: TABLE_AXIS_HANDLE_SIZE_PX,
        }
    );
    assert_eq!(
        table_menu_anchor(TableAxisSelection::new(7, TableAxis::Row, 1), &table_view),
        TableMenuAnchor {
            left: TABLE_AXIS_ROW_HANDLE_LEFT_PX,
            top: 43.0,
            height: TABLE_AXIS_SELECTED_HANDLE_LONG_EDGE_PX,
        }
    );

    let mut scrolled = table_view;
    scrolled.horizontal_scroll_offset_px = -80.0;
    assert_eq!(
        table_menu_anchor(TableAxisSelection::new(7, TableAxis::Column, 1), &scrolled).left,
        89.0
    );
}

#[test]
fn table_color_submenu_has_a_visible_gap_and_stays_inside_menu_container() {
    let viewport = MenuViewportBounds {
        left: 120.0,
        top: 80.0,
        right: 1_020.0,
        bottom: 680.0,
    };
    let submenu = secondary_menu_geometry(
        240.0,
        140.0,
        TABLE_MENU_WIDTH_PX,
        180.0,
        SECONDARY_MENU_WIDTH_PX,
        table_background_submenu_height(),
        viewport,
        TABLE_COLOR_SUBMENU_GAP_PX,
        TABLE_MENU_VIEWPORT_MARGIN_PX,
    );

    assert_eq!(
        submenu.left - (240.0 + TABLE_MENU_WIDTH_PX),
        TABLE_COLOR_SUBMENU_GAP_PX
    );
    assert!(submenu.left >= viewport.left + TABLE_MENU_VIEWPORT_MARGIN_PX);
    assert!(
        submenu.left + SECONDARY_MENU_WIDTH_PX <= viewport.right - TABLE_MENU_VIEWPORT_MARGIN_PX
    );
    assert_eq!(
        table_background_submenu_height(),
        TABLE_COLOR_SUBMENU_PADDING_PX * 2.0
            + TABLE_MENU_ROW_HEIGHT_PX * TableBackgroundColor::ALL.len() as f32
    );
}

#[test]
fn table_toolbar_editor_origin_tracks_block_shell_projection() {
    let origin = table_toolbar_editor_origin(
        &table_block_with_depth(2),
        120.0,
        GuiTheme::light(),
        DocumentLayoutMetrics::default(),
    );

    assert_eq!(
        origin,
        TableToolbarEditorOrigin {
            x_px: DocumentBlockGeometry::for_block(
                &table_block_with_depth(2),
                DocumentLayoutMetrics::default(),
            )
            .shell_left_px
                + depth_two_table_editor_x_px(),
            y_px: 129.0,
        }
    );
}

#[test]
fn table_content_editor_origin_matches_toolbar_origin() {
    let origin = table_content_editor_origin(
        &table_block_with_depth(2),
        120.0,
        GuiTheme::light(),
        DocumentLayoutMetrics::default(),
    );

    assert_eq!(
        origin,
        TableToolbarEditorOrigin {
            x_px: DocumentBlockGeometry::for_block(
                &table_block_with_depth(2),
                DocumentLayoutMetrics::default(),
            )
            .shell_left_px
                + depth_two_table_editor_x_px(),
            y_px: 129.0,
        }
    );
}

#[test]
fn table_projected_viewport_matches_the_current_table_track() {
    assert_eq!(
        table_projected_viewport_width_px(
            &table_block_with_depth(0),
            GuiTheme::light(),
            DocumentLayoutMetrics::default(),
        ),
        240.0
    );
}

fn table_view_with_two_by_two_cells() -> TableViewState {
    TableViewState {
        table: TablePayload::default().into(),
        row_count: 2,
        col_count: 2,
        width_px: 240.0,
        height_px: 72.0,
        column_widths_px: vec![120.0, 120.0],
        row_heights_px: vec![36.0, 36.0],
        horizontal_scroll_offset_px: 0.0,
        visible_cells: vec![cditor_runtime::TableVisibleCell {
            position: cditor_runtime::TableCellPosition { row: 1, col: 1 },
            row_span: 1,
            col_span: 1,
            x_px: 120.0,
            y_px: 36.0,
            width_px: 120.0,
            height_px: 36.0,
            header: false,
            align: TableCellAlign::Left,
            background_color: None,
            spans: Default::default(),
        }],
        focused_cell: None,
        focused_cell_offset: None,
        focused_cell_affinity: None,
        focused_cell_selection_range: None,
    }
}
