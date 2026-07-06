use gpui::prelude::FluentBuilder;
use std::{
    collections::HashMap,
    ops::Range,
    time::{Duration, Instant},
};

use gpui::{
    AnyElement, App, AppContext, Bounds, ClipboardItem, Context, EntityInputHandler, FocusHandle,
    InteractiveElement, IntoElement, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, ParentElement, Pixels, Point, Render, ScrollDelta, ScrollWheelEvent, Size,
    Styled, UTF16Selection, Window, div, px, rgb, rgba,
};

use crate::core::block::{BlockDropTarget, DragPoint, GutterBlockDragState};
use crate::core::ids::BlockId;
use crate::core::rich_text::InlineMark;
use crate::editor::scroll::{
    HeightCorrectionPriority, ScrollAccumulator, ScrollDeltaMode, ScrollDevice, ScrollInput,
    ScrollPhase, ScrollbarPolicy, ScrollbarVisualState,
};
use crate::gui::GuiTheme;
use crate::gui::app::input_trace::trace_input;
use crate::gui::app::interaction::geometry::{
    FallbackViewportOrigin, ProjectedBlockRect, drop_target_for_document_y_from_rects,
    parent_drop_target_from_rects, projected_block_rects_from_projection,
};
use crate::gui::app::interaction::image_resize::GuiImageResizeDrag;
use crate::gui::block::BlockDragOverlaySnapshot;
use crate::gui::clipboard_assets::image_asset_from_clipboard_item;
use crate::gui::document::{
    DocumentBlockActionProjection, DocumentDebugHeader, DocumentEditorView,
};
use crate::gui::image_preview::{
    close_active_preview_if_escape_enabled, render_image_preview_overlay,
};
use crate::gui::input::ime::{
    marked_preview_range_to_base_range, utf8_range_to_utf16_range, utf8_to_utf16_offset,
    utf16_range_to_utf8_range,
};
use crate::gui::input::{BlockDragSelectionController, GuiInputCommand, command_for_key_down};
use crate::gui::persistence::{
    DEFAULT_POSTGRES_SAVE_DEBOUNCE, EditorLoadStateLabel, EditorSaveStatus,
    PostgresPersistenceState, PostgresPersistenceTarget, mark_dirty_and_schedule_postgres_save,
    render_load_state, render_save_indicator, save_postgres_batch,
};
use crate::gui::text::{
    RichTextLayoutInput, RichTextPlatformLayout, platform_index_for_point, platform_range_bounds,
    wrap_rich_text,
};
use crate::runtime::DocumentRuntime;
use crate::storage::postgres::block_on_postgres;

pub struct CditorV2View {
    pub(in crate::gui::app) state: CditorViewState,
    pub(in crate::gui::app) focus: FocusHandle,
    pub(in crate::gui::app) show_debug: bool,
    pub(in crate::gui::app) readonly: bool,
    pub(in crate::gui::app) save_status: EditorSaveStatus,
    pub(in crate::gui::app) last_wheel_delta_y: f64,
    pub(in crate::gui::app) scroll_accumulator: ScrollAccumulator,
    pub(in crate::gui::app) text_layouts: HashMap<BlockId, RichTextPlatformLayout>,
    pub(in crate::gui::app) scrollbar_drag: Option<GuiScrollbarDrag>,
    pub(in crate::gui::app) text_drag_selection: Option<GuiTextDragSelection>,
    pub(in crate::gui::app) block_drag_selection: BlockDragSelectionController,
    pub(in crate::gui::app) hovered_block_id: Option<BlockId>,
    pub(in crate::gui::app) action_block_id: Option<BlockId>,
    pub(in crate::gui::app) gutter_block_drag: Option<GutterBlockDragState>,
    pub(in crate::gui::app) gutter_drag_auto_scroll_scheduled: bool,
    pub(in crate::gui::app) image_resize_drag: Option<GuiImageResizeDrag>,
    pub(in crate::gui::app) projected_block_rects: Vec<ProjectedBlockRect>,
    pub(in crate::gui::app) postgres_persistence: PostgresPersistenceState,
    pub(in crate::gui::app) autosave_interval: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::gui::app) struct GuiScrollbarDrag {
    pub(in crate::gui::app) pointer_y_offset_in_thumb: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::gui::app) struct GuiTextDragSelection {
    pub(in crate::gui::app) anchor_block_id: BlockId,
    pub(in crate::gui::app) anchor_offset: usize,
}

const GUI_SCROLLBAR_WIDTH_PX: f32 = 10.0;
const GUI_SCROLLBAR_RIGHT_PX: f32 = 8.0;
const GUTTER_DRAG_AUTO_SCROLL_EDGE_PX: f64 = 40.0;
const GUTTER_DRAG_AUTO_SCROLL_MAX_STEP_PX: f64 = 24.0;
const GUTTER_DRAG_AUTO_SCROLL_TICK_MS: u64 = 16;

fn gutter_drag_auto_scroll_delta(pointer_y: f64, viewport_height: f64) -> f64 {
    if viewport_height <= GUTTER_DRAG_AUTO_SCROLL_EDGE_PX * 2.0 {
        return 0.0;
    }
    if pointer_y < GUTTER_DRAG_AUTO_SCROLL_EDGE_PX {
        -((GUTTER_DRAG_AUTO_SCROLL_EDGE_PX - pointer_y) / GUTTER_DRAG_AUTO_SCROLL_EDGE_PX)
            .clamp(0.0, 1.0)
            * GUTTER_DRAG_AUTO_SCROLL_MAX_STEP_PX
    } else if pointer_y > viewport_height - GUTTER_DRAG_AUTO_SCROLL_EDGE_PX {
        ((pointer_y - (viewport_height - GUTTER_DRAG_AUTO_SCROLL_EDGE_PX))
            / GUTTER_DRAG_AUTO_SCROLL_EDGE_PX)
            .clamp(0.0, 1.0)
            * GUTTER_DRAG_AUTO_SCROLL_MAX_STEP_PX
    } else {
        0.0
    }
}

pub enum CditorViewState {
    Ready(DocumentRuntime),
    Loading { message: String },
    LoadFailed { message: String },
}

impl CditorViewState {
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready(_))
    }

    pub fn is_loading(&self) -> bool {
        matches!(self, Self::Loading { .. })
    }

    pub fn is_load_failed(&self) -> bool {
        matches!(self, Self::LoadFailed { .. })
    }

    pub fn apply_loaded_runtime(&mut self, runtime: DocumentRuntime) {
        *self = Self::Ready(runtime);
    }

    pub fn apply_load_failed(&mut self, message: impl Into<String>) {
        *self = Self::LoadFailed {
            message: message.into(),
        };
    }
}

