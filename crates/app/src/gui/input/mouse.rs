use gpui::{App, Entity, MouseDownEvent, MouseMoveEvent, Window};

use crate::gui::app::CditorV2View;
use cditor_core::ids::BlockId;
use cditor_runtime::DocumentRuntime;

pub fn focus_block_from_mouse(
    view: &Entity<CditorV2View>,
    block_id: BlockId,
    event: &MouseDownEvent,
    window: &mut Window,
    cx: &mut App,
) {
    let position = event.position;
    view.update(cx, |view, cx| {
        view.focus_block_from_gui_at_position(block_id, position, window, cx);
    });
}

pub fn focus_table_cell_from_mouse(
    view: &Entity<CditorV2View>,
    block_id: BlockId,
    row: usize,
    col: usize,
    event: &MouseDownEvent,
    window: &mut Window,
    cx: &mut App,
) {
    let position = event.position;
    view.update(cx, |view, cx| {
        view.focus_table_cell_from_gui(block_id, row, col, Some(position), window, cx);
    });
}

pub fn begin_table_cell_text_selection_from_mouse(
    view: &Entity<CditorV2View>,
    block_id: BlockId,
    row: usize,
    col: usize,
    event: &MouseDownEvent,
    window: &mut Window,
    cx: &mut App,
) {
    let position = event.position;
    view.update(cx, |view, cx| {
        view.begin_table_cell_text_selection_from_gui(
            block_id,
            row,
            col,
            Some(position),
            window,
            cx,
        );
    });
}

pub fn update_table_cell_text_selection_from_mouse(
    view: &Entity<CditorV2View>,
    block_id: BlockId,
    row: usize,
    col: usize,
    event: &MouseMoveEvent,
    cx: &mut App,
) {
    if !event.dragging() {
        return;
    }
    let position = event.position;
    view.update(cx, |view, cx| {
        view.update_table_cell_text_selection_from_gui(block_id, row, col, position, cx);
    });
}

pub fn toggle_todo_from_mouse(
    view: &Entity<CditorV2View>,
    block_id: BlockId,
    _event: &MouseDownEvent,
    _window: &mut Window,
    cx: &mut App,
) {
    view.update(cx, |view, cx| {
        view.toggle_todo_from_gui(block_id, cx);
    });
}

pub fn toggle_block_fold_from_mouse(
    view: &Entity<CditorV2View>,
    block_id: BlockId,
    _event: &MouseDownEvent,
    window: &mut Window,
    cx: &mut App,
) {
    view.update(cx, |view, cx| {
        view.toggle_block_fold_from_gui(block_id, window, cx);
    });
}

pub fn hover_block_from_mouse(
    view: &Entity<CditorV2View>,
    block_id: BlockId,
    event: &MouseMoveEvent,
    cx: &mut App,
) {
    let dragging = event.dragging();
    view.update(cx, |view, cx| {
        view.hover_block_from_gui(block_id, dragging, cx);
    });
}

pub fn gutter_mouse_down_from_mouse(
    view: &Entity<CditorV2View>,
    block_id: BlockId,
    event: &MouseDownEvent,
    window: &mut Window,
    cx: &mut App,
) {
    let position = event.position;
    view.update(cx, |view, cx| {
        view.gutter_mouse_down_from_gui(block_id, position, window, cx);
    });
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BlockDragSelectionController {
    anchor: Option<BlockId>,
    focus: Option<BlockId>,
}

impl BlockDragSelectionController {
    pub fn begin(&mut self, block_id: BlockId, runtime: &mut DocumentRuntime) -> bool {
        self.anchor = Some(block_id);
        self.focus = Some(block_id);
        runtime.select_visible_block_range(block_id, block_id)
    }

    pub fn update(&mut self, block_id: BlockId, runtime: &mut DocumentRuntime) -> bool {
        let Some(anchor) = self.anchor else {
            return self.begin(block_id, runtime);
        };
        self.focus = Some(block_id);
        runtime.select_visible_block_range(anchor, block_id)
    }

    pub fn finish(&mut self) -> Option<(BlockId, BlockId)> {
        let result = self.anchor.zip(self.focus);
        self.anchor = None;
        self.focus = None;
        result
    }

    pub fn is_dragging(&self) -> bool {
        self.anchor.is_some()
    }
}

#[cfg(test)]
mod tests {
    use cditor_runtime::DocumentRuntime;

    use super::*;

    #[test]
    fn block_drag_selection_updates_runtime_visible_selection() {
        let mut runtime = DocumentRuntime::demo();
        let projection = runtime.projection_for_window();
        let first = projection.blocks[0].block_id;
        let third = projection.blocks[2].block_id;
        let mut controller = BlockDragSelectionController::default();

        assert!(controller.begin(first, &mut runtime));
        assert!(controller.update(third, &mut runtime));
        let projection = runtime.projection_for_window();
        let selected = projection
            .blocks
            .iter()
            .filter(|block| block.selected)
            .map(|block| block.block_id)
            .collect::<Vec<_>>();

        assert_eq!(selected, vec![first, projection.blocks[1].block_id, third]);
        assert_eq!(controller.finish(), Some((first, third)));
        assert!(!controller.is_dragging());
    }
}
