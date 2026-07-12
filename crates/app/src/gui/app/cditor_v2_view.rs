use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use gpui::{AppContext, ClipboardItem, Context, FocusHandle, Pixels, Point, Window};

use cditor_core::block::GutterBlockDragState;
use cditor_core::ids::BlockId;

use crate::gui::app::input::text_drag::GuiTextDragSelection;
use crate::gui::app::input_trace::trace_input;
use crate::gui::app::interaction::geometry::ProjectedBlockRect;
use crate::gui::app::interaction::image_resize::GuiImageResizeDrag;
use crate::gui::app::interaction::scrollbar::GuiScrollbarDrag;
use crate::gui::app::interaction::table_mode::GuiTableInteractionMode;
use crate::gui::app::interaction::table_reorder::GuiTableReorderDrag;
use crate::gui::app::interaction::table_resize::GuiTableResizeDrag;
use crate::gui::app::interaction::table_scroll::{GuiTableHScrollDrag, GuiTableScrollState};
use crate::gui::block::{MermaidRenderCache, WhiteboardThumbnailCache};
use cditor_editor::scroll::ScrollAccumulator;

use crate::gui::input::{AiPromptState, BlockDragSelectionController, CodeLanguageEditState};
use crate::gui::overlay::GuiToast;
use crate::gui::overlay::SlashMenuState;
use crate::gui::overlay::WhiteboardEditorSession;

use crate::gui::persistence::{EditorSaveStatus, PostgresPersistenceState};
use crate::gui::text::RichTextPlatformLayout;
use cditor_runtime::DocumentRuntime;

pub(in crate::gui::app) mod ai;
mod block_actions;
mod code_language;
mod formatting_toolbar;
mod platform_input;
mod slash_menu;
mod table_actions;
mod whiteboard;

pub(in crate::gui::app) use super::persistence_bridge::save_status_for_mode;
pub use super::state::CditorViewState;
pub(crate) use crate::gui::app::interaction::table_scroll::TableScrollSnapshot;
pub(in crate::gui::app) use block_actions::block_focus_offset_after_missed_hit_test;
pub(in crate::gui::app) use formatting_toolbar::formatting_toolbar_state;
pub(crate) use platform_input::GuiPlatformInputTarget;
#[cfg(test)]
pub(crate) use platform_input::platform_input_registration_allows;

pub struct CditorV2View {
    pub(in crate::gui::app) state: CditorViewState,
    pub(in crate::gui::app) focus: FocusHandle,
    pub(in crate::gui::app) code_language_focus: FocusHandle,
    pub(in crate::gui::app) ai_prompt_focus: FocusHandle,
    pub(in crate::gui::app) ai_provider: Arc<dyn cditor_ai::AiProvider>,
    pub(in crate::gui::app) ai_prompt: Option<AiPromptState>,
    pub(in crate::gui::app) ai_preview_scroll_handle: gpui::ScrollHandle,
    pub(in crate::gui::app) show_debug: bool,
    pub(in crate::gui::app) readonly: bool,
    pub(in crate::gui::app) save_status: EditorSaveStatus,
    pub(in crate::gui::app) last_wheel_delta_y: f64,
    pub(in crate::gui::app) scroll_accumulator: ScrollAccumulator,
    pub(in crate::gui::app) text_layouts: HashMap<BlockId, RichTextPlatformLayout>,
    pub(in crate::gui::app) table_cell_layouts: HashMap<TableCellLayoutKey, RichTextPlatformLayout>,
    pub(in crate::gui::app) table_scroll_state: GuiTableScrollState,
    pub(in crate::gui::app) mermaid_renders: MermaidRenderCache,
    pub(in crate::gui::app) mermaid_source_blocks: std::collections::HashSet<BlockId>,
    pub(in crate::gui::app) whiteboard_thumbnails: WhiteboardThumbnailCache,
    pub(in crate::gui::app) whiteboard_editor: Option<WhiteboardEditorSession>,
    pub(in crate::gui::app) scrollbar_drag: Option<GuiScrollbarDrag>,
    pub(in crate::gui::app) text_drag_selection: Option<GuiTextDragSelection>,
    pub(in crate::gui::app) block_drag_selection: BlockDragSelectionController,
    pub(in crate::gui::app) code_language_edit: Option<CodeLanguageEditState>,
    pub(in crate::gui::app) slash_menu: Option<SlashMenuState>,
    pub(in crate::gui::app) toast: Option<GuiToast>,
    pub(in crate::gui::app) table_interaction_mode: GuiTableInteractionMode,
    pub(in crate::gui::app) hovered_block_id: Option<BlockId>,
    pub(in crate::gui::app) action_block_id: Option<BlockId>,
    pub(in crate::gui::app) gutter_toolbar_block_id: Option<BlockId>,
    pub(in crate::gui::app) gutter_block_drag: Option<GutterBlockDragState>,
    pub(in crate::gui::app) gutter_drag_auto_scroll_scheduled: bool,
    pub(in crate::gui::app) image_resize_drag: Option<GuiImageResizeDrag>,
    pub(in crate::gui::app) table_resize_drag: Option<GuiTableResizeDrag>,
    pub(in crate::gui::app) table_reorder_drag: Option<GuiTableReorderDrag>,
    pub(in crate::gui::app) table_hscroll_drag: Option<GuiTableHScrollDrag>,
    pub(in crate::gui::app) projected_block_rects: Vec<ProjectedBlockRect>,
    pub(in crate::gui::app) postgres_persistence: PostgresPersistenceState,
    pub(in crate::gui::app) autosave_interval: Duration,
    pub(in crate::gui::app) platform_input_target: Option<GuiPlatformInputTarget>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(in crate::gui::app) struct TableCellLayoutKey {
    pub block_id: BlockId,
    pub row: usize,
    pub col: usize,
}

fn table_trace_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("CDITOR_TRACE_TABLE")
            .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
            .unwrap_or(false)
    })
}

