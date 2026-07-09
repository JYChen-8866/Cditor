use std::ops::Range;

use crate::edit::TextPosition;
use crate::layout::{HeightConfidence, HeightEstimate};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlockViewport {
    pub scroll_top: f64,
    pub height: f64,
    pub width: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockInnerAnchor {
    TextOffset(usize),
    CodeLine {
        line: usize,
        column: usize,
    },
    TableCell {
        row: usize,
        col: usize,
        offset: usize,
    },
    CanvasPoint {
        x: i64,
        y: i64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockInnerSelection {
    TextRange(Range<usize>),
    CodeLines {
        start_line: usize,
        end_line: usize,
    },
    TableCells {
        rows: Range<usize>,
        cols: Range<usize>,
    },
    CanvasRegion {
        x: i64,
        y: i64,
        width: i64,
        height: i64,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlockFragment {
    pub range: Range<usize>,
    pub y_range: Range<f64>,
    pub kind: BlockFragmentKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockFragmentKind {
    CodeLines,
    TableRows,
    CanvasTile,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockHitTestResult {
    Text(TextPosition),
    Code {
        line: usize,
        column: usize,
    },
    TableCell {
        row: usize,
        col: usize,
        offset: usize,
    },
    Canvas {
        x: i64,
        y: i64,
    },
    None,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockInnerOperation {
    InsertCodeLines { index: usize, count: usize },
    DeleteCodeLines { range: Range<usize> },
    InsertTableRows { index: usize, count: usize },
    DeleteTableRows { range: Range<usize> },
    Select(BlockInnerSelection),
    SetAnchor(BlockInnerAnchor),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockInnerChange {
    pub affected_range: Range<usize>,
    pub height_changed: bool,
    pub outer_height_corrections: usize,
    pub selection: Option<BlockInnerSelection>,
    pub anchor: Option<BlockInnerAnchor>,
}

pub trait BlockEditorModel {
    fn apply_inner_op(&mut self, op: BlockInnerOperation) -> BlockInnerChange;
    fn estimate_height(&self, width: f64) -> HeightEstimate;
    fn visible_fragments(&self, viewport: BlockViewport) -> Vec<BlockFragment>;
    fn hit_test(&self, point: Point) -> BlockHitTestResult;
    fn selection(&self) -> Option<&BlockInnerSelection>;
    fn anchor(&self) -> Option<&BlockInnerAnchor>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WheelHandling {
    DocumentScroll,
    BlockInternalScroll,
    ConditionalTransferToDocument,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComplexBlockInteraction {
    Normal,
    SelectionDrag,
    ImeComposition,
    CaretNavigation,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlockInternalScrollState {
    pub scroll_top: f64,
    pub viewport_height: f64,
    pub content_height: f64,
    pub handling: WheelHandling,
    pub explicit_exit_enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WheelTransfer {
    pub consumed_by_block: f64,
    pub transfer_to_document: f64,
    pub next_scroll_top: f64,
    pub handling: WheelHandling,
    pub preserve_document_anchor: bool,
}

impl BlockInternalScrollState {
    pub fn max_scroll_top(&self) -> f64 {
        (self.content_height - self.viewport_height).max(0.0)
    }

    pub fn handle_wheel(
        &self,
        delta_y: f64,
        interaction: ComplexBlockInteraction,
    ) -> WheelTransfer {
        if matches!(
            interaction,
            ComplexBlockInteraction::SelectionDrag
                | ComplexBlockInteraction::ImeComposition
                | ComplexBlockInteraction::CaretNavigation
        ) {
            return WheelTransfer {
                consumed_by_block: 0.0,
                transfer_to_document: delta_y,
                next_scroll_top: self.scroll_top,
                handling: WheelHandling::DocumentScroll,
                preserve_document_anchor: true,
            };
        }

        match self.handling {
            WheelHandling::DocumentScroll => WheelTransfer {
                consumed_by_block: 0.0,
                transfer_to_document: delta_y,
                next_scroll_top: self.scroll_top,
                handling: WheelHandling::DocumentScroll,
                preserve_document_anchor: true,
            },
            WheelHandling::BlockInternalScroll | WheelHandling::ConditionalTransferToDocument => {
                let max_scroll = self.max_scroll_top();
                let desired = self.scroll_top + delta_y;
                let next = desired.clamp(0.0, max_scroll);
                let consumed = next - self.scroll_top;
                let remaining = delta_y - consumed;
                let transfer = if self.handling == WheelHandling::ConditionalTransferToDocument {
                    remaining
                } else if self.explicit_exit_enabled && (next <= 0.0 || next >= max_scroll) {
                    remaining
                } else {
                    0.0
                };
                WheelTransfer {
                    consumed_by_block: consumed,
                    transfer_to_document: transfer,
                    next_scroll_top: next,
                    handling: if transfer.abs() > 0.0 {
                        WheelHandling::ConditionalTransferToDocument
                    } else {
                        WheelHandling::BlockInternalScroll
                    },
                    preserve_document_anchor: true,
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CodeBlockEditorModel {
    pub line_count: usize,
    pub line_height: f64,
    pub column_width: f64,
    pub padding_y: f64,
    selection: Option<BlockInnerSelection>,
    anchor: Option<BlockInnerAnchor>,
}

impl CodeBlockEditorModel {
    pub fn new(line_count: usize, line_height: f64) -> Self {
        Self {
            line_count,
            line_height,
            column_width: 8.0,
            padding_y: 8.0,
            selection: None,
            anchor: None,
        }
    }
}

impl BlockEditorModel for CodeBlockEditorModel {
    fn apply_inner_op(&mut self, op: BlockInnerOperation) -> BlockInnerChange {
        match op {
            BlockInnerOperation::InsertCodeLines { index, count } => {
                self.line_count += count;
                BlockInnerChange {
                    affected_range: index..index + count,
                    height_changed: true,
                    outer_height_corrections: 1,
                    selection: self.selection.clone(),
                    anchor: self.anchor.clone(),
                }
            }
            BlockInnerOperation::DeleteCodeLines { range } => {
                let removed = range.len().min(self.line_count);
                self.line_count = self.line_count.saturating_sub(removed).max(1);
                BlockInnerChange {
                    affected_range: range,
                    height_changed: true,
                    outer_height_corrections: 1,
                    selection: self.selection.clone(),
                    anchor: self.anchor.clone(),
                }
            }
            BlockInnerOperation::Select(selection) => {
                self.selection = Some(selection.clone());
                BlockInnerChange {
                    affected_range: 0..0,
                    height_changed: false,
                    outer_height_corrections: 0,
                    selection: Some(selection),
                    anchor: self.anchor.clone(),
                }
            }
            BlockInnerOperation::SetAnchor(anchor) => {
                self.anchor = Some(anchor.clone());
                BlockInnerChange {
                    affected_range: 0..0,
                    height_changed: false,
                    outer_height_corrections: 0,
                    selection: self.selection.clone(),
                    anchor: Some(anchor),
                }
            }
            _ => BlockInnerChange {
                affected_range: 0..0,
                height_changed: false,
                outer_height_corrections: 0,
                selection: self.selection.clone(),
                anchor: self.anchor.clone(),
            },
        }
    }

    fn estimate_height(&self, _width: f64) -> HeightEstimate {
        HeightEstimate::new(
            self.line_count.max(1) as f64 * self.line_height + self.padding_y,
            HeightConfidence::Predictive,
            self.line_height,
        )
    }

    fn visible_fragments(&self, viewport: BlockViewport) -> Vec<BlockFragment> {
        let start = (viewport.scroll_top / self.line_height).floor().max(0.0) as usize;
        let end = ((viewport.scroll_top + viewport.height) / self.line_height).ceil() as usize;
        let end = end.min(self.line_count);
        vec![BlockFragment {
            range: start..end,
            y_range: start as f64 * self.line_height..end as f64 * self.line_height,
            kind: BlockFragmentKind::CodeLines,
        }]
    }

    fn hit_test(&self, point: Point) -> BlockHitTestResult {
        let line = (point.y / self.line_height).floor().max(0.0) as usize;
        if line >= self.line_count {
            return BlockHitTestResult::None;
        }
        let column = (point.x / self.column_width).floor().max(0.0) as usize;
        BlockHitTestResult::Code { line, column }
    }

    fn selection(&self) -> Option<&BlockInnerSelection> {
        self.selection.as_ref()
    }

    fn anchor(&self) -> Option<&BlockInnerAnchor> {
        self.anchor.as_ref()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TableEditorModel {
    pub row_count: usize,
    pub col_count: usize,
    pub row_height: f64,
    pub column_width: f64,
    pub header_height: f64,
    selection: Option<BlockInnerSelection>,
    anchor: Option<BlockInnerAnchor>,
}

impl TableEditorModel {
    pub fn new(row_count: usize, col_count: usize) -> Self {
        Self {
            row_count,
            col_count,
            row_height: 28.0,
            column_width: 120.0,
            header_height: 32.0,
            selection: None,
            anchor: None,
        }
    }
}

impl BlockEditorModel for TableEditorModel {
    fn apply_inner_op(&mut self, op: BlockInnerOperation) -> BlockInnerChange {
        match op {
            BlockInnerOperation::InsertTableRows { index, count } => {
                self.row_count += count;
                BlockInnerChange {
                    affected_range: index..index + count,
                    height_changed: true,
                    outer_height_corrections: 1,
                    selection: self.selection.clone(),
                    anchor: self.anchor.clone(),
                }
            }
            BlockInnerOperation::DeleteTableRows { range } => {
                let removed = range.len().min(self.row_count);
                self.row_count = self.row_count.saturating_sub(removed);
                BlockInnerChange {
                    affected_range: range,
                    height_changed: true,
                    outer_height_corrections: 1,
                    selection: self.selection.clone(),
                    anchor: self.anchor.clone(),
                }
            }
            BlockInnerOperation::Select(selection) => {
                self.selection = Some(selection.clone());
                BlockInnerChange {
                    affected_range: 0..0,
                    height_changed: false,
                    outer_height_corrections: 0,
                    selection: Some(selection),
                    anchor: self.anchor.clone(),
                }
            }
            BlockInnerOperation::SetAnchor(anchor) => {
                self.anchor = Some(anchor.clone());
                BlockInnerChange {
                    affected_range: 0..0,
                    height_changed: false,
                    outer_height_corrections: 0,
                    selection: self.selection.clone(),
                    anchor: Some(anchor),
                }
            }
            _ => BlockInnerChange {
                affected_range: 0..0,
                height_changed: false,
                outer_height_corrections: 0,
                selection: self.selection.clone(),
                anchor: self.anchor.clone(),
            },
        }
    }

    fn estimate_height(&self, _width: f64) -> HeightEstimate {
        HeightEstimate::new(
            self.header_height + self.row_count as f64 * self.row_height,
            HeightConfidence::Predictive,
            self.row_height * 2.0,
        )
    }

    fn visible_fragments(&self, viewport: BlockViewport) -> Vec<BlockFragment> {
        let body_scroll = viewport.scroll_top.saturating_sub_f64(self.header_height);
        let start = (body_scroll / self.row_height).floor().max(0.0) as usize;
        let end = ((body_scroll + viewport.height) / self.row_height).ceil() as usize;
        let end = end.min(self.row_count);
        vec![BlockFragment {
            range: start..end,
            y_range: self.header_height + start as f64 * self.row_height
                ..self.header_height + end as f64 * self.row_height,
            kind: BlockFragmentKind::TableRows,
        }]
    }

    fn hit_test(&self, point: Point) -> BlockHitTestResult {
        if point.y < self.header_height {
            return BlockHitTestResult::None;
        }
        let row = ((point.y - self.header_height) / self.row_height)
            .floor()
            .max(0.0) as usize;
        let col = (point.x / self.column_width).floor().max(0.0) as usize;
        if row >= self.row_count || col >= self.col_count {
            return BlockHitTestResult::None;
        }
        BlockHitTestResult::TableCell {
            row,
            col,
            offset: 0,
        }
    }

    fn selection(&self) -> Option<&BlockInnerSelection> {
        self.selection.as_ref()
    }

    fn anchor(&self) -> Option<&BlockInnerAnchor> {
        self.anchor.as_ref()
    }
}

trait SaturatingSubF64 {
    fn saturating_sub_f64(self, rhs: f64) -> f64;
}

impl SaturatingSubF64 for f64 {
    fn saturating_sub_f64(self, rhs: f64) -> f64 {
        (self - rhs).max(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ten_mb_code_block_scroll_uses_line_virtualization() {
        let model = CodeBlockEditorModel::new(500_000, 18.0);
        let fragments = model.visible_fragments(BlockViewport {
            scroll_top: 18.0 * 10_000.0,
            height: 18.0 * 40.0,
            width: 800.0,
        });
        assert_eq!(fragments.len(), 1);
        assert_eq!(fragments[0].range, 10_000..10_040);
    }

    #[test]
    fn table_50k_rows_scroll_uses_row_virtualization() {
        let model = TableEditorModel::new(50_000, 20);
        let fragments = model.visible_fragments(BlockViewport {
            scroll_top: 32.0 + 28.0 * 20_000.0,
            height: 28.0 * 30.0,
            width: 1000.0,
        });
        assert_eq!(fragments[0].range, 20_000..20_030);
    }

    #[test]
    fn table_batch_insert_1000_rows_triggers_one_outer_height_correction() {
        let mut model = TableEditorModel::new(100, 5);
        let change = model.apply_inner_op(BlockInnerOperation::InsertTableRows {
            index: 20,
            count: 1_000,
        });
        assert_eq!(model.row_count, 1_100);
        assert!(change.height_changed);
        assert_eq!(change.outer_height_corrections, 1);
        assert_eq!(change.affected_range, 20..1020);
    }

    #[test]
    fn code_block_hit_test_returns_inner_line_and_column() {
        let model = CodeBlockEditorModel::new(100, 20.0);
        assert_eq!(
            model.hit_test(Point { x: 40.0, y: 60.0 }),
            BlockHitTestResult::Code { line: 3, column: 5 }
        );
    }

    #[test]
    fn table_hit_test_returns_inner_cell() {
        let model = TableEditorModel::new(100, 10);
        assert_eq!(
            model.hit_test(Point {
                x: 250.0,
                y: 32.0 + 56.0
            }),
            BlockHitTestResult::TableCell {
                row: 2,
                col: 2,
                offset: 0
            }
        );
    }

    #[test]
    fn block_inner_selection_and_anchor_are_stored_separately_from_document_selection() {
        let mut model = TableEditorModel::new(100, 10);
        let selection = BlockInnerSelection::TableCells {
            rows: 2..4,
            cols: 1..3,
        };
        model.apply_inner_op(BlockInnerOperation::Select(selection.clone()));
        let anchor = BlockInnerAnchor::TableCell {
            row: 3,
            col: 2,
            offset: 0,
        };
        model.apply_inner_op(BlockInnerOperation::SetAnchor(anchor.clone()));

        assert_eq!(model.selection(), Some(&selection));
        assert_eq!(model.anchor(), Some(&anchor));
    }

    #[test]
    fn code_block_internal_scroll_consumes_wheel_before_document() {
        let state = BlockInternalScrollState {
            scroll_top: 0.0,
            viewport_height: 100.0,
            content_height: 1_000.0,
            handling: WheelHandling::ConditionalTransferToDocument,
            explicit_exit_enabled: false,
        };

        let transfer = state.handle_wheel(50.0, ComplexBlockInteraction::Normal);

        assert_eq!(transfer.consumed_by_block, 50.0);
        assert_eq!(transfer.transfer_to_document, 0.0);
        assert_eq!(transfer.next_scroll_top, 50.0);
        assert_eq!(transfer.handling, WheelHandling::BlockInternalScroll);
        assert!(transfer.preserve_document_anchor);
    }

    #[test]
    fn internal_scroll_at_boundary_transfers_remaining_delta_to_document() {
        let state = BlockInternalScrollState {
            scroll_top: 900.0,
            viewport_height: 100.0,
            content_height: 1_000.0,
            handling: WheelHandling::ConditionalTransferToDocument,
            explicit_exit_enabled: false,
        };

        let transfer = state.handle_wheel(50.0, ComplexBlockInteraction::Normal);

        assert_eq!(transfer.consumed_by_block, 0.0);
        assert_eq!(transfer.transfer_to_document, 50.0);
        assert_eq!(transfer.next_scroll_top, 900.0);
        assert_eq!(
            transfer.handling,
            WheelHandling::ConditionalTransferToDocument
        );
    }

    #[test]
    fn table_internal_scroll_transfers_after_consuming_to_bottom() {
        let state = BlockInternalScrollState {
            scroll_top: 850.0,
            viewport_height: 100.0,
            content_height: 1_000.0,
            handling: WheelHandling::ConditionalTransferToDocument,
            explicit_exit_enabled: false,
        };

        let transfer = state.handle_wheel(100.0, ComplexBlockInteraction::Normal);

        assert_eq!(transfer.consumed_by_block, 50.0);
        assert_eq!(transfer.transfer_to_document, 50.0);
        assert_eq!(transfer.next_scroll_top, 900.0);
        assert_eq!(
            transfer.handling,
            WheelHandling::ConditionalTransferToDocument
        );
    }

    #[test]
    fn embed_wheel_capture_requires_exit_or_boundary_transfer() {
        let state = BlockInternalScrollState {
            scroll_top: 900.0,
            viewport_height: 100.0,
            content_height: 1_000.0,
            handling: WheelHandling::BlockInternalScroll,
            explicit_exit_enabled: true,
        };

        let transfer = state.handle_wheel(40.0, ComplexBlockInteraction::Normal);

        assert_eq!(transfer.consumed_by_block, 0.0);
        assert_eq!(transfer.transfer_to_document, 40.0);
        assert_eq!(transfer.next_scroll_top, 900.0);
        assert_eq!(
            transfer.handling,
            WheelHandling::ConditionalTransferToDocument
        );
    }

    #[test]
    fn selection_drag_or_ime_does_not_swallow_document_auto_scroll() {
        let state = BlockInternalScrollState {
            scroll_top: 50.0,
            viewport_height: 100.0,
            content_height: 1_000.0,
            handling: WheelHandling::BlockInternalScroll,
            explicit_exit_enabled: false,
        };

        let selection_drag = state.handle_wheel(30.0, ComplexBlockInteraction::SelectionDrag);
        assert_eq!(selection_drag.consumed_by_block, 0.0);
        assert_eq!(selection_drag.transfer_to_document, 30.0);
        assert_eq!(selection_drag.next_scroll_top, 50.0);
        assert_eq!(selection_drag.handling, WheelHandling::DocumentScroll);

        let ime = state.handle_wheel(-25.0, ComplexBlockInteraction::ImeComposition);
        assert_eq!(ime.consumed_by_block, 0.0);
        assert_eq!(ime.transfer_to_document, -25.0);
        assert_eq!(ime.next_scroll_top, 50.0);
        assert_eq!(ime.handling, WheelHandling::DocumentScroll);
    }

    #[test]
    fn outer_scroll_anchor_not_overwritten_by_inner_scroll() {
        let state = BlockInternalScrollState {
            scroll_top: 100.0,
            viewport_height: 100.0,
            content_height: 1_000.0,
            handling: WheelHandling::ConditionalTransferToDocument,
            explicit_exit_enabled: false,
        };

        let transfer = state.handle_wheel(20.0, ComplexBlockInteraction::Normal);

        assert_eq!(transfer.next_scroll_top, 120.0);
        assert!(transfer.preserve_document_anchor);
    }
}
