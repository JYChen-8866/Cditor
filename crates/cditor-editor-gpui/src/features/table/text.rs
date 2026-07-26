use std::ops::Range;

use gpui::{AnyElement, Entity, FocusHandle, FontWeight, IntoElement};

use crate::editor_view::CditorV2View;
use crate::text::input::RichTextLayoutSpans;
use crate::text::{RichTextElement, RichTextLayoutInput, RichTextTypography, TextLayoutSurfaceId};
use crate::theme::GuiTheme;
use cditor_core::edit::TextAffinity;
use cditor_core::ids::BlockId;
#[cfg(test)]
use cditor_core::rich_text::InlineSpan;
use cditor_core::rich_text::{RichBlockKind, TableCellAlign, TextAlign};
use cditor_runtime::{TableCellPosition, TableCellSpansSnapshot};

pub(super) struct TableCellTextElement {
    block_id: BlockId,
    content_version: u64,
    layout_version: u64,
    position: TableCellPosition,
    spans: TableCellSpansSnapshot,
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
        spans: TableCellSpansSnapshot,
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
            .with_typography(table_cell_typography(self.header))
            .with_table_cell_input_handler(self.view, self.focus, self.active, self.position)
            .render()
    }
}

fn table_cell_layout_input(
    block_id: BlockId,
    content_version: u64,
    layout_version: u64,
    position: TableCellPosition,
    spans: impl Into<RichTextLayoutSpans>,
    align: TableCellAlign,
) -> RichTextLayoutInput {
    RichTextLayoutInput {
        block_id,
        surface_id: table_cell_surface_id(block_id, position),
        content_version,
        layout_version,
        kind: RichBlockKind::Paragraph,
        text_align: core_text_align(align),
        spans: spans.into(),
        width_px: 0.0,
        theme_version: 1,
        font_version: 1,
    }
}

fn table_cell_surface_id(block_id: BlockId, position: TableCellPosition) -> TextLayoutSurfaceId {
    crate::surfaces::table_cell::layout_surface_id(block_id, position)
}

pub(crate) fn core_text_align(align: TableCellAlign) -> TextAlign {
    match align {
        TableCellAlign::Left => TextAlign::Start,
        TableCellAlign::Center => TextAlign::Center,
        TableCellAlign::Right => TextAlign::End,
    }
}

pub(crate) fn table_cell_typography(header: bool) -> RichTextTypography {
    let style = if header {
        cditor_config::APP_CONFIG
            .document
            .typography
            .styles
            .table_header
    } else {
        cditor_config::APP_CONFIG
            .document
            .typography
            .styles
            .table_cell
    };
    RichTextTypography {
        font_size_px: Some(style.size_px),
        line_height_px: Some(style.line_height_px),
        font_weight: Some(FontWeight(style.weight as f32)),
    }
}

#[cfg(test)]
fn table_cell_font_weight(header: bool) -> FontWeight {
    let style = if header {
        cditor_config::APP_CONFIG
            .document
            .typography
            .styles
            .table_header
    } else {
        cditor_config::APP_CONFIG
            .document
            .typography
            .styles
            .table_cell
    };
    FontWeight(style.weight as f32)
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
    fn table_cell_alignment_maps_to_text_layout_input_alignment() {
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
