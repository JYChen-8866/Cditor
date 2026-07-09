use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use gpui::{AppContext, ClipboardItem, Context, FocusHandle, KeyDownEvent, Pixels, Point, Window};

use cditor_core::block::GutterBlockDragState;
use cditor_core::ids::BlockId;
use cditor_core::rich_text::{TableCellAlign, TableRange};
use cditor_runtime::InputTarget;

use crate::gui::app::input::text_drag::GuiTextDragSelection;
use crate::gui::app::input_trace::trace_input;
use crate::gui::app::interaction::geometry::ProjectedBlockRect;
use crate::gui::app::interaction::image_resize::GuiImageResizeDrag;
use crate::gui::app::interaction::scrollbar::GuiScrollbarDrag;
use crate::gui::app::interaction::table_reorder::GuiTableReorderDrag;
use crate::gui::app::interaction::table_resize::GuiTableResizeDrag;
use crate::gui::block::table::menu::TableMenuAction;
use crate::gui::block::table::{TableAxis, TableAxisSelection, TableCellRangeSelection};
use cditor_editor::scroll::ScrollAccumulator;

use crate::gui::input::{
    BlockDragSelectionController, CodeLanguageEditKeyResult, CodeLanguageEditState,
    CodeLanguagePopupPlacement, RichClipboardItem, apply_code_language_key,
};
use crate::gui::overlay::GuiToast;
use crate::gui::overlay::SlashMenuState;

use crate::gui::persistence::{EditorSaveStatus, PostgresPersistenceState};
use crate::gui::text::RichTextPlatformLayout;
use crate::gui::text::platform_range_bounds;
use cditor_runtime::DocumentRuntime;

pub(in crate::gui::app) use super::persistence_bridge::save_status_for_mode;
pub use super::state::CditorViewState;

pub struct CditorV2View {
    pub(in crate::gui::app) state: CditorViewState,
    pub(in crate::gui::app) focus: FocusHandle,
    pub(in crate::gui::app) code_language_focus: FocusHandle,
    pub(in crate::gui::app) show_debug: bool,
    pub(in crate::gui::app) readonly: bool,
    pub(in crate::gui::app) save_status: EditorSaveStatus,
    pub(in crate::gui::app) last_wheel_delta_y: f64,
    pub(in crate::gui::app) scroll_accumulator: ScrollAccumulator,
    pub(in crate::gui::app) text_layouts: HashMap<BlockId, RichTextPlatformLayout>,
    pub(in crate::gui::app) table_cell_layouts: HashMap<TableCellLayoutKey, RichTextPlatformLayout>,
    pub(in crate::gui::app) scrollbar_drag: Option<GuiScrollbarDrag>,
    pub(in crate::gui::app) text_drag_selection: Option<GuiTextDragSelection>,
    pub(in crate::gui::app) block_drag_selection: BlockDragSelectionController,
    pub(in crate::gui::app) internal_clipboard: Option<RichClipboardItem>,
    pub(in crate::gui::app) code_language_edit: Option<CodeLanguageEditState>,
    pub(in crate::gui::app) slash_menu: Option<SlashMenuState>,
    pub(in crate::gui::app) toast: Option<GuiToast>,
    pub(in crate::gui::app) selected_table_axis: Option<TableAxisSelection>,
    pub(in crate::gui::app) selected_table_cell_range: Option<TableCellRangeSelection>,
    pub(in crate::gui::app) table_cell_range_drag: Option<TableCellRangeSelection>,
    pub(in crate::gui::app) hovered_block_id: Option<BlockId>,
    pub(in crate::gui::app) action_block_id: Option<BlockId>,
    pub(in crate::gui::app) gutter_block_drag: Option<GutterBlockDragState>,
    pub(in crate::gui::app) gutter_drag_auto_scroll_scheduled: bool,
    pub(in crate::gui::app) image_resize_drag: Option<GuiImageResizeDrag>,
    pub(in crate::gui::app) table_resize_drag: Option<GuiTableResizeDrag>,
    pub(in crate::gui::app) table_reorder_drag: Option<GuiTableReorderDrag>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GuiPlatformInputTarget {
    BlockText {
        block_id: BlockId,
    },
    TableCell {
        block_id: BlockId,
        row: usize,
        col: usize,
    },
    CodeLanguage {
        block_id: BlockId,
    },
}

impl GuiPlatformInputTarget {
    pub(crate) fn from_runtime_target(target: InputTarget) -> Self {
        match target {
            InputTarget::BlockText { block_id } => Self::BlockText { block_id },
            InputTarget::TableCell { block_id, row, col } => Self::TableCell { block_id, row, col },
        }
    }

