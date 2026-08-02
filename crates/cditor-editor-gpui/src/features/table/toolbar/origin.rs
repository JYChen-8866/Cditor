use crate::block::chrome::{BLOCK_SHELL_BORDER_WIDTH_PX, BlockChromeStyle};
use crate::document::{DocumentBlockGeometry, DocumentLayoutMetrics};
use crate::theme::GuiTheme;
use cditor_runtime::ViewBlockSnapshot;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TableToolbarEditorOrigin {
    pub x_px: f32,
    pub y_px: f32,
}

pub(crate) fn table_projected_viewport_width_px(
    block: &ViewBlockSnapshot,
    theme: GuiTheme,
    document_layout: DocumentLayoutMetrics,
) -> f32 {
    let chrome = BlockChromeStyle::from_snapshot(block, theme);
    let horizontal = chrome.horizontal_geometry();
    let block_geometry = DocumentBlockGeometry::for_block(block, document_layout);
    (block_geometry.shell_width_px - horizontal.text_left_px - horizontal.content_right_inset_px)
        .max(1.0)
}

pub(crate) fn table_toolbar_editor_origin(
    block: &ViewBlockSnapshot,
    block_top_px: f32,
    theme: GuiTheme,
    document_layout: DocumentLayoutMetrics,
) -> TableToolbarEditorOrigin {
    table_content_editor_origin(block, block_top_px, theme, document_layout)
}

pub(crate) fn table_content_editor_origin(
    block: &ViewBlockSnapshot,
    block_top_px: f32,
    theme: GuiTheme,
    document_layout: DocumentLayoutMetrics,
) -> TableToolbarEditorOrigin {
    let chrome = BlockChromeStyle::from_snapshot(block, theme);
    let horizontal = chrome.horizontal_geometry();
    let block_geometry = DocumentBlockGeometry::for_block(block, document_layout);
    TableToolbarEditorOrigin {
        x_px: block_geometry.shell_left_px + horizontal.text_left_px,
        y_px: block_top_px
            + BLOCK_SHELL_BORDER_WIDTH_PX
            + chrome.outer_padding_top_px
            + chrome.content_border_width_px
            + chrome.content_padding_y_px,
    }
}