impl CditorV2View {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self::from_runtime(DocumentRuntime::demo(), true, cx)
    }

    pub fn from_runtime(
        runtime: DocumentRuntime,
        show_debug: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::from_runtime_with_options(runtime, show_debug, false, cx)
    }

    pub fn from_runtime_with_options(
        runtime: DocumentRuntime,
        show_debug: bool,
        readonly: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::from_runtime_with_postgres_options(runtime, show_debug, readonly, None, cx)
    }

    pub fn from_runtime_with_postgres_options(
        runtime: DocumentRuntime,
        show_debug: bool,
        readonly: bool,
        postgres_target: Option<PostgresPersistenceTarget>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::from_runtime_with_postgres_options_and_autosave(
            runtime,
            show_debug,
            readonly,
            postgres_target,
            None,
            cx,
        )
    }

    pub fn from_runtime_with_postgres_options_and_autosave(
        runtime: DocumentRuntime,
        show_debug: bool,
        readonly: bool,
        postgres_target: Option<PostgresPersistenceTarget>,
        autosave_interval: Option<Duration>,
        cx: &mut Context<Self>,
    ) -> Self {
        let autosave_interval = autosave_interval.unwrap_or(DEFAULT_POSTGRES_SAVE_DEBOUNCE);
        Self {
            state: CditorViewState::Ready(runtime),
            focus: cx.focus_handle(),
            show_debug,
            readonly,
            save_status: save_status_for_mode(readonly),
            last_wheel_delta_y: 0.0,
            scroll_accumulator: ScrollAccumulator::default(),
            text_layouts: HashMap::new(),
            scrollbar_drag: None,
            text_drag_selection: None,
            block_drag_selection: BlockDragSelectionController::default(),
            hovered_block_id: None,
            action_block_id: None,
            gutter_block_drag: None,
            gutter_drag_auto_scroll_scheduled: false,
            image_resize_drag: None,
            projected_block_rects: Vec::new(),
            postgres_persistence: postgres_target
                .map(|target| PostgresPersistenceState::for_target(target, autosave_interval))
                .unwrap_or_else(PostgresPersistenceState::disabled),
            autosave_interval,
        }
    }

    pub fn loading(message: impl Into<String>, show_debug: bool, cx: &mut Context<Self>) -> Self {
        Self::loading_with_options(message, show_debug, false, None, cx)
    }

    pub fn loading_with_options(
        message: impl Into<String>,
        show_debug: bool,
        readonly: bool,
        autosave_interval: Option<Duration>,
        cx: &mut Context<Self>,
    ) -> Self {
        let autosave_interval = autosave_interval.unwrap_or(DEFAULT_POSTGRES_SAVE_DEBOUNCE);
        Self {
            state: CditorViewState::Loading {
                message: message.into(),
            },
            focus: cx.focus_handle(),
            show_debug,
            readonly,
            save_status: save_status_for_mode(readonly),
            last_wheel_delta_y: 0.0,
            scroll_accumulator: ScrollAccumulator::default(),
            text_layouts: HashMap::new(),
            scrollbar_drag: None,
            text_drag_selection: None,
            block_drag_selection: BlockDragSelectionController::default(),
            hovered_block_id: None,
            action_block_id: None,
            gutter_block_drag: None,
            gutter_drag_auto_scroll_scheduled: false,
            image_resize_drag: None,
            projected_block_rects: Vec::new(),
            postgres_persistence: PostgresPersistenceState::disabled(),
            autosave_interval,
        }
    }

    pub fn load_failed(
        message: impl Into<String>,
        show_debug: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::load_failed_with_options(message, show_debug, false, cx)
    }

    pub fn load_failed_with_options(
        message: impl Into<String>,
        show_debug: bool,
        readonly: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            state: CditorViewState::LoadFailed {
                message: message.into(),
            },
            focus: cx.focus_handle(),
            show_debug,
            readonly,
            save_status: save_status_for_mode(readonly),
            last_wheel_delta_y: 0.0,
            scroll_accumulator: ScrollAccumulator::default(),
            text_layouts: HashMap::new(),
            scrollbar_drag: None,
            text_drag_selection: None,
            block_drag_selection: BlockDragSelectionController::default(),
            hovered_block_id: None,
            action_block_id: None,
            gutter_block_drag: None,
            gutter_drag_auto_scroll_scheduled: false,
            image_resize_drag: None,
            projected_block_rects: Vec::new(),
            postgres_persistence: PostgresPersistenceState::disabled(),
            autosave_interval: DEFAULT_POSTGRES_SAVE_DEBOUNCE,
        }
    }

    pub fn apply_loaded_runtime(&mut self, runtime: DocumentRuntime) {
        self.apply_loaded_runtime_with_postgres_target(runtime, None);
    }

    pub fn apply_loaded_runtime_with_postgres_target(
        &mut self,
        runtime: DocumentRuntime,
        postgres_target: Option<PostgresPersistenceTarget>,
    ) {
        self.state.apply_loaded_runtime(runtime);
        self.text_layouts.clear();
        self.text_drag_selection = None;
        self.block_drag_selection = BlockDragSelectionController::default();
        self.hovered_block_id = None;
        self.action_block_id = None;
        self.gutter_block_drag = None;
        self.gutter_drag_auto_scroll_scheduled = false;
        self.image_resize_drag = None;
        self.projected_block_rects.clear();
        self.postgres_persistence
            .set_target(postgres_target, self.autosave_interval);
        if let CditorViewState::Ready(runtime) = &self.state {
            self.postgres_persistence
                .mark_loaded_structure_version(runtime.structure_version());
        }
        self.save_status = save_status_for_mode(self.readonly);
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

    pub fn apply_load_failed(&mut self, message: impl Into<String>) {
        self.state.apply_load_failed(message);
        self.text_layouts.clear();
        self.text_drag_selection = None;
        self.block_drag_selection = BlockDragSelectionController::default();
        self.hovered_block_id = None;
        self.action_block_id = None;
        self.gutter_block_drag = None;
        self.gutter_drag_auto_scroll_scheduled = false;
        self.image_resize_drag = None;
        self.projected_block_rects.clear();
    }

    pub fn view_state(&self) -> &CditorViewState {
        &self.state
    }

    pub fn save_status(&self) -> &EditorSaveStatus {
        &self.save_status
    }

    pub fn apply_save_status(&mut self, status: EditorSaveStatus) {
        self.save_status = status;
    }

    pub(crate) fn mark_dirty(&mut self, cx: &mut Context<Self>) {
        mark_dirty_and_schedule_postgres_save(
            &mut self.postgres_persistence,
            &mut self.save_status,
            cx,
        );
    }

    pub(crate) fn flush_postgres_persistence(&mut self, cx: &mut Context<Self>) {
        if self.readonly {
            return;
        }
        let CditorViewState::Ready(runtime) = &mut self.state else {
            return;
        };
        let Some(batch) = self.postgres_persistence.begin_batch(runtime) else {
            return;
        };
        self.save_status = EditorSaveStatus::Saving;
        let save_task = cx.background_spawn(async move {
            block_on_postgres(save_postgres_batch(batch)).and_then(|result| result)
        });
        cx.spawn(async move |view, cx| match save_task.await {
            Ok(saved_structure_version) => {
                let _ = view.update(cx, |view, cx| {
                    let saved_layout_or_structure = saved_structure_version.is_some();
                    let should_reschedule = view
                        .postgres_persistence
                        .finish_success(saved_structure_version);
                    if saved_layout_or_structure
                        && !should_reschedule
                        && let Some(runtime) = view.ready_runtime()
                    {
                        runtime.mark_layout_saved();
                    }
                    view.save_status = save_status_for_mode(view.readonly);
                    if should_reschedule {
                        view.postgres_persistence.schedule(cx);
                    }
                    cx.notify();
                });
            }
            Err(message) => {
                let _ = view.update(cx, |view, cx| {
                    view.postgres_persistence.finish_failed();
                    view.save_status = EditorSaveStatus::Failed(message);
                    cx.notify();
                });
            }
        })
        .detach();
        cx.notify();
    }

    fn ready_runtime(&mut self) -> Option<&mut DocumentRuntime> {
        match &mut self.state {
            CditorViewState::Ready(runtime) => Some(runtime),
            CditorViewState::Loading { .. } | CditorViewState::LoadFailed { .. } => None,
        }
    }

    fn ready_runtime_ref(&self) -> Option<&DocumentRuntime> {
        match &self.state {
            CditorViewState::Ready(runtime) => Some(runtime),
            CditorViewState::Loading { .. } | CditorViewState::LoadFailed { .. } => None,
        }
    }

    pub(crate) fn focus_block_from_gui_at_position(
        &mut self,
        block_id: crate::core::ids::BlockId,
        position: impl Into<Option<Point<Pixels>>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus, cx);
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
                if runtime.focused_block_id() != Some(block_id) {
                    runtime.focus_block(block_id);
                }
                let anchor_offset = runtime.caret_offset_for_block(block_id).unwrap_or(0);
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

    pub(crate) fn gutter_mouse_down_from_gui(
        &mut self,
        block_id: BlockId,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus, cx);
        self.hovered_block_id = Some(block_id);
        self.action_block_id = Some(block_id);
        self.text_drag_selection = None;
        self.block_drag_selection = BlockDragSelectionController::default();
        self.gutter_block_drag = Some(GutterBlockDragState::new(
            block_id,
            DragPoint::new(f32::from(position.x), f32::from(position.y)),
        ));
        if let CditorViewState::Ready(runtime) = &mut self.state {
            runtime.focus_block(block_id);
        }
        cx.notify();
    }

    fn update_gutter_block_drag(
        &mut self,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(mut drag) = self.gutter_block_drag else {
            return false;
        };
        let point = DragPoint::new(f32::from(position.x), f32::from(position.y));
        let threshold_changed = drag.update_position(point);
        let auto_scrolled = if drag.exceeded_threshold {
            self.apply_gutter_drag_auto_scroll(f64::from(position.y))
        } else {
            false
        };
        self.gutter_block_drag = Some(drag);
        let target_changed = self.refresh_gutter_block_drag_target();
        if self.should_continue_gutter_drag_auto_scroll() {
            self.schedule_gutter_drag_auto_scroll_tick(cx);
        }
        if threshold_changed || target_changed || auto_scrolled {
            cx.notify();
        }
        true
    }

    fn refresh_gutter_block_drag_target(&mut self) -> bool {
        let Some(mut drag) = self.gutter_block_drag else {
            return false;
        };
        let pointer_document_y = f64::from(drag.current_position.y)
            + self
                .ready_runtime_ref()
                .map(|runtime| runtime.scroll.global_scroll_top)
                .unwrap_or(0.0);
        let target = drag
            .exceeded_threshold
            .then(|| self.drop_target_for_document_y(drag.block_id, pointer_document_y))
            .flatten();
        let target_changed = drag.target != target;
        drag.target = target;
        self.gutter_block_drag = Some(drag);
        target_changed
    }

    fn should_continue_gutter_drag_auto_scroll(&self) -> bool {
        let Some(drag) = self.gutter_block_drag else {
            return false;
        };
        if !drag.exceeded_threshold {
            return false;
        }
        let Some(runtime) = self.ready_runtime_ref() else {
            return false;
        };
        gutter_drag_auto_scroll_delta(
            f64::from(drag.current_position.y),
            runtime.scroll.viewport_height,
        )
        .abs()
            >= f64::EPSILON
    }

    fn schedule_gutter_drag_auto_scroll_tick(&mut self, cx: &mut Context<Self>) {
        if self.gutter_drag_auto_scroll_scheduled {
            return;
        }
        self.gutter_drag_auto_scroll_scheduled = true;
        let tick = cx.background_spawn(async move {
            std::thread::sleep(Duration::from_millis(GUTTER_DRAG_AUTO_SCROLL_TICK_MS));
        });
        cx.spawn(async move |view, cx| {
            let _ = tick.await;
            let _ = view.update(cx, |view, cx| {
                view.gutter_drag_auto_scroll_scheduled = false;
                let changed = view.tick_gutter_drag_auto_scroll();
                if changed {
                    cx.notify();
                }
                if view.should_continue_gutter_drag_auto_scroll() {
                    view.schedule_gutter_drag_auto_scroll_tick(cx);
                }
            });
        })
        .detach();
    }

    fn tick_gutter_drag_auto_scroll(&mut self) -> bool {
        let Some(drag) = self.gutter_block_drag else {
            return false;
        };
        if !drag.exceeded_threshold {
            return false;
        }
        let auto_scrolled = self.apply_gutter_drag_auto_scroll(f64::from(drag.current_position.y));
        let target_changed = self.refresh_gutter_block_drag_target();
        auto_scrolled || target_changed
    }

    fn apply_gutter_drag_auto_scroll(&mut self, pointer_y: f64) -> bool {
        let CditorViewState::Ready(runtime) = &mut self.state else {
            return false;
        };
        let delta = gutter_drag_auto_scroll_delta(pointer_y, runtime.scroll.viewport_height);
        if delta.abs() < f64::EPSILON {
            return false;
        }
        let before = runtime.scroll.global_scroll_top;
        runtime.scroll_by_delta(delta).is_ok() && runtime.scroll.global_scroll_top != before
    }

    fn commit_gutter_block_drag(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(drag) = self.gutter_block_drag.take() else {
            self.gutter_drag_auto_scroll_scheduled = false;
            return false;
        };
        self.gutter_drag_auto_scroll_scheduled = false;
        let Some(target) = drag.target.filter(|_| drag.exceeded_threshold) else {
            cx.notify();
            return true;
        };
        let horizontal_delta = drag.current_position.x - drag.start_position.x;
        let parent_target = (horizontal_delta >= crate::gui::block::chrome::BLOCK_INDENT_STEP_PX)
            .then(|| {
                parent_drop_target_from_rects(&self.projected_block_rects, drag.block_id, target)
            })
            .flatten();
        if let CditorViewState::Ready(runtime) = &mut self.state {
            let moved = if let Some(parent_target) = parent_target {
                runtime
                    .move_block_subtree_to_parent(
                        drag.block_id,
                        Some(parent_target.parent_id),
                        parent_target.sibling_index,
                    )
                    .unwrap_or(false)
            } else {
                runtime
                    .move_block_subtree_before(drag.block_id, target.insert_before_block_id)
                    .unwrap_or(false)
            };
            if moved {
                self.mark_dirty(cx);
            }
        }
        cx.notify();
        true
    }

    fn drop_target_for_document_y(
        &self,
        source_block_id: BlockId,
        document_y: f64,
    ) -> Option<BlockDropTarget> {
        drop_target_for_document_y_from_rects(
            &self.projected_block_rects,
            source_block_id,
            document_y,
        )
    }

    fn block_drag_overlay_snapshot(&self) -> Option<BlockDragOverlaySnapshot> {
        let drag = self.gutter_block_drag?;
        let target = drag.target.filter(|_| drag.exceeded_threshold)?;
        let (y_px, indent_px) = match target.insert_before_block_id {
            Some(block_id) => self
                .projected_block_rects
                .iter()
                .find(|rect| rect.block_id == block_id)
                .map(|rect| (rect.document_top as f32, rect.indent_px))?,
            None => self
                .projected_block_rects
                .last()
                .map(|rect| (rect.document_bottom as f32, rect.indent_px))?,
        };
        Some(BlockDragOverlaySnapshot {
            y_px,
            indent_px,
            visible: true,
        })
    }

    fn current_text_layout_cache(
        &self,
        runtime: &DocumentRuntime,
        block_id: BlockId,
    ) -> Option<&RichTextPlatformLayout> {
        let cache = self.text_layouts.get(&block_id)?;
        let current_content_version = runtime.block_content_version(block_id)?;
        (cache.content_version == current_content_version).then_some(cache)
    }

    fn text_offset_for_block_at_position(
        &self,
        block_id: BlockId,
        position: Point<Pixels>,
    ) -> Option<usize> {
        let runtime = self.ready_runtime_ref()?;
        if let Some(cache) = self.current_text_layout_cache(runtime, block_id) {
            return Some(platform_index_for_point(cache, position));
        }
        self.fallback_text_offset_for_block_at_position(runtime, block_id, position)
    }

    fn fallback_text_offset_for_block_at_position(
        &self,
        runtime: &DocumentRuntime,
        block_id: BlockId,
        position: Point<Pixels>,
    ) -> Option<usize> {
        let rect = self
            .projected_block_rects
            .iter()
            .find(|rect| rect.block_id == block_id)?;
        let viewport_origin = self.infer_document_viewport_origin()?;
        let payload = runtime.block_payload_record(block_id)?;
        let spans = match &payload.payload {
            crate::core::rich_text::BlockPayload::RichText { spans } => spans.clone(),
            crate::core::rich_text::BlockPayload::Code { text, .. } => {
                vec![crate::core::rich_text::InlineSpan::plain(text)]
            }
            crate::core::rich_text::BlockPayload::Html { html, .. } => {
                vec![crate::core::rich_text::InlineSpan::plain(html)]
            }
            _ => return Some(0),
        };
        let text = crate::core::rich_text::plain_text_from_spans(&spans);
        if text.is_empty() {
            return Some(0);
        }
        let text_origin_x = viewport_origin.x + rect.text_origin_x_in_block_px;
        let text_origin_y = viewport_origin.y + rect.document_top
            - runtime.scroll.global_scroll_top
            + rect.text_origin_y_in_block_px;
        let input = RichTextLayoutInput {
            block_id,
            content_version: payload.content_version,
            layout_version: 0,
            kind: payload.kind,
            spans,
            width_px: rect.text_width_px,
            theme_version: 1,
            font_version: 1,
        };
        let layout = wrap_rich_text(&input);
        Some(layout.offset_for_point(
            &text,
            crate::gui::text::TextHitPoint {
                x: f32::from(position.x) as f64 - text_origin_x,
                y: f32::from(position.y) as f64 - text_origin_y,
            },
        ))
    }

    fn infer_document_viewport_origin(&self) -> Option<FallbackViewportOrigin> {
        self.text_layouts.iter().find_map(|(block_id, cache)| {
            let rect = self
                .projected_block_rects
                .iter()
                .find(|rect| rect.block_id == *block_id)?;
            let runtime = self.ready_runtime_ref()?;
            if runtime.block_content_version(*block_id)? != cache.content_version {
                return None;
            }
            Some(FallbackViewportOrigin {
                x: f32::from(cache.bounds.left()) as f64 - rect.text_origin_x_in_block_px,
                y: f32::from(cache.bounds.top()) as f64 - rect.document_top
                    + runtime.scroll.global_scroll_top
                    - rect.text_origin_y_in_block_px,
            })
        })
    }

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

    fn update_text_drag_selection(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
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

    fn finish_text_drag_selection(&mut self) {
        self.text_drag_selection = None;
    }

    fn on_scroll_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.last_wheel_delta_y = scroll_delta_y(event);
        if let CditorViewState::Ready(runtime) = &mut self.state {
            let before = runtime.scroll.global_scroll_top;
            let start = Instant::now();
            self.scroll_accumulator.push_input(
                ScrollInput {
                    delta_y: self.last_wheel_delta_y,
                    mode: ScrollDeltaMode::Pixel,
                    phase: scroll_phase_from_touch(event.touch_phase),
                    device: ScrollDevice::Trackpad,
                    timestamp: start,
                },
                runtime.scroll.viewport_height,
            );
            let _ = self.scroll_accumulator.apply_frame(&mut runtime.scroll);
            eprintln!(
                "[cditor][wheel] delta_y={:.2} scroll_top {:.2}->{:.2} interaction={:?} elapsed_ms={:.2}",
                self.last_wheel_delta_y,
                before,
                runtime.scroll.global_scroll_top,
                self.scroll_accumulator.interaction_state,
                start.elapsed().as_secs_f64() * 1000.0
            );
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.key.as_str() == "escape" && close_active_preview_if_escape_enabled(cx) {
            cx.stop_propagation();
            cx.notify();
            return;
        }
        let command = command_for_key_down(event);
        if command.should_stop_propagation() {
            self.apply_input_command(command, cx);
            cx.stop_propagation();
            cx.notify();
        }
    }

    fn on_scrollbar_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let CditorViewState::Ready(runtime) = &mut self.state else {
            return;
        };
        let policy = scrollbar_policy(runtime);
        let visual = runtime.begin_scrollbar_drag(policy);
        if !visual.enabled {
            return;
        }
        let pointer_y = f64::from(event.position.y);
        let inside_thumb =
            visual.thumb_top <= pointer_y && pointer_y <= visual.thumb_top + visual.thumb_height;
        let pointer_y_offset_in_thumb = if inside_thumb {
            (pointer_y - visual.thumb_top).clamp(0.0, visual.thumb_height)
        } else {
            visual.thumb_height / 2.0
        };
        self.scrollbar_drag = Some(GuiScrollbarDrag {
            pointer_y_offset_in_thumb,
        });
        let _ = runtime.drag_scrollbar_to_thumb_top(policy, pointer_y - pointer_y_offset_in_thumb);
        cx.stop_propagation();
        cx.notify();
    }

    fn on_scrollbar_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.dragging() && self.image_resize_drag.is_some() {
            if self.update_image_resize_drag(event.position, cx) {
                cx.stop_propagation();
            }
            return;
        }
        if event.dragging() && self.gutter_block_drag.is_some() {
            if self.update_gutter_block_drag(event.position, cx) {
                cx.stop_propagation();
            }
            return;
        }
        let Some(drag) = self.scrollbar_drag else {
            if event.dragging() {
                if !self.block_drag_selection.is_dragging() {
                    self.update_text_drag_selection(event.position, cx);
                }
            } else {
                self.finish_text_drag_selection();
                self.finish_block_drag_selection();
            }
            return;
        };
        if !event.dragging() {
            self.finish_gui_scrollbar_drag(cx);
            self.finish_text_drag_selection();
            self.finish_block_drag_selection();
            return;
        }
        let CditorViewState::Ready(runtime) = &mut self.state else {
            self.scrollbar_drag = None;
            return;
        };
        let policy = scrollbar_policy(runtime);
        let thumb_top = f64::from(event.position.y) - drag.pointer_y_offset_in_thumb;
        let _ = runtime.drag_scrollbar_to_thumb_top(policy, thumb_top);
        cx.stop_propagation();
        cx.notify();
    }

    fn on_scrollbar_mouse_up(
        &mut self,
        _event: &MouseUpEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.commit_image_resize_drag(cx) {
            cx.stop_propagation();
        }
        if self.commit_gutter_block_drag(cx) {
            cx.stop_propagation();
        }
        self.finish_gui_scrollbar_drag(cx);
        self.finish_text_drag_selection();
        self.finish_block_drag_selection();
    }

    fn finish_gui_scrollbar_drag(&mut self, cx: &mut Context<Self>) {
        if self.scrollbar_drag.take().is_none() {
            return;
        }
        if let CditorViewState::Ready(runtime) = &mut self.state {
            let _ = runtime.finish_scrollbar_drag();
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn finish_block_drag_selection(&mut self) {
        let _ = self.block_drag_selection.finish();
    }

    fn apply_input_command(&mut self, command: GuiInputCommand, cx: &mut Context<Self>) {
        if matches!(command, GuiInputCommand::ToggleDebugOverlay) {
            self.show_debug = !self.show_debug;
            return;
        }
        if self.readonly {
            return;
        }
        let CditorViewState::Ready(runtime) = &mut self.state else {
            return;
        };
        match command {
            GuiInputCommand::Ignore | GuiInputCommand::ToggleDebugOverlay => {}
            GuiInputCommand::SelectAllFocusedText => {
                runtime.select_focused_text_all();
            }
            GuiInputCommand::CopySelection => {
                if let Some(text) = runtime.selected_focused_text() {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }
            }
            GuiInputCommand::CutSelection => {
                if let Some(text) = runtime.selected_focused_text() {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                    let changed = if runtime.has_cross_block_text_selection() {
                        runtime.delete_document_selection().unwrap_or(false)
                    } else {
                        runtime
                            .replace_text_in_focused_range(None, "")
                            .unwrap_or(false)
                    };
                    if changed {
                        self.mark_dirty(cx);
                    }
                }
            }
            GuiInputCommand::PasteClipboard => {
                if let Some(item) = cx.read_from_clipboard() {
                    let changed = if let Some(asset) = image_asset_from_clipboard_item(&item) {
                        runtime
                            .insert_image_asset_after_focused(asset.payload)
                            .is_ok()
                    } else if let Some(text) = item.text() {
                        match runtime.insert_markdown_paste(&text) {
                            Ok(true) => true,
                            Ok(false) | Err(_) => runtime
                                .replace_text_in_focused_range(None, &text)
                                .unwrap_or(false),
                        }
                    } else {
                        false
                    };
                    if changed {
                        self.mark_dirty(cx);
                    }
                }
            }
            GuiInputCommand::UndoFocusedBlock => {
                if runtime.undo_focused_block().is_ok() {
                    self.mark_dirty(cx);
                }
            }
            GuiInputCommand::RedoFocusedBlock => {
                if runtime.redo_focused_block().is_ok() {
                    self.mark_dirty(cx);
                }
            }
            GuiInputCommand::InsertParagraphAfterFocused => {
                if runtime.insert_paragraph_after_focused().is_ok() {
                    self.mark_dirty(cx);
                }
            }
            GuiInputCommand::InsertSoftLineBreak => {
                if runtime.insert_soft_line_break().is_ok() {
                    self.mark_dirty(cx);
                }
            }
            GuiInputCommand::HandleEnter => {
                if runtime.handle_enter().is_ok() {
                    self.mark_dirty(cx);
                }
            }
            GuiInputCommand::IndentBlock => {
                if runtime.indent_focused_block().unwrap_or(false) {
                    self.mark_dirty(cx);
                }
            }
            GuiInputCommand::OutdentBlock => {
                if runtime.outdent_focused_block().unwrap_or(false) {
                    self.mark_dirty(cx);
                }
            }
            GuiInputCommand::InsertSpaceOrMarkdownShortcut => {
                if runtime.insert_space_or_markdown_shortcut().is_ok() {
                    self.mark_dirty(cx);
                }
            }
            GuiInputCommand::DeleteBackward => {
                if runtime.delete_backward().is_ok() {
                    self.mark_dirty(cx);
                }
            }
            GuiInputCommand::DeleteForward => {
                if runtime.delete_forward().is_ok() {
                    self.mark_dirty(cx);
                }
            }
            GuiInputCommand::MoveCaretLeft { extend_selection } => {
                let _ = runtime.move_caret_left(extend_selection);
            }
            GuiInputCommand::MoveCaretRight { extend_selection } => {
                let _ = runtime.move_caret_right(extend_selection);
            }
            GuiInputCommand::MoveCaretUp { extend_selection } => {
                let moved_in_block = move_caret_vertically_in_focused_block(
                    &self.text_layouts,
                    runtime,
                    -1,
                    extend_selection,
                )
                .unwrap_or(false);
                if !moved_in_block {
                    let _ = runtime.move_caret_up(extend_selection);
                }
            }
            GuiInputCommand::MoveCaretDown { extend_selection } => {
                let moved_in_block = move_caret_vertically_in_focused_block(
                    &self.text_layouts,
                    runtime,
                    1,
                    extend_selection,
                )
                .unwrap_or(false);
                if !moved_in_block {
                    let _ = runtime.move_caret_down(extend_selection);
                }
            }
            GuiInputCommand::ToggleBold => {
                if runtime
                    .toggle_inline_mark_on_selection(InlineMark::Bold)
                    .is_ok()
                {
                    self.mark_dirty(cx);
                }
            }
            GuiInputCommand::ToggleItalic => {
                if runtime
                    .toggle_inline_mark_on_selection(InlineMark::Italic)
                    .is_ok()
                {
                    self.mark_dirty(cx);
                }
            }
            GuiInputCommand::ToggleUnderline => {
                if runtime
                    .toggle_inline_mark_on_selection(InlineMark::Underline)
                    .is_ok()
                {
                    self.mark_dirty(cx);
                }
            }
            GuiInputCommand::ToggleInlineCode => {
                if runtime
                    .toggle_inline_mark_on_selection(InlineMark::Code)
                    .is_ok()
                {
                    self.mark_dirty(cx);
                }
            }
            GuiInputCommand::InsertChar(ch) => {
                ensure_runtime_focus_for_insert_char(runtime);
                if runtime.insert_char(ch).is_ok() {
                    self.mark_dirty(cx);
                }
            }
        }
    }
}

fn ensure_runtime_focus_for_insert_char(runtime: &mut DocumentRuntime) {
    if runtime.focused_block_id().is_none()
        && let Some(block_id) = runtime.first_visible_block_id()
    {
        runtime.focus_block(block_id);
    }
}

fn move_caret_vertically_in_focused_block(
    text_layouts: &HashMap<BlockId, RichTextPlatformLayout>,
    runtime: &mut DocumentRuntime,
    direction: i32,
    extend_selection: bool,
) -> Result<bool, String> {
    let Some(block_id) = runtime.focused_block_id() else {
        return Ok(false);
    };
    let Some(cache) = text_layouts.get(&block_id) else {
        return Ok(false);
    };
    let Some(current_content_version) = runtime.block_content_version(block_id) else {
        return Ok(false);
    };
    if cache.content_version != current_content_version {
        return Ok(false);
    }
    let Some(caret) = runtime.caret_offset_for_block(block_id) else {
        return Ok(false);
    };
    let Some(caret_bounds) = platform_range_bounds(cache, caret..caret) else {
        return Ok(false);
    };
    let target_y = if direction < 0 {
        caret_bounds.top() - cache.line_height * 0.5
    } else {
        caret_bounds.top() + cache.line_height * 1.5
    };
    if target_y < cache.bounds.top() || target_y >= cache.bounds.bottom() {
        return Ok(false);
    }
    let target_offset = platform_index_for_point(
        cache,
        Point {
            x: caret_bounds.left(),
            y: target_y,
        },
    );
    runtime.move_focused_caret_to_offset(block_id, target_offset, extend_selection)
}

impl EntityInputHandler for CditorV2View {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let runtime = self.ready_runtime()?;
        let (block_id, text) = runtime.focused_text_for_platform_input()?;
        let range = utf16_range_to_utf8_range(&text, &range_utf16);
        let actual = utf8_range_to_utf16_range(&text, &range);
        actual_range.replace(actual.clone());
        trace_input(
            "text_for_range",
            format_args!(
                "block={block_id} range_utf16={range_utf16:?} utf8_range={range:?} actual_utf16={actual:?} text_len={}",
                text.len()
            ),
        );
        text.get(range).map(ToOwned::to_owned)
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let runtime = self.ready_runtime()?;
        let selection = platform_selected_text_range(runtime);
        trace_input(
            "selected_text_range",
            format_args!(
                "focused={:?} selection={selection:?}",
                runtime.focused_block_id()
            ),
        );
        selection
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        let runtime = self.ready_runtime_ref()?;
        let (block_id, text) = runtime.focused_text_for_platform_input()?;
        let marked = runtime
            .active_composition_marked_range()
            .map(|range| utf8_range_to_utf16_range(&text, &range));
        trace_input(
            "marked_text_range",
            format_args!(
                "block={block_id} marked_utf16={marked:?} text_len={}",
                text.len()
            ),
        );
        marked
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(runtime) = self.ready_runtime() {
            runtime.cancel_composition();
            cx.notify();
        }
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.readonly {
            return;
        }
        let Some(runtime) = self.ready_runtime() else {
            return;
        };
        let focused = runtime.focused_block_id();
        let range = ime_replacement_range(runtime, range_utf16.clone());
        trace_input(
            "replace_text_in_range",
            format_args!(
                "focused={focused:?} range_utf16={range_utf16:?} resolved_utf8={range:?} text_len={}",
                text.len()
            ),
        );
        match runtime.replace_text_in_focused_range(range, text) {
            Ok(true) => {
                self.mark_dirty(cx);
                cx.notify();
            }
            Ok(false) => {}
            Err(error) => {
                self.save_status = EditorSaveStatus::Failed(error);
                cx.notify();
            }
        }
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.readonly {
            return;
        }
        let Some(runtime) = self.ready_runtime() else {
            return;
        };
        let Some(block_id) = runtime.focused_block_id() else {
            return;
        };
        let range_from_ime = ime_replacement_range(runtime, range_utf16.clone());
        let range = range_from_ime
            .clone()
            .unwrap_or_else(|| platform_input_fallback_range(runtime, block_id));
        let selected_range = new_selected_range
            .clone()
            .map(|range| utf16_range_to_utf8_range(new_text, &range));
        trace_input(
            "replace_and_mark_text_in_range",
            format_args!(
                "block={block_id} range_utf16={range_utf16:?} range_from_ime={range_from_ime:?} resolved_utf8={range:?} new_text_len={} new_selected_utf16={new_selected_range:?} selected_utf8={selected_range:?}",
                new_text.len()
            ),
        );
        if runtime
            .begin_or_update_composition_with_selection(block_id, range, new_text, selected_range)
            .is_ok()
        {
            cx.notify();
        }
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let runtime = self.ready_runtime_ref()?;
        let (block_id, text) = runtime.focused_text_for_platform_input()?;
        let range = utf16_range_to_utf8_range(&text, &range_utf16);
        let cache = self.current_text_layout_cache(runtime, block_id)?;
        platform_range_bounds(cache, range).or(Some(Bounds {
            origin: element_bounds.origin,
            size: Size {
                width: px(1.0),
                height: px(24.0),
            },
        }))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let runtime = self.ready_runtime_ref()?;
        let (block_id, text) = runtime.focused_text_for_platform_input()?;
        let cache = self.current_text_layout_cache(runtime, block_id)?;
        let utf8 = platform_index_for_point(cache, point).min(text.len());
        let utf16 = utf8_to_utf16_offset(&text, utf8);
        trace_input(
            "character_index_for_point",
            format_args!(
                "block={block_id} point={point:?} utf8={utf8} utf16={utf16} text_len={}",
                text.len()
            ),
        );
        Some(utf16)
    }

    fn accepts_text_input(&self, _window: &mut Window, _cx: &mut Context<Self>) -> bool {
        !self.readonly && matches!(self.state, CditorViewState::Ready(_))
    }
}