    pub(crate) fn code_language(block_id: BlockId) -> Self {
        Self::CodeLanguage { block_id }
    }

    pub(crate) fn block_id(self) -> BlockId {
        match self {
            Self::BlockText { block_id }
            | Self::TableCell { block_id, .. }
            | Self::CodeLanguage { block_id } => block_id,
        }
    }

    pub(crate) fn is_code_language_for(self, block_id: BlockId) -> bool {
        self == Self::CodeLanguage { block_id }
    }

    pub(crate) fn matches_runtime_target(self, target: InputTarget) -> bool {
        self == Self::from_runtime_target(target)
    }
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
    pub(in crate::gui::app) fn begin_platform_input_registration_frame(&mut self) {
        self.platform_input_target = self
            .code_language_edit
            .as_ref()
            .map(|edit| GuiPlatformInputTarget::code_language(edit.block_id));
    }

    pub(crate) fn register_platform_input_target(
        &mut self,
        target: GuiPlatformInputTarget,
    ) -> bool {
        let Some(runtime) = self.ready_runtime_ref() else {
            return false;
        };
        if !platform_input_registration_allows(self.platform_input_target, target, runtime) {
            trace_input(
                "register_platform_input_target.rejected",
                format_args!(
                    "current={:?} target={:?} runtime={:?}",
                    self.platform_input_target,
                    target,
                    runtime.input_session_target()
                ),
            );
            return false;
        }
        self.platform_input_target = Some(target);
        true
    }
}

pub(crate) fn platform_input_registration_allows(
    current: Option<GuiPlatformInputTarget>,
    target: GuiPlatformInputTarget,
    runtime: &DocumentRuntime,
) -> bool {
    if current.is_some_and(|current| current != target) {
        return false;
    }
    runtime
        .input_session_target()
        .is_some_and(|runtime_target| target.matches_runtime_target(runtime_target))
}

impl CditorV2View {
    pub(crate) fn start_code_language_edit_from_gui(
        &mut self,
        block_id: BlockId,
        language: Option<&str>,
        pointer_y_px: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.code_language_focus, cx);
        self.platform_input_target = Some(GuiPlatformInputTarget::code_language(block_id));
        let placement = code_language_popup_placement(pointer_y_px, window);
        self.code_language_edit = Some(CodeLanguageEditState::new_with_placement(
            block_id, language, placement,
        ));
        cx.notify();
    }

    pub(crate) fn commit_code_language_edit(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(edit) = self.code_language_edit.take() else {
            return false;
        };
        if self.platform_input_target == Some(GuiPlatformInputTarget::code_language(edit.block_id))
        {
            self.platform_input_target = None;
        }
        let changed = self
            .ready_runtime()
            .and_then(|runtime| {
                runtime
                    .set_code_block_language(edit.block_id, edit.normalized_draft())
                    .ok()
            })
            .unwrap_or(false);
        if changed {
            self.mark_dirty(cx);
        }
        changed
    }

    pub(crate) fn select_code_language_from_gui(
        &mut self,
        block_id: BlockId,
        language: String,
        cx: &mut Context<Self>,
    ) {
        self.code_language_edit = Some(CodeLanguageEditState {
            block_id,
            original: String::new(),
            draft: language,
            is_open: false,
            selected_index: 0,
            scroll_start: 0,
            placement: CodeLanguagePopupPlacement::Below,
            caret_offset: 0,
            marked_range: None,
        });
        self.platform_input_target = Some(GuiPlatformInputTarget::code_language(block_id));
        self.commit_code_language_edit(cx);
        cx.notify();
    }

    pub(crate) fn apply_code_language_key_from_gui(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(edit) = self.code_language_edit.as_mut() else {
            return false;
        };
        match apply_code_language_key(edit, event) {
            CodeLanguageEditKeyResult::Commit => {
                self.commit_code_language_edit(cx);
                true
            }
            CodeLanguageEditKeyResult::Cancel => self.cancel_code_language_edit(cx),
            CodeLanguageEditKeyResult::Changed => true,
            CodeLanguageEditKeyResult::Ignored => false,
        }
    }

