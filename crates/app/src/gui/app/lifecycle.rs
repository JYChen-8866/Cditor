use std::{collections::HashMap, time::Duration};

use gpui::Context;

use crate::gui::app::cditor_v2_view::{CditorV2View, CditorViewState, save_status_for_mode};
use crate::gui::input::BlockDragSelectionController;
use crate::gui::persistence::{
    DEFAULT_POSTGRES_SAVE_DEBOUNCE, EditorSaveStatus, PostgresPersistenceState,
    PostgresPersistenceTarget,
};
use cditor_runtime::DocumentRuntime;

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
            code_language_focus: cx.focus_handle(),
            show_debug,
            readonly,
            save_status: save_status_for_mode(readonly),
            last_wheel_delta_y: 0.0,
            scroll_accumulator: Default::default(),
            text_layouts: HashMap::new(),
            table_cell_layouts: HashMap::new(),
            scrollbar_drag: None,
            text_drag_selection: None,
            block_drag_selection: BlockDragSelectionController::default(),
            internal_clipboard: None,
            code_language_edit: None,
            slash_menu: None,
            toast: None,
            selected_table_axis: None,
            selected_table_cell_range: None,
            table_cell_range_drag: None,
            hovered_block_id: None,
            action_block_id: None,
            gutter_block_drag: None,
            gutter_drag_auto_scroll_scheduled: false,
            image_resize_drag: None,
            table_resize_drag: None,
            table_reorder_drag: None,
            projected_block_rects: Vec::new(),
            postgres_persistence: postgres_target
                .map(|target| PostgresPersistenceState::for_target(target, autosave_interval))
                .unwrap_or_else(PostgresPersistenceState::disabled),
            autosave_interval,
            platform_input_target: None,
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
            code_language_focus: cx.focus_handle(),
            show_debug,
            readonly,
            save_status: save_status_for_mode(readonly),
            last_wheel_delta_y: 0.0,
            scroll_accumulator: Default::default(),
            text_layouts: HashMap::new(),
            table_cell_layouts: HashMap::new(),
            scrollbar_drag: None,
            text_drag_selection: None,
            block_drag_selection: BlockDragSelectionController::default(),
            internal_clipboard: None,
            code_language_edit: None,
            slash_menu: None,
            toast: None,
            selected_table_axis: None,
            selected_table_cell_range: None,
            table_cell_range_drag: None,
            hovered_block_id: None,
            action_block_id: None,
            gutter_block_drag: None,
            gutter_drag_auto_scroll_scheduled: false,
            image_resize_drag: None,
            table_resize_drag: None,
            table_reorder_drag: None,
            projected_block_rects: Vec::new(),
            postgres_persistence: PostgresPersistenceState::disabled(),
            autosave_interval,
            platform_input_target: None,
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
            show_debug,
            readonly,
            save_status: save_status_for_mode(readonly),
            last_wheel_delta_y: 0.0,
            scroll_accumulator: Default::default(),
            text_layouts: HashMap::new(),
            table_cell_layouts: HashMap::new(),
            scrollbar_drag: None,
            text_drag_selection: None,
            block_drag_selection: BlockDragSelectionController::default(),
            internal_clipboard: None,
            code_language_edit: None,
            slash_menu: None,
            toast: None,
            selected_table_axis: None,
            selected_table_cell_range: None,
            table_cell_range_drag: None,
            hovered_block_id: None,
            action_block_id: None,
            gutter_block_drag: None,
            gutter_drag_auto_scroll_scheduled: false,
            image_resize_drag: None,
            table_resize_drag: None,
            table_reorder_drag: None,
            projected_block_rects: Vec::new(),
            postgres_persistence: PostgresPersistenceState::disabled(),
            autosave_interval: DEFAULT_POSTGRES_SAVE_DEBOUNCE,
            platform_input_target: None,
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
        self.table_cell_layouts.clear();
        self.text_drag_selection = None;
        self.block_drag_selection = BlockDragSelectionController::default();
        self.internal_clipboard = None;
        self.code_language_edit = None;
        self.slash_menu = None;
        self.toast = None;
        self.selected_table_axis = None;
        self.selected_table_cell_range = None;
        self.table_cell_range_drag = None;
        self.hovered_block_id = None;
        self.action_block_id = None;
        self.gutter_block_drag = None;
        self.gutter_drag_auto_scroll_scheduled = false;
        self.image_resize_drag = None;
        self.table_resize_drag = None;
        self.table_reorder_drag = None;
        self.projected_block_rects.clear();
        self.postgres_persistence
            .set_target(postgres_target, self.autosave_interval);
        if let CditorViewState::Ready(runtime) = &self.state {
            self.postgres_persistence
                .mark_loaded_structure_version(runtime.structure_version());
        }
        self.save_status = save_status_for_mode(self.readonly);
    }

    pub fn apply_load_failed(&mut self, message: impl Into<String>) {
        self.state.apply_load_failed(message);
        self.text_layouts.clear();
        self.table_cell_layouts.clear();
        self.text_drag_selection = None;
        self.block_drag_selection = BlockDragSelectionController::default();
        self.internal_clipboard = None;
        self.code_language_edit = None;
        self.slash_menu = None;
        self.toast = None;
        self.selected_table_axis = None;
        self.selected_table_cell_range = None;
        self.table_cell_range_drag = None;
        self.hovered_block_id = None;
        self.action_block_id = None;
        self.gutter_block_drag = None;
        self.gutter_drag_auto_scroll_scheduled = false;
        self.image_resize_drag = None;
        self.table_reorder_drag = None;
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
}