impl Render for CditorV2View {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let render_start = Instant::now();
        let theme = GuiTheme::light();
        let focus = self.focus.clone();
        if !focus.is_focused(window) {
            window.focus(&focus, cx);
        }

        let view = cx.entity();
        let mut root = div()
            .id("cditor-v2-root")
            .relative()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key_down))
            .on_scroll_wheel(cx.listener(Self::on_scroll_wheel))
            .on_mouse_move(cx.listener(Self::on_scrollbar_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_scrollbar_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_scrollbar_mouse_up))
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .bg(rgb(theme.surface))
            .text_color(rgb(theme.text))
            .child(
                div()
                    .flex_none()
                    .px_4()
                    .py_2()
                    .bg(rgb(theme.page))
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child("CDitor V2 Runtime GUI · 输入文本会写入当前 V2 DocumentRuntime · Tab 切换调试信息")
                    .child(render_save_indicator(&self.save_status, theme)),
            );

        match &mut self.state {
            CditorViewState::Ready(runtime) => {
                self.scroll_accumulator.maybe_mark_idle(Instant::now());
                let height_correction_priority = if self.scrollbar_drag.is_some() {
                    HeightCorrectionPriority::DeferUntilIdle
                } else {
                    self.scroll_accumulator.height_correction_priority()
                };
                let flush_start = Instant::now();
                let height_changed = runtime
                    .flush_pending_height_corrections_with_priority(height_correction_priority)
                    .unwrap_or(false);
                let flush_ms = flush_start.elapsed().as_secs_f64() * 1000.0;
                let projection_start = Instant::now();
                let projection = runtime.projection_for_window_planned();
                let focused_block_id = runtime.focused_block_id();
                let scrollbar_policy = scrollbar_policy(runtime);
                let scrollbar_visual = runtime.scrollbar_visual_state(scrollbar_policy);
                let projection_ms = projection_start.elapsed().as_secs_f64() * 1000.0;
                self.projected_block_rects = projected_block_rects_from_projection(&projection);
                let drag_overlay = self.block_drag_overlay_snapshot();
                let block_action = DocumentBlockActionProjection {
                    action_block_id: self.action_block_id,
                    dragging: self
                        .gutter_block_drag
                        .is_some_and(|drag| drag.exceeded_threshold),
                };
                eprintln!(
                    "[cditor][render] scroll_top={:.2} blocks={} window={:?} placeholder={} height_changed={} height_priority={:?} flush_ms={:.2} projection_ms={:.2}",
                    projection.scroll.global_scroll_top,
                    projection.blocks.len(),
                    projection.render_window.block_range,
                    projection.placeholder_window_height.is_some(),
                    height_changed,
                    height_correction_priority,
                    flush_ms,
                    projection_ms
                );
                let document_editor = DocumentEditorView::new(theme);
                let scrollbar_dragging = self.scrollbar_drag.is_some();
                let debug_header = DocumentDebugHeader::from_projection(
                    &projection,
                    self.last_wheel_delta_y,
                    focused_block_id,
                );
                root = root
                    .when(self.show_debug, |this| {
                        this.child(debug_header.render(theme))
                    })
                    .child(document_editor.render(
                        &projection,
                        view,
                        self.focus.clone(),
                        self.hovered_block_id,
                        drag_overlay,
                        block_action,
                        self.image_resize_preview(),
                        cx,
                    ))
                    .child(render_scrollbar(
                        scrollbar_visual,
                        scrollbar_dragging,
                        cx.listener(Self::on_scrollbar_mouse_down),
                    ));
            }
            CditorViewState::Loading { message } => {
                root = root.child(render_load_state(
                    &EditorLoadStateLabel::Loading(message.clone()),
                    theme,
                ));
            }
            CditorViewState::LoadFailed { message } => {
                root = root.child(render_load_state(
                    &EditorLoadStateLabel::Failed(message.clone()),
                    theme,
                ));
            }
        }
        if let Some(preview_overlay) = render_image_preview_overlay(window, cx) {
            root = root.child(preview_overlay);
        }

        let elapsed_ms = render_start.elapsed().as_secs_f64() * 1000.0;
        if elapsed_ms >= 1.0 {
            eprintln!("[cditor][render] total_elapsed_ms={elapsed_ms:.2}");
        }
        root
    }
}