    pub(crate) fn cancel_code_language_edit(&mut self, cx: &mut Context<Self>) -> bool {
        let had_edit = self.code_language_edit.take().is_some();
        if had_edit {
            if self
                .platform_input_target
                .is_some_and(|target| target.is_code_language_for(target.block_id()))
            {
                self.platform_input_target = None;
            }
            cx.notify();
        }
        had_edit
    }

    pub(crate) fn scroll_code_language_suggestions_from_gui(
        &mut self,
        delta_rows: isize,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(edit) = self.code_language_edit.as_mut() else {
            return false;
        };
        let changed = edit.scroll_suggestions(delta_rows);
        if changed {
            cx.notify();
        }
        changed
    }

    pub(crate) fn sync_slash_menu_from_runtime(&mut self, cx: &mut Context<Self>) {
        let Some((block_id, text, caret)) = self.ready_runtime_ref().and_then(|runtime| {
            let block_id = runtime.focused_block_id()?;
            let text = runtime.focused_text()?.to_owned();
            let caret = runtime.caret_offset_for_block(block_id)?;
            Some((block_id, text, caret))
        }) else {
            self.slash_menu = None;
            return;
        };
        let Some((trigger_start, query)) =
            crate::gui::overlay::slash_query_before_caret(&text, caret)
        else {
            self.slash_menu = None;
            return;
        };
        let (x, y) = self.slash_menu_anchor(block_id, caret);
        let mut next = SlashMenuState::new(block_id, trigger_start, query, x, y);
        if let Some(previous) = self
            .slash_menu
            .as_ref()
            .filter(|menu| menu.block_id == block_id && menu.trigger_start == trigger_start)
        {
            next.selected_index = previous
                .selected_index
                .min(next.visible_items().len().saturating_sub(1));
            next.scroll_start = previous.scroll_start;
        }
        self.slash_menu = Some(next);
        cx.notify();
    }

    pub(crate) fn apply_slash_menu_index_from_gui(
        &mut self,
        index: usize,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(mut menu) = self.slash_menu.clone() else {
            return false;
        };
        let Some(item) = menu.visible_items().get(index).cloned() else {
            return false;
        };
        menu.selected_index = index;
        self.apply_slash_menu_item(menu, item.kind, cx)
    }

    pub(in crate::gui::app) fn apply_selected_slash_menu_item(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(menu) = self.slash_menu.clone() else {
            return false;
        };
        let Some(item) = menu.selected_item() else {
            return false;
        };
        self.apply_slash_menu_item(menu, item.kind, cx)
    }

    pub(in crate::gui::app) fn move_slash_menu_selection(
        &mut self,
        delta: isize,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(menu) = self.slash_menu.as_mut() else {
            return false;
        };
        let changed = menu.move_selection(delta);
        if changed {
            cx.notify();
        }
        changed
    }

    pub(crate) fn scroll_slash_menu_from_gui(
        &mut self,
        delta_rows: isize,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(menu) = self.slash_menu.as_mut() else {
            return false;
        };
        let changed = menu.scroll(delta_rows);
        if changed {
            cx.notify();
        }
        changed
    }

    pub(crate) fn select_slash_menu_index_from_gui(
        &mut self,
        index: usize,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(menu) = self.slash_menu.as_mut() else {
            return false;
        };
        if index >= menu.visible_items().len() || menu.selected_index == index {
            return false;
        }
        menu.selected_index = index;
        cx.notify();
        true
    }

    pub(crate) fn cancel_slash_menu(&mut self, cx: &mut Context<Self>) -> bool {
        let had_menu = self.slash_menu.take().is_some();
        if had_menu {
            cx.notify();
        }
        had_menu
    }

    fn apply_slash_menu_item(
        &mut self,
        menu: SlashMenuState,
        kind: cditor_core::rich_text::RichBlockKind,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.readonly {
            return false;
        }
        let result: Result<bool, String> = (|| {
            let runtime = self
                .ready_runtime()
                .ok_or_else(|| "runtime is not ready".to_owned())?;
            if runtime.focused_block_id() != Some(menu.block_id) {
                return Ok(false);
            }
            let caret = runtime
                .caret_offset_for_block(menu.block_id)
                .unwrap_or(menu.trigger_start);
            let deleted_trigger =
                runtime.replace_text_in_focused_range(Some(menu.trigger_start..caret), "")?;
            let converted = runtime.convert_focused_block_kind(kind)?;
            Ok(deleted_trigger || converted)
        })();
        match result {
            Ok(changed) => {
                self.slash_menu = None;
                if changed {
                    self.mark_dirty(cx);
                }
                cx.notify();
                true
            }
            Err(error) => {
                self.save_status = EditorSaveStatus::Failed(error);
                self.slash_menu = None;
                cx.notify();
                false
            }
        }
    }

