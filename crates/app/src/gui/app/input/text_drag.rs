use gpui::{Context, Pixels, Point};

use crate::gui::app::cditor_v2_view::{CditorV2View, CditorViewState};
use crate::gui::text::platform_index_for_point;
use cditor_core::ids::BlockId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::gui::app) struct GuiTextDragSelection {
    pub(in crate::gui::app) anchor_block_id: BlockId,
    pub(in crate::gui::app) anchor_offset: usize,
}

impl CditorV2View {
    fn text_position_at_point(&self, position: Point<Pixels>) -> Option<(BlockId, usize)> {
        let runtime = self.ready_runtime_ref()?;
        self.text_layouts.iter().find_map(|(block_id, cache)| {
            if runtime.block_content_version(*block_id)? != cache.content_version {
                return None;
            }
            let within_y = position.y >= cache.bounds.top() && position.y <= cache.bounds.bottom();
            within_y.then(|| (*block_id, platform_index_for_point(cache, position)))
        })
    }

    pub(in crate::gui::app) fn update_text_drag_selection(
        &mut self,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let Some(drag) = self.text_drag_selection else {
            return;
        };
        let Some((focus_block_id, focus_offset)) = self.text_position_at_point(position) else {
            return;
        };
        if let CditorViewState::Ready(runtime) = &mut self.state {
            let _ = runtime.set_document_text_selection(
                drag.anchor_block_id,
                drag.anchor_offset,
                focus_block_id,
                focus_offset,
            );
            cx.stop_propagation();
            cx.notify();
        }
    }

    pub(in crate::gui::app) fn finish_text_drag_selection(&mut self) {
        self.text_drag_selection = None;
    }

    pub(in crate::gui::app) fn finish_block_drag_selection(&mut self) {
        let _ = self.block_drag_selection.finish();
    }
}
