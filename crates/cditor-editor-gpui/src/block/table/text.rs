use std::ops::Range;

use gpui::{AnyElement, Entity, FocusHandle, FontWeight, IntoElement};

use crate::editor_view::CditorV2View;
use crate::text::{RichTextElement, RichTextLayoutInput, RichTextTypography, TextLayoutSurfaceId};
use crate::theme::GuiTheme;
use cditor_core::edit::TextAffinity;
use cditor_core::ids::BlockId;
use cditor_core::rich_text::{InlineSpan, RichBlockKind, TableCellAlign, TextAlign};
use cditor_runtime::TableCellPosition;

use super::style::{table_cell_line_height, table_cell_text_size};

pub(super) struct TableCellTextElement {
    block_id: BlockId,
    content_version: u64,
    layout_version: u64,
    position: TableCellPosition,
    spans: Vec<InlineSpan>,
    active: bool,
    caret_offset: Option<usize>,
    caret_affinity: TextAffinity,
    selection_range: Option<Range<usize>>,
    marked_range: Option<Range<usize>>,
    header: bool,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
    focus: FocusHandle,
    align: TableCellAlign,
}

impl TableCellTextElement {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        block_id: BlockId,
        content_version: u64,
        layout_version: u64,
        position: TableCellPosition,
        spans: Vec<InlineSpan>,
        active: bool,
        caret_offset: Option<usize>,
        caret_affinity: TextAffinity,
        selection_range: Option<Range<usize>>,
        marked_range: Option<Range<usize>>,
        header: bool,
        theme: GuiTheme,
        view: Entity<CditorV2View>,
        focus: FocusHandle,
        align: TableCellAlign,
    ) -> Self {
        Self {
            block_id,
            content_version,
            layout_version,
            position,
            spans,
            active,
            caret_offset,
            caret_affinity,
            selection_range,
            marked_range,
            header,
            theme,
            view,
            focus,
            align,
        }
    }
}

impl IntoElement for TableCellTextElement {
    type Element = AnyElement;

    fn into_element(self) -> Self::Element {
        let input = table_cell_layout_input(
            self.block_id,
            self.content_version,
            self.layout_version,
            self.position,
            self.spans,
            self.align,
        );
        RichTextElement::new(input, self.theme)
            .with_caret(self.caret_offset)
            .with_caret_affinity(self.caret_affinity)
            .with_selection_range(self.selection_range)
            .with_marked_range(self.marked_range)
            .with_typography(RichTextTypography {
                font_size_px: Some(f32::from(table_cell_text_size())),
                line_height_px: Some(f32::from(table_cell_line_height())),
                font_weight: Some(table_cell_font_weight(self.header)),
            })
            .with_placeholder(self.active.then_some("请输入..."))
            .with_table_cell_input_handler(self.view, self.focus, self.active, self.position)
            .render()
    }
}

fn table_cell_layout_input(
    block_id: BlockId,
    content_version: u64,
    layout_version: u64,
    position: TableCellPosition,
    spans: Vec<InlineSpan>,
    align: TableCellAlign,
) -> RichTextLayoutInput {
    RichTextLayoutInput {
        block_id,
        surface_id: table_cell_surface_id(block_id, position),
        content_version,
        layout_version,
        kind: RichBlockKind::Paragraph,
        text_align: core_text_align(align),
        spans,
        width_px: 0.0,
        theme_version: 1,
        font_version: 1,
    }
}

fn table_cell_surface_id(block_id: BlockId, position: TableCellPosition) -> TextLayoutSurfaceId {
    crate::surfaces::table_cell::layout_surface_id(block_id, position)
}

fn core_text_align(align: TableCellAlign) -> TextAlign {
    match align {
        TableCellAlign::Left => TextAlign::Start,
        TableCellAlign::Center => TextAlign::Center,
        TableCellAlign::Right => TextAlign::End,
    }
}

fn table_cell_font_weight(header: bool) -> FontWeight {
    if header {
        FontWeight::MEDIUM
    } else {
        FontWeight::NORMAL
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_cell_layout_identity_uses_owner_layout_version() {
        let position = TableCellPosition { row: 2, col: 3 };
        let input = table_cell_layout_input(
            10,
            17,
            29,
            position,
            vec![InlineSpan::plain("cell")],
            TableCellAlign::Center,
        );

        assert_eq!(input.content_version, 17);
        assert_eq!(input.layout_version, 29);
        assert_eq!(input.surface_id, table_cell_surface_id(10, position));
    }

    #[test]
    fn table_cell_alignment_maps_to_parley_input_alignment() {
        assert_eq!(core_text_align(TableCellAlign::Left), TextAlign::Start);
        assert_eq!(core_text_align(TableCellAlign::Center), TextAlign::Center);
        assert_eq!(core_text_align(TableCellAlign::Right), TextAlign::End);
    }

    #[test]
    fn table_header_uses_notion_medium_weight() {
        assert_eq!(table_cell_font_weight(true), FontWeight::MEDIUM);
        assert_eq!(table_cell_font_weight(false), FontWeight::NORMAL);
    }

    #[test]
    fn table_cell_layout_surface_keeps_row_and_column_identity() {
        assert_eq!(
            table_cell_surface_id(7, TableCellPosition { row: 2, col: 3 }),
            TextLayoutSurfaceId::TableCell {
                block_id: 7,
                row: 2,
                column: 3,
            }
        );
    }
}