    fn slash_menu_anchor(&self, block_id: BlockId, caret: usize) -> (f32, f32) {
        if let Some(cache) = self.text_layouts.get(&block_id)
            && let Some(bounds) = platform_range_bounds(cache, caret..caret)
        {
            return (f32::from(bounds.left()), f32::from(bounds.bottom()) + 4.0);
        }
        self.projected_block_rects
            .iter()
            .find(|rect| rect.block_id == block_id)
            .map(|rect| {
                (
                    rect.text_origin_x_in_block_px as f32,
                    (rect.document_top + rect.text_origin_y_in_block_px + 24.0) as f32,
                )
            })
            .unwrap_or((120.0, 120.0))
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
        self.selected_table_axis = None;
        self.selected_table_cell_range = None;
        self.table_cell_range_drag = None;
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

    pub(crate) fn focus_table_cell_from_gui(
        &mut self,
        block_id: BlockId,
        row: usize,
        col: usize,
        position: Option<Point<Pixels>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus, cx);
        self.selected_table_axis = None;
        self.selected_table_cell_range = None;
        let offset = position.and_then(|position| {
            self.text_offset_for_table_cell_at_position(block_id, row, col, position)
        });
        trace_table(
            "focus_cell.gui.begin",
            format_args!(
                "block={block_id} row={row} col={col} position={position:?} resolved_offset={offset:?}"
            ),
        );
        if let CditorViewState::Ready(runtime) = &mut self.state {
            if let Some(offset) = offset {
                let _ = runtime.focus_table_cell_at_offset(block_id, row, col, offset);
            } else {
                let _ = runtime.focus_table_cell(block_id, row, col);
            }
            let payload_state = runtime
                .block_payload_record(block_id)
                .map(|payload| match &payload.payload {
                    cditor_core::rich_text::BlockPayload::Table(table) => format!(
                        "table rows={} cols={} content_version={}",
                        table.rows.len(),
                        table.rows.first().map(|row| row.cells.len()).unwrap_or(0),
                        payload.content_version
                    ),
                    other => format!("non_table payload={other:?}"),
                })
                .unwrap_or_else(|| "missing_payload".to_owned());
            trace_table(
                "focus_cell.gui.end",
                format_args!(
                    "block={block_id} row={row} col={col} focused_block={:?} focused_cell={:?} focused_cell_offset={:?} payload={payload_state}",
                    runtime.focused_block_id(),
                    runtime.focused_table_cell_for_block(block_id),
                    runtime.focused_table_cell_offset()
                ),
            );
        }
        cx.notify();
    }

    pub(crate) fn begin_table_cell_range_selection_from_gui(
        &mut self,
        block_id: BlockId,
        row: usize,
        col: usize,
        position: Option<Point<Pixels>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_table_cell_from_gui(block_id, row, col, position, window, cx);
        self.table_cell_range_drag =
            Some(TableCellRangeSelection::new(block_id, row, col, row, col));
        self.selected_table_cell_range = None;
    }

    pub(crate) fn update_table_cell_range_selection_from_gui(
        &mut self,
        block_id: BlockId,
        row: usize,
        col: usize,
        cx: &mut Context<Self>,
    ) {
        let Some(anchor) = self.table_cell_range_drag else {
            return;
        };
        if anchor.block_id != block_id {
            return;
        }
        let selection =
            TableCellRangeSelection::new(block_id, anchor.anchor_row, anchor.anchor_col, row, col);
        self.table_cell_range_drag = Some(selection);
        self.selected_table_axis = None;
        self.selected_table_cell_range = selection.is_multi_cell().then_some(selection);
        cx.notify();
    }

    pub(in crate::gui::app) fn finish_table_cell_range_selection_drag(&mut self) {
        self.table_cell_range_drag = None;
    }

