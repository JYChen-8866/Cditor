use cditor_core::ids::BlockId;
use cditor_runtime::EditorViewProjection;
use gpui::{AnyElement, App, Context, Entity};

use crate::app::worker_admission::EditorWorkerAdmission;
use crate::editor_view::CditorV2View;
use crate::theme::GuiTheme;

#[derive(Default)]
pub(crate) struct MermaidRenderCache;

impl MermaidRenderCache {
    pub(crate) fn sync_visible_window(
        &mut self,
        _projection: &EditorViewProjection,
        _theme: GuiTheme,
        _worker_admission: &EditorWorkerAdmission,
        _cx: &mut Context<CditorV2View>,
    ) {
    }

    pub(crate) fn clear(&mut self) {}
}

#[expect(
    clippy::too_many_arguments,
    reason = "matches the enabled renderer contract"
)]
pub(crate) fn render_mermaid_block(
    _block_id: BlockId,
    _content_version: u64,
    source_content: AnyElement,
    _show_source: bool,
    _cache: &MermaidRenderCache,
    _theme: GuiTheme,
    _view: Entity<CditorV2View>,
    _cx: &mut App,
) -> AnyElement {
    source_content
}
