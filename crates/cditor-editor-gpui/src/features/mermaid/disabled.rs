use std::collections::HashSet;
use std::sync::Arc;

use cditor_core::ids::BlockId;
use cditor_runtime::EditorViewProjection;
use gpui::{AnyElement, App, Context, Entity, RenderImage};

use crate::app::worker_admission::EditorWorkerAdmission;
use crate::editor_view::CditorV2View;
use crate::theme::GuiTheme;

#[derive(Default)]
pub(crate) struct MermaidRenderCache;

#[derive(Default)]
pub(crate) struct MermaidRenderCacheTrimResult {
    pub(crate) evicted_entries: usize,
    pub(crate) evicted_budgeted_bytes: usize,
    pub(crate) evicted_resident_bytes: usize,
    pub(crate) invalidated_renderings: usize,
    pub(crate) remaining_entries: usize,
    pub(crate) remaining_budgeted_bytes: usize,
    pub(crate) remaining_resident_bytes: usize,
    pub(crate) retired_images: Vec<Arc<RenderImage>>,
}

impl MermaidRenderCache {
    pub(crate) fn diagnostics(&self) -> cditor_sdk::diagnostics::MermaidDiagnostics {
        cditor_sdk::diagnostics::MermaidDiagnostics::default()
    }

    pub(crate) fn sync_visible_window(
        &mut self,
        _projection: &EditorViewProjection,
        _source_blocks: &HashSet<BlockId>,
        _preview_code_blocks: &HashSet<BlockId>,
        _theme: GuiTheme,
        _worker_admission: &EditorWorkerAdmission,
        _cx: &mut Context<CditorV2View>,
    ) {
    }

    pub(crate) fn clear(&mut self) -> Vec<Arc<RenderImage>> {
        Vec::new()
    }

    pub(crate) fn apply_memory_pressure(
        &mut self,
        _pressure: crate::memory_pressure::CditorMemoryPressure,
        _protected: &HashSet<BlockId>,
    ) -> MermaidRenderCacheTrimResult {
        MermaidRenderCacheTrimResult::default()
    }
}

/// mermaid 关闭时代码块拿不到图，原样显示源码。
pub(crate) fn render_code_block_mermaid_preview(
    _block_id: BlockId,
    _content_version: u64,
    source_content: AnyElement,
    _cache: &MermaidRenderCache,
    _theme: GuiTheme,
    _view: Entity<CditorV2View>,
    _report_stable_height: bool,
    _cx: &mut App,
) -> AnyElement {
    source_content
}

#[expect(
    clippy::too_many_arguments,
    reason = "matches the enabled renderer contract"
)]
pub(crate) fn render_mermaid_block(
    _block_id: BlockId,
    _content_version: u64,
    _layout_height_px: f64,
    _source_block_height_px: f64,
    source_content: AnyElement,
    _show_source: bool,
    _cache: &MermaidRenderCache,
    _theme: GuiTheme,
    _view: Entity<CditorV2View>,
    _animated_block_height: Option<f64>,
    _cx: &mut App,
) -> AnyElement {
    source_content
}
