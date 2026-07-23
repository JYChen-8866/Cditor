use std::time::Duration;

use gpui::Context;

use cditor_core::ids::BlockId;

use crate::app::cditor_v2_view::ai::default_ai_provider;
use crate::app::cditor_v2_view::{CditorV2View, CditorViewState, save_status_for_mode};
use crate::app::interaction::table_mode::GuiTableInteractionMode;
use crate::block::code::highlight::DEFAULT_CODE_HIGHLIGHT_THEME;
use crate::input::BlockDragSelectionController;
use crate::overlay::table::TableViewportMeasurement;
use crate::persistence::{
    DEFAULT_STORAGE_SAVE_DEBOUNCE, EditorSaveStatus, StoragePersistenceState,
};
use cditor_runtime::DocumentRuntime;
use cditor_storage::StorageSession;

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
        Self::from_runtime_with_storage_options(runtime, show_debug, readonly, None, cx)
    }

    pub fn from_runtime_with_storage_options(
        runtime: DocumentRuntime,
        show_debug: bool,
        readonly: bool,
        storage_session: Option<StorageSession>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::from_runtime_with_storage_options_and_autosave(
            runtime,
            show_debug,
            readonly,
            storage_session,
            Some(DEFAULT_STORAGE_SAVE_DEBOUNCE),
            cx,
        )
    }

    pub fn from_runtime_with_storage_options_and_autosave(
        runtime: DocumentRuntime,
        show_debug: bool,
        readonly: bool,
        storage_session: Option<StorageSession>,
        autosave_interval: Option<Duration>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            state: CditorViewState::Ready(Box::new(runtime)),
            focus: cx.focus_handle(),
            code_language_focus: cx.focus_handle(),
            ai_prompt_focus: cx.focus_handle(),
            ai_provider: default_ai_provider(),
            ai_enabled: true,
            ai_prompt: None,
            ai_preview_scroll_handle: Default::default(),
            show_debug,
            readonly,
            requested_readonly: readonly,
            readonly_reason: None,
            dirty: false,
            sdk_focus_observers_registered: false,
            last_emitted_selection: None,
            save_status: save_status_for_mode(readonly),
            last_wheel_delta_y: 0.0,
            scroll_accumulator: Default::default(),
            editor_viewport_handle: Default::default(),
            text_layouts: crate::app::platform_layout_cache::block_layout_cache(),
            table_cell_layouts: crate::app::platform_layout_cache::table_layout_cache(),
            text_surface_layouts: crate::app::platform_layout_cache::auxiliary_layout_cache(),
            table_scroll_state: Default::default(),
            code_highlights: Default::default(),
            mermaid_renders: Default::default(),
            mermaid_source_blocks: Default::default(),
            whiteboard_thumbnails: Default::default(),
            whiteboard_editor: None,
            scrollbar_drag: None,
            text_drag_selection: None,
            text_drag_auto_scroll_scheduled: false,
            block_drag_selection: BlockDragSelectionController::default(),
            code_language_edit: None,
            code_theme_menu_block_id: None,
            code_highlight_theme: DEFAULT_CODE_HIGHLIGHT_THEME,
            slash_menu: None,
            toast: None,
            table_interaction_mode: GuiTableInteractionMode::Idle,
            table_menu_ui: Default::default(),
            hovered_block_id: None,
            action_block_id: None,
            gutter_toolbar_block_id: None,
            selection_toolbar_delay: Default::default(),
            block_transform_menu_open: false,
            color_menu_open: false,
            color_menu_hover_generation: 0,
            color_menu_scroll_handle: Default::default(),
            last_color_action: None,
            gutter_block_drag: None,
            gutter_drag_auto_scroll_scheduled: false,
            image_resize_drag: None,
            table_resize_drag: None,
            table_reorder_drag: None,
            table_hscroll_drag: None,
            projected_block_rects: Vec::new(),
            storage_persistence: storage_session
                .map(|session| StoragePersistenceState::for_session(session, autosave_interval))
                .unwrap_or_else(StoragePersistenceState::disabled),
            undo_spill_in_flight: false,
            history_hydration_in_flight: None,
            selection_materialization_in_flight: None,
            undo_cleanup_in_flight: false,
            payload_window_load_scheduler: Default::default(),
            autosave_interval,
            platform_input_target: None,
            platform_input_session_identity: None,
            platform_input_layout_identity: None,
            preferred_text_navigation_x: None,
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
        Self {
            state: CditorViewState::Loading {
                message: message.into(),
            },
            focus: cx.focus_handle(),
            code_language_focus: cx.focus_handle(),
            ai_prompt_focus: cx.focus_handle(),
            ai_provider: default_ai_provider(),
            ai_enabled: true,
            ai_prompt: None,
            ai_preview_scroll_handle: Default::default(),
            show_debug,
            readonly,
            requested_readonly: readonly,
            readonly_reason: None,
            dirty: false,
            sdk_focus_observers_registered: false,
            last_emitted_selection: None,
            save_status: save_status_for_mode(readonly),
            last_wheel_delta_y: 0.0,
            scroll_accumulator: Default::default(),
            editor_viewport_handle: Default::default(),
            text_layouts: crate::app::platform_layout_cache::block_layout_cache(),
            table_cell_layouts: crate::app::platform_layout_cache::table_layout_cache(),
            text_surface_layouts: crate::app::platform_layout_cache::auxiliary_layout_cache(),
            table_scroll_state: Default::default(),
            code_highlights: Default::default(),
            mermaid_renders: Default::default(),
            mermaid_source_blocks: Default::default(),
            whiteboard_thumbnails: Default::default(),
            whiteboard_editor: None,
            scrollbar_drag: None,
            text_drag_selection: None,
            text_drag_auto_scroll_scheduled: false,
            block_drag_selection: BlockDragSelectionController::default(),
            code_language_edit: None,
            code_theme_menu_block_id: None,
            code_highlight_theme: DEFAULT_CODE_HIGHLIGHT_THEME,
            slash_menu: None,
            toast: None,
            table_interaction_mode: GuiTableInteractionMode::Idle,
            table_menu_ui: Default::default(),
            hovered_block_id: None,
            action_block_id: None,
            gutter_toolbar_block_id: None,
            selection_toolbar_delay: Default::default(),
            block_transform_menu_open: false,
            color_menu_open: false,
            color_menu_hover_generation: 0,
            color_menu_scroll_handle: Default::default(),
            last_color_action: None,
            gutter_block_drag: None,
            gutter_drag_auto_scroll_scheduled: false,
            image_resize_drag: None,
            table_resize_drag: None,
            table_reorder_drag: None,
            table_hscroll_drag: None,
            projected_block_rects: Vec::new(),
            storage_persistence: StoragePersistenceState::disabled(),
            undo_spill_in_flight: false,
            history_hydration_in_flight: None,
            selection_materialization_in_flight: None,
            undo_cleanup_in_flight: false,
            payload_window_load_scheduler: Default::default(),
            autosave_interval,
            platform_input_target: None,
            platform_input_session_identity: None,
            platform_input_layout_identity: None,
            preferred_text_navigation_x: None,
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
            code_language_focus: cx.focus_handle(),
            ai_prompt_focus: cx.focus_handle(),
            ai_provider: default_ai_provider(),
            ai_enabled: true,
            ai_prompt: None,
            ai_preview_scroll_handle: Default::default(),
            show_debug,
            readonly,
            requested_readonly: readonly,
            readonly_reason: None,
            dirty: false,
            sdk_focus_observers_registered: false,
            last_emitted_selection: None,
            save_status: save_status_for_mode(readonly),
            last_wheel_delta_y: 0.0,
            scroll_accumulator: Default::default(),
            editor_viewport_handle: Default::default(),
            text_layouts: crate::app::platform_layout_cache::block_layout_cache(),
            table_cell_layouts: crate::app::platform_layout_cache::table_layout_cache(),
            text_surface_layouts: crate::app::platform_layout_cache::auxiliary_layout_cache(),
            table_scroll_state: Default::default(),
            code_highlights: Default::default(),
            mermaid_renders: Default::default(),
            mermaid_source_blocks: Default::default(),
            whiteboard_thumbnails: Default::default(),
            whiteboard_editor: None,
            scrollbar_drag: None,
            text_drag_selection: None,
            text_drag_auto_scroll_scheduled: false,
            block_drag_selection: BlockDragSelectionController::default(),
            code_language_edit: None,
            code_theme_menu_block_id: None,
            code_highlight_theme: DEFAULT_CODE_HIGHLIGHT_THEME,
            slash_menu: None,
            toast: None,
            table_interaction_mode: GuiTableInteractionMode::Idle,
            table_menu_ui: Default::default(),
            hovered_block_id: None,
            action_block_id: None,
            gutter_toolbar_block_id: None,
            selection_toolbar_delay: Default::default(),
            block_transform_menu_open: false,
            color_menu_open: false,
            color_menu_hover_generation: 0,
            color_menu_scroll_handle: Default::default(),
            last_color_action: None,
            gutter_block_drag: None,
            gutter_drag_auto_scroll_scheduled: false,
            image_resize_drag: None,
            table_resize_drag: None,
            table_reorder_drag: None,
            table_hscroll_drag: None,
            projected_block_rects: Vec::new(),
            storage_persistence: StoragePersistenceState::disabled(),
            undo_spill_in_flight: false,
            history_hydration_in_flight: None,
            selection_materialization_in_flight: None,
            undo_cleanup_in_flight: false,
            payload_window_load_scheduler: Default::default(),
            autosave_interval: None,
            platform_input_target: None,
            platform_input_session_identity: None,
            platform_input_layout_identity: None,
            preferred_text_navigation_x: None,
        }
    }

    pub fn apply_loaded_runtime(&mut self, runtime: DocumentRuntime) {
        self.apply_loaded_runtime_with_storage(runtime, None);
    }

    pub fn apply_loaded_runtime_with_storage(
        &mut self,
        runtime: DocumentRuntime,
        storage_session: Option<StorageSession>,
    ) {
        self.state.apply_loaded_runtime(runtime);
        self.readonly_reason = None;
        self.readonly = self.requested_readonly;
        self.dirty = false;
        self.last_emitted_selection = None;
        self.text_layouts.clear();
        self.table_cell_layouts.clear();
        self.text_surface_layouts.clear();
        self.table_scroll_state.clear();
        self.code_highlights.clear();
        self.mermaid_renders.clear();
        self.mermaid_source_blocks.clear();
        self.whiteboard_thumbnails.clear();
        self.whiteboard_editor = None;
        self.payload_window_load_scheduler.reset();
        self.selection_materialization_in_flight = None;
        self.text_drag_selection = None;
        self.block_drag_selection = BlockDragSelectionController::default();
        self.code_language_edit = None;
        self.code_theme_menu_block_id = None;
        self.slash_menu = None;
        self.toast = None;
        self.table_interaction_mode = GuiTableInteractionMode::Idle;
        self.table_menu_ui = Default::default();
        self.hovered_block_id = None;
        self.action_block_id = None;
        self.gutter_toolbar_block_id = None;
        self.selection_toolbar_delay = Default::default();
        self.block_transform_menu_open = false;
        self.color_menu_open = false;
        self.gutter_block_drag = None;
        self.gutter_drag_auto_scroll_scheduled = false;
        self.image_resize_drag = None;
        self.table_resize_drag = None;
        self.table_reorder_drag = None;
        self.table_hscroll_drag = None;
        self.projected_block_rects.clear();
        self.storage_persistence
            .set_session(storage_session, self.autosave_interval);
        if let CditorViewState::Ready(runtime) = &self.state {
            self.storage_persistence.mark_loaded_structure_version(
                cditor_session::project_persistence_runtime_snapshot(runtime).structure_version,
            );
        }
        self.save_status = save_status_for_mode(self.readonly);
    }

    pub fn apply_load_failed(&mut self, message: impl Into<String>) {
        self.state.apply_load_failed(message);
        self.readonly_reason = None;
        self.readonly = self.requested_readonly;
        self.dirty = false;
        self.last_emitted_selection = None;
        self.text_layouts.clear();
        self.table_cell_layouts.clear();
        self.text_surface_layouts.clear();
        self.table_scroll_state.clear();
        self.code_highlights.clear();
        self.mermaid_renders.clear();
        self.mermaid_source_blocks.clear();
        self.text_drag_selection = None;
        self.block_drag_selection = BlockDragSelectionController::default();
        self.code_language_edit = None;
        self.code_theme_menu_block_id = None;
        self.slash_menu = None;
        self.toast = None;
        self.table_interaction_mode = GuiTableInteractionMode::Idle;
        self.table_menu_ui = Default::default();
        self.hovered_block_id = None;
        self.action_block_id = None;
        self.gutter_toolbar_block_id = None;
        self.selection_toolbar_delay = Default::default();
        self.block_transform_menu_open = false;
        self.color_menu_open = false;
        self.gutter_block_drag = None;
        self.gutter_drag_auto_scroll_scheduled = false;
        self.image_resize_drag = None;
        self.table_resize_drag = None;
        self.table_reorder_drag = None;
        self.table_hscroll_drag = None;
        self.projected_block_rects.clear();
    }

    /// Return the persistent horizontal `ScrollHandle` for a table block.
    /// The handle is a GPUI adapter; the stable offset lives in table state.
    pub(in crate::app) fn table_scroll_handle(
        &mut self,
        block_id: BlockId,
        offset_x: f32,
    ) -> gpui::ScrollHandle {
        self.table_scroll_state.handle(block_id, offset_x)
    }

    pub(in crate::app) fn stable_table_viewport_measurement(
        &mut self,
        block_id: BlockId,
        handle: &gpui::ScrollHandle,
    ) -> Option<TableViewportMeasurement> {
        self.table_scroll_state
            .stable_viewport_measurement(block_id, handle)
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
}