    pub(crate) fn select_table_axis_from_gui(
        &mut self,
        block_id: BlockId,
        axis: TableAxis,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus, cx);
        self.clear_gutter_action();
        self.text_drag_selection = None;
        if let CditorViewState::Ready(runtime) = &mut self.state {
            runtime.focus_block(block_id);
        }
        self.selected_table_cell_range = None;
        self.table_cell_range_drag = None;
        self.selected_table_axis = Some(TableAxisSelection::new(block_id, axis, index));
        cx.notify();
    }

    pub(crate) fn set_selected_table_axis_align_from_gui(
        &mut self,
        align: TableCellAlign,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.readonly {
            return false;
        }
        let Some((block_id, range)) = self.selected_table_range() else {
            return false;
        };
        let changed = self
            .ready_runtime()
            .and_then(|runtime| runtime.set_table_cell_align(block_id, range, align).ok())
            .unwrap_or(false);
        if changed {
            self.mark_dirty(cx);
            cx.notify();
        }
        changed
    }

    pub(crate) fn apply_selected_table_menu_action_from_gui(
        &mut self,
        action: TableMenuAction,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.readonly {
            return false;
        }
        let changed = match action {
            TableMenuAction::InsertRowAbove
                if self
                    .selected_table_axis
                    .is_some_and(|selection| selection.axis == TableAxis::Row) =>
            {
                let selection = self.selected_table_axis.expect("checked row selection");
                self.ready_runtime()
                    .and_then(|runtime| {
                        runtime
                            .insert_table_row(selection.block_id, selection.index)
                            .ok()
                    })
                    .unwrap_or(false)
            }
            TableMenuAction::InsertRowBelow
                if self
                    .selected_table_axis
                    .is_some_and(|selection| selection.axis == TableAxis::Row) =>
            {
                let selection = self.selected_table_axis.expect("checked row selection");
                self.ready_runtime()
                    .and_then(|runtime| {
                        runtime
                            .insert_table_row(selection.block_id, selection.index.saturating_add(1))
                            .ok()
                    })
                    .unwrap_or(false)
            }
            TableMenuAction::DeleteRow
                if self
                    .selected_table_axis
                    .is_some_and(|selection| selection.axis == TableAxis::Row) =>
            {
                let selection = self.selected_table_axis.expect("checked row selection");
                self.ready_runtime()
                    .and_then(|runtime| {
                        runtime
                            .delete_table_row(selection.block_id, selection.index)
                            .ok()
                    })
                    .unwrap_or(false)
            }
            TableMenuAction::DuplicateRow
                if self
                    .selected_table_axis
                    .is_some_and(|selection| selection.axis == TableAxis::Row) =>
            {
                let selection = self.selected_table_axis.expect("checked row selection");
                self.ready_runtime()
                    .and_then(|runtime| {
                        runtime
                            .duplicate_table_row(selection.block_id, selection.index)
                            .ok()
                    })
                    .unwrap_or(false)
            }
            TableMenuAction::InsertColumnLeft
                if self
                    .selected_table_axis
                    .is_some_and(|selection| selection.axis == TableAxis::Column) =>
            {
                let selection = self.selected_table_axis.expect("checked column selection");
                self.ready_runtime()
                    .and_then(|runtime| {
                        runtime
                            .insert_table_column(selection.block_id, selection.index)
                            .ok()
                    })
                    .unwrap_or(false)
            }
            TableMenuAction::InsertColumnRight
                if self
                    .selected_table_axis
                    .is_some_and(|selection| selection.axis == TableAxis::Column) =>
            {
                let selection = self.selected_table_axis.expect("checked column selection");
                self.ready_runtime()
                    .and_then(|runtime| {
                        runtime
                            .insert_table_column(
                                selection.block_id,
                                selection.index.saturating_add(1),
                            )
                            .ok()
                    })
                    .unwrap_or(false)
            }
            TableMenuAction::DeleteColumn
                if self
                    .selected_table_axis
                    .is_some_and(|selection| selection.axis == TableAxis::Column) =>
            {
                let selection = self.selected_table_axis.expect("checked column selection");
                self.ready_runtime()
                    .and_then(|runtime| {
                        runtime
                            .delete_table_column(selection.block_id, selection.index)
                            .ok()
                    })
                    .unwrap_or(false)
            }
            TableMenuAction::DuplicateColumn
                if self
                    .selected_table_axis
                    .is_some_and(|selection| selection.axis == TableAxis::Column) =>
            {
                let selection = self.selected_table_axis.expect("checked column selection");
                self.ready_runtime()
                    .and_then(|runtime| {
                        runtime
                            .duplicate_table_column(selection.block_id, selection.index)
                            .ok()
                    })
                    .unwrap_or(false)
            }
            TableMenuAction::Align(align) => self.set_selected_table_axis_align_from_gui(align, cx),
            TableMenuAction::MergeCells => self.merge_selected_table_axis_from_gui(cx),
            TableMenuAction::SplitCell => self.split_selected_table_axis_from_gui(cx),
            TableMenuAction::BackgroundColor => {
                let Some((block_id, range)) = self.selected_table_range() else {
                    return false;
                };
                self.ready_runtime()
                    .and_then(|runtime| {
                        runtime
                            .set_table_cell_background_color(
                                block_id,
                                range,
                                Some("action_background".to_owned()),
                            )
                            .ok()
                    })
                    .unwrap_or(false)
            }
            TableMenuAction::DuplicateRow
            | TableMenuAction::DuplicateColumn
            | TableMenuAction::InsertRowAbove
            | TableMenuAction::InsertRowBelow
            | TableMenuAction::DeleteRow
            | TableMenuAction::InsertColumnLeft
            | TableMenuAction::InsertColumnRight
            | TableMenuAction::DeleteColumn => false,
        };
        if changed {
            self.mark_dirty(cx);
            cx.notify();
        }
        changed
    }

    pub(crate) fn merge_selected_table_axis_from_gui(&mut self, cx: &mut Context<Self>) -> bool {
        if self.readonly {
            return false;
        }
        let Some((block_id, range)) = self.selected_table_range() else {
            return false;
        };
        let changed = self
            .ready_runtime()
            .and_then(|runtime| runtime.merge_table_cells(block_id, range).ok())
            .unwrap_or(false);
        if changed {
            self.mark_dirty(cx);
            cx.notify();
        }
        changed
    }

    pub(crate) fn split_selected_table_axis_from_gui(&mut self, cx: &mut Context<Self>) -> bool {
        if self.readonly {
            return false;
        }
        let Some((block_id, range)) = self.selected_table_range() else {
            return false;
        };
        let changed = self
            .ready_runtime()
            .and_then(|runtime| {
                runtime
                    .split_table_cell(block_id, range.start_row, range.start_col)
                    .ok()
            })
            .unwrap_or(false);
        if changed {
            self.mark_dirty(cx);
            cx.notify();
        }
        changed
    }

    pub(in crate::gui::app) fn selected_table_range(&self) -> Option<(BlockId, TableRange)> {
        if let Some(selection) = self.selected_table_cell_range {
            let runtime = self.ready_runtime_ref()?;
            return runtime
                .table_range_selection_range(selection.block_id, selection.range)
                .map(|range| (selection.block_id, range));
        }
        let selection = self.selected_table_axis?;
        let runtime = self.ready_runtime_ref()?;
        let range = match selection.axis {
            TableAxis::Row => {
                runtime.table_row_selection_range(selection.block_id, selection.index)
            }
            TableAxis::Column => {
                runtime.table_column_selection_range(selection.block_id, selection.index)
            }
        };
        range.map(|range| (selection.block_id, range))
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
        self.gutter_block_drag = None;
        self.gutter_drag_auto_scroll_scheduled = false;
    }
}

pub(in crate::gui::app) fn block_focus_offset_after_missed_hit_test(
    focused_block_id: Option<BlockId>,
    target_block_id: BlockId,
    target_caret_offset: Option<usize>,
) -> usize {
    if focused_block_id == Some(target_block_id) {
        target_caret_offset.unwrap_or(0)
    } else {
        0
    }
}

fn code_language_popup_placement(pointer_y_px: f32, window: &Window) -> CodeLanguagePopupPlacement {
    const POPUP_MARGIN_PX: f32 = 12.0;
    const POPUP_ESTIMATED_HEIGHT_PX: f32 = 260.0;

    let viewport_height = f32::from(window.viewport_size().height);
    let below = viewport_height - pointer_y_px - POPUP_MARGIN_PX;
    let above = pointer_y_px - POPUP_MARGIN_PX;
    if below < POPUP_ESTIMATED_HEIGHT_PX && above > below {
        CodeLanguagePopupPlacement::Above
    } else {
        CodeLanguagePopupPlacement::Below
    }
}

#[cfg(test)]
#[path = "cditor_v2_view_tests.rs"]
mod cditor_v2_view_tests;