fn scrollbar_policy(runtime: &DocumentRuntime) -> ScrollbarPolicy {
    ScrollbarPolicy {
        track_height: runtime.scroll.viewport_height.max(1.0),
        min_thumb_height: 24.0,
        local_list_state_scrollbar_enabled: false,
    }
}

fn render_scrollbar(
    visual: ScrollbarVisualState,
    dragging: bool,
    on_mouse_down: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    if !visual.enabled {
        return div().into_any_element();
    }

    let thumb_color = if dragging { 0x0969daaa } else { 0x8c959f88 };
    div()
        .absolute()
        .top_0()
        .right(px(GUI_SCROLLBAR_RIGHT_PX))
        .w(px(GUI_SCROLLBAR_WIDTH_PX))
        .h(px(visual.track_height as f32))
        .rounded(px(GUI_SCROLLBAR_WIDTH_PX / 2.0))
        .bg(rgba(0x8c959f22))
        .on_mouse_down(MouseButton::Left, on_mouse_down)
        .child(
            div()
                .absolute()
                .top(px(visual.thumb_top as f32))
                .left(px(0.0))
                .right(px(0.0))
                .h(px(visual.thumb_height as f32))
                .rounded(px(GUI_SCROLLBAR_WIDTH_PX / 2.0))
                .bg(rgba(thumb_color)),
        )
        .into_any_element()
}

