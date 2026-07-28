use cditor_runtime::{EditorViewProjection, WorkCost};
use gpui::Context;

use crate::editor_view::CditorV2View;
use crate::theme::GuiTheme;

#[derive(Default)]
pub(crate) struct WhiteboardThumbnailCache;

impl WhiteboardThumbnailCache {
    pub(crate) fn sync_visible_window(
        &mut self,
        _projection: &EditorViewProjection,
        _theme: GuiTheme,
        _read_only: bool,
        _admit_entity: impl FnMut(WorkCost) -> bool,
        _cx: &mut Context<CditorV2View>,
    ) -> bool {
        false
    }

    pub(crate) fn clear(&mut self) {}
}