fn trace_table(event: &str, details: impl std::fmt::Display) {
    if table_trace_enabled() {
        eprintln!("[cditor][table][gui][{event}] {details}");
    }
}

impl CditorV2View {
    pub(crate) fn toggle_mermaid_source_from_gui(
        &mut self,
        block_id: BlockId,
        cx: &mut Context<Self>,
    ) {
        if !self.mermaid_source_blocks.remove(&block_id) {
            self.mermaid_source_blocks.insert(block_id);
        }
        cx.notify();
    }

    pub(crate) fn copy_code_block_from_gui(&mut self, block_id: BlockId, cx: &mut Context<Self>) {
        let Some(text) = self.ready_runtime_ref().and_then(|runtime| {
            runtime
                .block_payload_record(block_id)
                .map(|payload| payload.plain_text())
        }) else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.show_toast("已将代码拷贝到剪贴板", Duration::from_secs(3), cx);
    }

    fn show_toast(
        &mut self,
        message: impl Into<String>,
        duration: Duration,
        cx: &mut Context<Self>,
    ) {
        self.toast = Some(GuiToast::new(message, duration));
        let dismiss_after = cx.background_spawn(async move {
            std::thread::sleep(duration);
        });
        cx.spawn(async move |view, cx| {
            let _ = dismiss_after.await;
            let _ = view.update(cx, |view, cx| {
                let should_clear = view
                    .toast
                    .as_ref()
                    .is_some_and(|toast| !toast.is_alive(Instant::now()));
                if should_clear {
                    view.toast = None;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn queue_rendered_media_height(
        &mut self,
        block_id: BlockId,
        content_version: u64,
        measured_height: f64,
        _cx: &mut Context<Self>,
    ) -> bool {
        self.ready_runtime()
            .and_then(|runtime| {
                runtime
                    .queue_measured_height(block_id, content_version, measured_height)
                    .ok()
            })
            .unwrap_or(false)
    }

    pub(crate) fn update_text_layout_cache(&mut self, cache: RichTextPlatformLayout) -> bool {
        if let Some(position) = cache.table_cell_position {
            trace_table(
                "cache.table_cell",
                format_args!(
                    "block={} row={} col={} content_version={} bounds=({}, {}, {}, {}) text_len={} lines={}",
                    cache.block_id,
                    position.row,
                    position.col,
                    cache.content_version,
                    f32::from(cache.bounds.left()),
                    f32::from(cache.bounds.top()),
                    f32::from(cache.bounds.size.width),
                    f32::from(cache.bounds.size.height),
                    cache.text.len(),
                    cache.lines.len()
                ),
            );
            self.table_cell_layouts.insert(
                TableCellLayoutKey {
                    block_id: cache.block_id,
                    row: position.row,
                    col: position.col,
                },
                cache,
            );
            return false;
        }
        let block_id = cache.block_id;
        let content_version = cache.content_version;
        let measured_height = cache.measured_height;
        self.text_layouts.insert(block_id, cache);
        self.ready_runtime()
            .and_then(|runtime| {
                runtime
                    .queue_measured_height(block_id, content_version, measured_height)
                    .ok()
            })
            .unwrap_or(false)
    }

    pub(in crate::gui::app) fn ready_runtime(&mut self) -> Option<&mut DocumentRuntime> {
        match &mut self.state {
            CditorViewState::Ready(runtime) => Some(runtime),
            CditorViewState::Loading { .. } | CditorViewState::LoadFailed { .. } => None,
        }
    }

    pub(in crate::gui::app) fn ready_runtime_ref(&self) -> Option<&DocumentRuntime> {
        match &self.state {
            CditorViewState::Ready(runtime) => Some(runtime),
            CditorViewState::Loading { .. } | CditorViewState::LoadFailed { .. } => None,
        }
    }

    pub(crate) fn focus_block_from_gui_at_position(
        &mut self,
        block_id: cditor_core::ids::BlockId,
        position: impl Into<Option<Point<Pixels>>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus, cx);
        if self.table_interaction_mode.block_id().is_some() {
            self.table_interaction_mode = GuiTableInteractionMode::Idle;
        }
        self.clear_gutter_action();
        let position = position.into();
        let offset = position
            .and_then(|position| self.text_offset_for_block_at_position(block_id, position));
        trace_input(
            "focus_block_from_gui_at_position",
            format_args!("block={block_id} position={position:?} resolved_offset={offset:?}"),
        );
        if let CditorViewState::Ready(runtime) = &mut self.state {
            if let Some(offset) = offset {
                let _ = runtime.focus_block_at_offset(block_id, offset);
                self.text_drag_selection = Some(GuiTextDragSelection {
                    anchor_block_id: block_id,
                    anchor_offset: offset,
                });
            } else {
                let anchor_offset = block_focus_offset_after_missed_hit_test(
                    runtime.focused_block_id(),
                    block_id,
                    runtime.caret_offset_for_block(block_id),
                );
                let _ = runtime.focus_block_at_offset(block_id, anchor_offset);
                self.text_drag_selection = Some(GuiTextDragSelection {
                    anchor_block_id: block_id,
                    anchor_offset,
                });
            }
        }
        cx.notify();
    }

    pub(crate) fn toggle_todo_from_gui(&mut self, block_id: BlockId, cx: &mut Context<Self>) {
        if self.readonly {
            return;
        }
        let CditorViewState::Ready(runtime) = &mut self.state else {
            return;
        };
        if runtime.toggle_todo_checked(block_id).unwrap_or(false) {
            self.mark_dirty(cx);
            cx.notify();
        }
    }

    pub(crate) fn focus_down_placer_from_gui(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus, cx);
        if self.readonly {
            return;
        }
        let result = {
            let CditorViewState::Ready(runtime) = &mut self.state else {
                return;
            };
            let result = runtime.focus_or_create_down_placer_paragraph();
            if result.is_ok() {
                let _ = runtime.scroll_focused_block_into_view();
            }
            result
        };
        match result {
            Ok(changed) => {
                if changed {
                    self.mark_dirty(cx);
                }
                cx.notify();
            }
            Err(error) => {
                self.save_status = EditorSaveStatus::Failed(error);
                cx.notify();
            }
        }
    }

    pub(crate) fn hover_block_from_gui(
        &mut self,
        block_id: BlockId,
        dragging: bool,
        cx: &mut Context<Self>,
    ) {
        let hover_changed = self.hovered_block_id != Some(block_id);
        self.hovered_block_id = Some(block_id);
        let mut selection_changed = false;
        if dragging
            && self.block_drag_selection.is_dragging()
            && let CditorViewState::Ready(runtime) = &mut self.state
        {
            selection_changed = self.block_drag_selection.update(block_id, runtime);
        }
        if hover_changed || selection_changed {
            cx.notify();
        }
    }

    pub(in crate::gui::app) fn clear_gutter_action(&mut self) {
        self.action_block_id = None;
        self.gutter_toolbar_block_id = None;
        self.gutter_block_drag = None;
        self.gutter_drag_auto_scroll_scheduled = false;
    }
}

#[cfg(test)]
#[path = "cditor_v2_view_tests.rs"]
mod cditor_v2_view_tests;