fn platform_selected_text_range(runtime: &DocumentRuntime) -> Option<UTF16Selection> {
    let (_block_id, text) = runtime.focused_text_for_platform_input()?;
    if let Some(selection) = runtime.active_composition_selected_range() {
        return Some(UTF16Selection {
            range: utf8_range_to_utf16_range(&text, &selection),
            reversed: false,
        });
    }
    if let Some(marked_range) = runtime.active_composition_marked_range() {
        let caret = utf8_to_utf16_offset(&text, marked_range.end.min(text.len()));
        return Some(UTF16Selection {
            range: caret..caret,
            reversed: false,
        });
    }
    if let Some(selection) = runtime.focused_text_selection_range() {
        return Some(UTF16Selection {
            range: utf8_range_to_utf16_range(&text, &selection),
            reversed: false,
        });
    }
    let caret = runtime
        .editing
        .as_ref()
        .map(|editing| editing.caret_anchor.text_offset as usize)
        .unwrap_or(text.len())
        .min(text.len());
    let caret = utf8_to_utf16_offset(&text, caret);
    Some(UTF16Selection {
        range: caret..caret,
        reversed: false,
    })
}

fn platform_input_fallback_range(runtime: &DocumentRuntime, block_id: BlockId) -> Range<usize> {
    runtime
        .active_composition()
        .filter(|composition| composition.block_id == block_id)
        .map(|composition| composition.range_start as usize..composition.range_end as usize)
        .or_else(|| runtime.focused_text_selection_range())
        .unwrap_or_else(|| {
            let caret = runtime
                .editing
                .as_ref()
                .map(|editing| editing.caret_anchor.text_offset as usize)
                .unwrap_or_else(|| runtime.focused_text().map(str::len).unwrap_or(0));
            caret..caret
        })
}

