use std::time::Duration;

use gpui::{App, Context, Entity};

use cditor_core::ids::BlockId;

use crate::cache::RenderCacheState;
use crate::editor_view::state::{
    EditorDiagnosticsState, EditorStatusUiState, FeatureUiState, FocusUiState, InteractionUiState,
    OverlayUiState, PlatformInputState,
};
use crate::editor_view::{CditorV2View, CditorViewState};
use crate::overlays::table::TableViewportMeasurement;
use crate::persistence::{
    DEFAULT_STORAGE_SAVE_DEBOUNCE, EditorSaveStatus, PersistencePipeline, schedule_storage_autosave,
};
use crate::text::CaretBlink;
use cditor_runtime::DocumentRuntime;
use cditor_session::{EditorSession, EditorSessionHandle};

impl CditorV2View {
    pub(crate) fn set_caret_blink_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.focus
            .caret_blink
            .update(cx, |blink, cx| blink.set_enabled(enabled, cx));
    }

    pub(crate) fn pause_caret_blink(&mut self, cx: &mut Context<Self>) {
        self.focus
            .caret_blink
            .update(cx, |blink, cx| blink.pause(cx));
    }

    pub(crate) fn caret_blink_visible(&self, cx: &App) -> bool {
        self.focus.caret_blink.read(cx).visible()
    }

    pub fn caret_blink_entity(&self) -> &Entity<CaretBlink> {
        &self.focus.caret_blink
    }
    pub(crate) fn ready_session(&self) -> Option<&EditorSessionHandle> {
        match &self.state {
            CditorViewState::Ready(session) => Some(session),
            CditorViewState::Loading { .. } | CditorViewState::LoadFailed { .. } => None,
        }
    }

    fn compose(
        state: CditorViewState,
        show_debug: bool,
        effective_readonly: bool,
        requested_readonly: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            state,
            focus: FocusUiState::new(cx),
            input: PlatformInputState::default(),
            features: FeatureUiState::default(),
            overlay: OverlayUiState::default(),
            diagnostics: EditorDiagnosticsState::new(show_debug),
            status: EditorStatusUiState::new(effective_readonly, requested_readonly),
            interaction: InteractionUiState::default(),
            cache: RenderCacheState::default(),
            scheduling: Default::default(),
        }
    }

    fn reset_document_ui(&mut self) {
        self.focus.reset_session_projection();
        self.input.reset();
        self.interaction.reset();
        self.features.reset_session();
        self.overlay.reset();
        self.cache.reset_session();
        self.scheduling.main_thread.clear();
        self.scheduling.workers = Default::default();
        self.scheduling.layout_correction_frame_scheduled = false;
    }

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
        Self::from_runtime_with_persistence_options(runtime, show_debug, readonly, None, cx)
    }

    pub fn from_runtime_with_persistence_options(
        runtime: DocumentRuntime,
        show_debug: bool,
        readonly: bool,
        persistence: Option<PersistencePipeline>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::from_runtime_with_persistence_options_and_autosave(
            runtime,
            show_debug,
            readonly,
            persistence,
            Some(DEFAULT_STORAGE_SAVE_DEBOUNCE),
            cx,
        )
    }

    pub fn from_runtime_with_persistence_options_and_autosave(
        runtime: DocumentRuntime,
        show_debug: bool,
        readonly: bool,
        persistence: Option<PersistencePipeline>,
        _autosave_interval: Option<Duration>,
        cx: &mut Context<Self>,
    ) -> Self {
        let persistence = persistence.unwrap_or_else(PersistencePipeline::disabled);
        let session = EditorSession::with_persistence(runtime, readonly, persistence).into_handle();
        Self::from_session_with_options(session, show_debug, readonly, cx)
    }

    pub fn from_session_with_options(
        session: EditorSessionHandle,
        show_debug: bool,
        requested_readonly: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        let readonly = session
            .snapshot()
            .map_or(requested_readonly, |snapshot| snapshot.readonly);
        Self::compose(
            CditorViewState::Ready(session),
            show_debug,
            readonly,
            requested_readonly,
            cx,
        )
    }

    pub fn loading(message: impl Into<String>, show_debug: bool, cx: &mut Context<Self>) -> Self {
        Self::loading_with_options(message, show_debug, false, None, cx)
    }

    pub fn loading_with_options(
        message: impl Into<String>,
        show_debug: bool,
        readonly: bool,
        _autosave_interval: Option<Duration>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::compose(
            CditorViewState::Loading {
                message: message.into(),
                progress: None,
            },
            show_debug,
            readonly,
            readonly,
            cx,
        )
    }

    pub fn loading_with_progress_options(
        message: impl Into<String>,
        initial_progress: u8,
        show_debug: bool,
        readonly: bool,
        _autosave_interval: Option<Duration>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::compose(
            CditorViewState::Loading {
                message: message.into(),
                progress: Some(initial_progress.min(100)),
            },
            show_debug,
            readonly,
            readonly,
            cx,
        )
    }

    pub fn apply_load_progress(&mut self, message: impl Into<String>, progress: u8) -> bool {
        self.state.apply_load_progress(message, progress)
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
        Self::compose(
            CditorViewState::LoadFailed {
                message: message.into(),
            },
            show_debug,
            readonly,
            readonly,
            cx,
        )
    }

    pub fn apply_loaded_session(&mut self, session: EditorSessionHandle) {
        let session_readonly = session
            .snapshot()
            .map_or(self.status.requested_readonly, |snapshot| snapshot.readonly);
        self.state.apply_loaded_session(session);
        self.status.reset_for_session(session_readonly);
        self.reset_document_ui();
    }

    pub fn apply_recovered_session(
        &mut self,
        session: EditorSessionHandle,
        recovered_transactions: usize,
        cx: &mut Context<Self>,
    ) {
        self.apply_loaded_session(session);
        if recovered_transactions == 0 || self.status.readonly {
            return;
        }
        self.status.dirty = true;
        self.status.save_status = EditorSaveStatus::DirtyMemory;
        schedule_storage_autosave(self, cx);
    }

    pub fn apply_load_failed(&mut self, message: impl Into<String>) {
        self.state.apply_load_failed(message);
        self.status.reset_after_load_failure();
        self.reset_document_ui();
    }

    /// Return the persistent horizontal `ScrollHandle` for a table block.
    /// The handle is a GPUI adapter; the stable offset lives in table state.
    pub(crate) fn table_scroll_handle(
        &mut self,
        block_id: BlockId,
        offset_x: f32,
    ) -> gpui::ScrollHandle {
        self.interaction
            .table_scroll_state
            .handle(block_id, offset_x)
    }

    pub(crate) fn code_scroll_handle(&mut self, block_id: BlockId) -> gpui::ScrollHandle {
        self.interaction
            .code_scroll_handles
            .entry(block_id)
            .or_default()
            .clone()
    }

    pub(crate) fn request_code_caret_reveal_after_line_break(&mut self, block_id: BlockId) {
        self.interaction
            .code_caret_reveal_after_line_break
            .insert(block_id);
    }

    pub(crate) fn take_code_caret_reveal_after_line_break(&mut self, block_id: BlockId) -> bool {
        self.interaction
            .code_caret_reveal_after_line_break
            .remove(&block_id)
    }

    pub(crate) fn stable_table_viewport_measurement(
        &mut self,
        block_id: BlockId,
        handle: &gpui::ScrollHandle,
    ) -> Option<TableViewportMeasurement> {
        self.interaction
            .table_scroll_state
            .stable_viewport_measurement(block_id, handle)
    }

    pub fn view_state(&self) -> &CditorViewState {
        &self.state
    }

    pub fn save_status(&self) -> &EditorSaveStatus {
        &self.status.save_status
    }

    pub fn apply_save_status(&mut self, status: EditorSaveStatus) {
        self.status.save_status = status;
    }
}
