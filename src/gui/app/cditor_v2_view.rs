use gpui::prelude::FluentBuilder;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use gpui::{
    AppContext, Context, FocusHandle, InteractiveElement, IntoElement, MouseButton, ParentElement,
    Pixels, Point, Render, Styled, Window, div, rgb,
};

use crate::core::block::GutterBlockDragState;
use crate::core::ids::BlockId;

use crate::editor::scroll::{HeightCorrectionPriority, ScrollAccumulator};
use crate::gui::GuiTheme;
use crate::gui::app::input::text_drag::GuiTextDragSelection;
use crate::gui::app::input_trace::trace_input;
use crate::gui::app::interaction::geometry::{
    FallbackViewportOrigin, ProjectedBlockRect, projected_block_rects_from_projection,
};
use crate::gui::app::interaction::image_resize::GuiImageResizeDrag;
use crate::gui::app::interaction::scrollbar::{
    GuiScrollbarDrag, render_scrollbar, scrollbar_policy,
};

use crate::gui::document::{
    DocumentBlockActionProjection, DocumentDebugHeader, DocumentEditorView,
};
use crate::gui::image_preview::render_image_preview_overlay;
use crate::gui::input::BlockDragSelectionController;

use crate::gui::persistence::{
    DEFAULT_POSTGRES_SAVE_DEBOUNCE, EditorLoadStateLabel, EditorSaveStatus,
    PostgresPersistenceState, PostgresPersistenceTarget, mark_dirty_and_schedule_postgres_save,
    render_load_state, render_save_indicator, save_postgres_batch,
};
use crate::gui::text::{
    RichTextLayoutInput, RichTextPlatformLayout, platform_index_for_point, wrap_rich_text,
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

    pub(in crate::gui::app) fn current_text_layout_cache(
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

fn save_status_for_mode(readonly: bool) -> EditorSaveStatus {
    if readonly {
        EditorSaveStatus::Readonly
    } else {
        EditorSaveStatus::Clean
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::block::BlockDropTarget;
    use crate::gui::app::input::ime::{
        platform_input_fallback_range, platform_selected_text_range,
    };
    use crate::gui::app::input::keyboard::ensure_runtime_focus_for_insert_char;
    use crate::gui::app::input::mouse::scroll_delta_y;
    use crate::gui::app::interaction::geometry::{
        ParentDropTarget, drop_target_for_document_y_from_rects, fallback_text_metrics_for_block,
        parent_drop_target_from_rects,
    };
    use crate::gui::app::interaction::gutter_drag::gutter_drag_auto_scroll_delta;
    use crate::gui::block::code::{V1_CODE_CONTENT_PADDING_TOP_PX, V1_CODE_CONTENT_PADDING_X_PX};
    use gpui::{ScrollDelta, ScrollWheelEvent};

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