fn ime_replacement_range(
    runtime: &DocumentRuntime,
    range_utf16: Option<Range<usize>>,
) -> Option<Range<usize>> {
    let range_utf16 = range_utf16?;
    let (_block_id, text) = runtime.focused_text_for_platform_input()?;
    let preview_range = utf16_range_to_utf8_range(&text, &range_utf16);
    let Some(composition) = runtime.active_composition() else {
        return Some(preview_range);
    };
    let preview_marked_range = runtime.active_composition_marked_range()?;
    let base_marked_range = composition.range_start as usize..composition.range_end as usize;
    Some(marked_preview_range_to_base_range(
        preview_range,
        base_marked_range,
        preview_marked_range,
    ))
}

fn save_status_for_mode(readonly: bool) -> EditorSaveStatus {
    if readonly {
        EditorSaveStatus::Readonly
    } else {
        EditorSaveStatus::Clean
    }
}

fn scroll_delta_y(event: &ScrollWheelEvent) -> f64 {
    match event.delta {
        ScrollDelta::Pixels(delta) => -(f32::from(delta.y) as f64),
        ScrollDelta::Lines(delta) => -(delta.y as f64 * 16.0),
    }
}

fn scroll_phase_from_touch(phase: gpui::TouchPhase) -> ScrollPhase {
    match phase {
        gpui::TouchPhase::Started => ScrollPhase::Began,
        gpui::TouchPhase::Moved => ScrollPhase::Changed,
        gpui::TouchPhase::Ended => ScrollPhase::Ended,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::app::interaction::geometry::{
        ParentDropTarget, fallback_text_metrics_for_block,
    };
    use crate::gui::block::code::{V1_CODE_CONTENT_PADDING_TOP_PX, V1_CODE_CONTENT_PADDING_X_PX};

    #[test]
    fn save_status_for_mode_respects_readonly() {
        assert_eq!(save_status_for_mode(false), EditorSaveStatus::Clean);
        assert_eq!(save_status_for_mode(true), EditorSaveStatus::Readonly);
    }

    #[test]
    fn cditor_view_state_can_swap_from_loading_to_ready_or_failed() {
        let mut state = CditorViewState::Loading {
            message: "loading".to_owned(),
        };

        assert!(state.is_loading());
        state.apply_loaded_runtime(DocumentRuntime::demo());
        assert!(state.is_ready());
        state.apply_load_failed("network error");
        assert!(state.is_load_failed());
    }

    #[test]
    fn insert_char_focus_helper_preserves_existing_middle_caret() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![crate::core::rich_text::BlockPayloadRecord::rich_text(
                1,
                crate::core::rich_text::RichBlockKind::Paragraph,
                "abcdef",
            )],
            720.0,
        );
        runtime.focus_block_at_offset(1, 3).unwrap();

        ensure_runtime_focus_for_insert_char(&mut runtime);

        assert_eq!(runtime.focused_block_id(), Some(1));
        assert_eq!(runtime.caret_offset_for_block(1), Some(3));
    }

    #[test]
    fn insert_char_focus_helper_falls_back_only_when_unfocused() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![crate::core::rich_text::BlockPayloadRecord::rich_text(
                1,
                crate::core::rich_text::RichBlockKind::Paragraph,
                "abcdef",
            )],
            720.0,
        );

        ensure_runtime_focus_for_insert_char(&mut runtime);

        assert_eq!(runtime.focused_block_id(), Some(1));
        assert_eq!(runtime.caret_offset_for_block(1), Some("abcdef".len()));
    }

    #[test]
    fn platform_input_fallback_prefers_active_composition_base_range_over_caret() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![crate::core::rich_text::BlockPayloadRecord::rich_text(
                1,
                crate::core::rich_text::RichBlockKind::Paragraph,
                "abcdef",
            )],
            720.0,
        );
        runtime.focus_block_at_offset(1, 3).unwrap();
        runtime
            .begin_or_update_composition_with_selection(1, 3..3, "你", Some("你".len().."你".len()))
            .unwrap();
        assert_eq!(runtime.caret_offset_for_block(1), Some("abc你".len()));

        let fallback = platform_input_fallback_range(&runtime, 1);

        assert_eq!(fallback, 3..3);
    }

    #[test]
    fn platform_selected_text_range_prefers_ime_selected_subrange() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![crate::core::rich_text::BlockPayloadRecord::rich_text(
                1,
                crate::core::rich_text::RichBlockKind::Paragraph,
                "abcd",
            )],
            720.0,
        );
        runtime.focus_block_at_offset(1, 2).unwrap();
        runtime
            .begin_or_update_composition_with_selection(
                1,
                2..2,
                "你好",
                Some("你".len().."你好".len()),
            )
            .unwrap();

        let selection = platform_selected_text_range(&runtime).unwrap();

        assert_eq!(selection.range, 3..4);
        assert!(!selection.reversed);
    }

    #[test]
    fn platform_selected_text_range_uses_marked_end_when_ime_has_no_subrange() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![crate::core::rich_text::BlockPayloadRecord::rich_text(
                1,
                crate::core::rich_text::RichBlockKind::Paragraph,
                "abcd",
            )],
            720.0,
        );
        runtime.focus_block_at_offset(1, 2).unwrap();
        runtime
            .begin_or_update_composition_with_selection(1, 2..2, "你好", None)
            .unwrap();

        let selection = platform_selected_text_range(&runtime).unwrap();

        assert_eq!(selection.range, 4..4);
        assert!(!selection.reversed);
    }

    fn fallback_snapshot(
        kind: crate::core::rich_text::RichBlockKind,
        chrome: crate::core::block::BlockChromeSnapshot,
    ) -> crate::runtime::ViewBlockSnapshot {
        crate::runtime::ViewBlockSnapshot {
            block_id: 1,
            visible_index: 0,
            depth: chrome.list_info.depth as u16,
            chrome,
            kind,
            attrs: crate::core::rich_text::BlockAttrs::default(),
            payload: crate::core::rich_text::BlockPayloadView::Placeholder {
                estimated_height: 32.0,
            },
            layout: crate::core::layout::BlockLayoutMeta::new(1, 32.0),
            selected: false,
            selection_range: None,
            focused: false,
            caret_offset: None,
            marked_range: None,
            pinned: false,
            placeholder: false,
        }
    }

    #[test]
    fn fallback_text_metrics_include_list_prefix_and_indent() {
        let list_block = fallback_snapshot(
            crate::core::rich_text::RichBlockKind::BulletedList,
            crate::core::block::BlockChromeSnapshot {
                list_info: crate::core::block::BlockListInfo::with_depth(2),
                prefix: crate::core::block::BlockPrefixSnapshot::Bullet { depth: 2 },
                has_children: false,
                collapsed: false,
            },
        );

        let metrics = fallback_text_metrics_for_block(&list_block, GuiTheme::light());

        assert!(metrics.origin_x_in_block_px >= 8.0 + 48.0 + 24.0 + 8.0 + 38.0);
        assert!(metrics.width_px > 0.0);
    }

    #[test]
    fn fallback_text_metrics_include_v1_code_content_padding() {
        let code_block = fallback_snapshot(
            crate::core::rich_text::RichBlockKind::Code {
                language: Some("rust".to_owned()),
            },
            crate::core::block::BlockChromeSnapshot::plain(),
        );
        let paragraph = fallback_snapshot(
            crate::core::rich_text::RichBlockKind::Paragraph,
            crate::core::block::BlockChromeSnapshot::plain(),
        );

        let code = fallback_text_metrics_for_block(&code_block, GuiTheme::light());
        let paragraph = fallback_text_metrics_for_block(&paragraph, GuiTheme::light());

        assert_eq!(
            code.origin_y_in_block_px,
            4.0 + 1.0 + f64::from(V1_CODE_CONTENT_PADDING_TOP_PX)
        );
        assert!(
            code.origin_x_in_block_px
                >= paragraph.origin_x_in_block_px + f64::from(V1_CODE_CONTENT_PADDING_X_PX)
        );
    }

    #[test]
    fn gutter_drag_auto_scroll_delta_only_triggers_near_edges() {
        assert_eq!(gutter_drag_auto_scroll_delta(100.0, 400.0), 0.0);
        assert_eq!(gutter_drag_auto_scroll_delta(20.0, 400.0), -12.0);
        assert_eq!(gutter_drag_auto_scroll_delta(380.0, 400.0), 12.0);
        assert_eq!(gutter_drag_auto_scroll_delta(0.0, 400.0), -24.0);
        assert_eq!(gutter_drag_auto_scroll_delta(400.0, 400.0), 24.0);
        assert_eq!(gutter_drag_auto_scroll_delta(10.0, 60.0), 0.0);
    }

    #[test]
    fn gutter_drag_drop_target_uses_midpoints_and_skips_source_subtree() {
        let rects = vec![
            ProjectedBlockRect {
                block_id: 1,
                visible_index: 0,
                depth: 0,
                document_top: 0.0,
                document_bottom: 40.0,
                indent_px: 0.0,
                text_origin_x_in_block_px: 0.0,
                text_origin_y_in_block_px: 0.0,
                text_width_px: 860.0,
                supports_children: true,
            },
            ProjectedBlockRect {
                block_id: 2,
                visible_index: 1,
                depth: 1,
                document_top: 40.0,
                document_bottom: 80.0,
                indent_px: 24.0,
                text_origin_x_in_block_px: 24.0,
                text_origin_y_in_block_px: 0.0,
                text_width_px: 836.0,
                supports_children: false,
            },
            ProjectedBlockRect {
                block_id: 3,
                visible_index: 2,
                depth: 0,
                document_top: 80.0,
                document_bottom: 120.0,
                indent_px: 0.0,
                text_origin_x_in_block_px: 0.0,
                text_origin_y_in_block_px: 0.0,
                text_width_px: 860.0,
                supports_children: false,
            },
        ];

        assert_eq!(
            drop_target_for_document_y_from_rects(&rects, 1, 10.0),
            Some(BlockDropTarget {
                insert_before_block_id: Some(3),
                target_visible_index: 2,
            })
        );
        assert_eq!(
            drop_target_for_document_y_from_rects(&rects, 1, 140.0),
            Some(BlockDropTarget {
                insert_before_block_id: None,
                target_visible_index: 3,
            })
        );
    }

    #[test]
    fn parent_drop_target_uses_previous_supported_block_outside_source_subtree() {
        let rects = vec![
            ProjectedBlockRect {
                block_id: 1,
                visible_index: 0,
                depth: 0,
                document_top: 0.0,
                document_bottom: 40.0,
                indent_px: 0.0,
                text_origin_x_in_block_px: 0.0,
                text_origin_y_in_block_px: 0.0,
                text_width_px: 860.0,
                supports_children: true,
            },
            ProjectedBlockRect {
                block_id: 2,
                visible_index: 1,
                depth: 1,
                document_top: 40.0,
                document_bottom: 80.0,
                indent_px: 24.0,
                text_origin_x_in_block_px: 24.0,
                text_origin_y_in_block_px: 0.0,
                text_width_px: 836.0,
                supports_children: true,
            },
            ProjectedBlockRect {
                block_id: 3,
                visible_index: 2,
                depth: 0,
                document_top: 80.0,
                document_bottom: 120.0,
                indent_px: 0.0,
                text_origin_x_in_block_px: 0.0,
                text_origin_y_in_block_px: 0.0,
                text_width_px: 860.0,
                supports_children: false,
            },
            ProjectedBlockRect {
                block_id: 4,
                visible_index: 3,
                depth: 0,
                document_top: 120.0,
                document_bottom: 160.0,
                indent_px: 0.0,
                text_origin_x_in_block_px: 0.0,
                text_origin_y_in_block_px: 0.0,
                text_width_px: 860.0,
                supports_children: true,
            },
        ];

        assert_eq!(
            parent_drop_target_from_rects(
                &rects,
                1,
                BlockDropTarget {
                    insert_before_block_id: Some(4),
                    target_visible_index: 3,
                },
            ),
            None
        );
        assert_eq!(
            parent_drop_target_from_rects(
                &rects,
                3,
                BlockDropTarget {
                    insert_before_block_id: Some(4),
                    target_visible_index: 3,
                },
            ),
            Some(ParentDropTarget {
                parent_id: 2,
                sibling_index: usize::MAX,
            })
        );
    }

    #[test]
    fn parent_drop_target_computes_direct_child_sibling_index() {
        let rects = vec![
            ProjectedBlockRect {
                block_id: 10,
                visible_index: 0,
                depth: 0,
                document_top: 0.0,
                document_bottom: 40.0,
                indent_px: 0.0,
                text_origin_x_in_block_px: 0.0,
                text_origin_y_in_block_px: 0.0,
                text_width_px: 860.0,
                supports_children: true,
            },
            ProjectedBlockRect {
                block_id: 11,
                visible_index: 1,
                depth: 1,
                document_top: 40.0,
                document_bottom: 80.0,
                indent_px: 24.0,
                text_origin_x_in_block_px: 24.0,
                text_origin_y_in_block_px: 0.0,
                text_width_px: 836.0,
                supports_children: false,
            },
            ProjectedBlockRect {
                block_id: 12,
                visible_index: 2,
                depth: 1,
                document_top: 80.0,
                document_bottom: 120.0,
                indent_px: 24.0,
                text_origin_x_in_block_px: 24.0,
                text_origin_y_in_block_px: 0.0,
                text_width_px: 836.0,
                supports_children: false,
            },
            ProjectedBlockRect {
                block_id: 20,
                visible_index: 3,
                depth: 0,
                document_top: 120.0,
                document_bottom: 160.0,
                indent_px: 0.0,
                text_origin_x_in_block_px: 0.0,
                text_origin_y_in_block_px: 0.0,
                text_width_px: 860.0,
                supports_children: false,
            },
        ];

        assert_eq!(
            parent_drop_target_from_rects(
                &rects,
                20,
                BlockDropTarget {
                    insert_before_block_id: Some(12),
                    target_visible_index: 2,
                },
            ),
            Some(ParentDropTarget {
                parent_id: 10,
                sibling_index: 1,
            })
        );
    }

    #[test]
    fn gui_scroll_delta_pixels_and_lines_are_normalized() {
        let pixel_event = ScrollWheelEvent {
            position: gpui::point(gpui::px(0.0), gpui::px(0.0)),
            delta: ScrollDelta::Pixels(gpui::point(gpui::px(0.0), gpui::px(42.0))),
            modifiers: gpui::Modifiers::default(),
            touch_phase: gpui::TouchPhase::Moved,
        };
        let line_event = ScrollWheelEvent {
            position: gpui::point(gpui::px(0.0), gpui::px(0.0)),
            delta: ScrollDelta::Lines(gpui::point(0.0, 3.0)),
            modifiers: gpui::Modifiers::default(),
            touch_phase: gpui::TouchPhase::Moved,
        };

        assert_eq!(scroll_delta_y(&pixel_event), -42.0);
        assert_eq!(scroll_delta_y(&line_event), -48.0);
    }
}
