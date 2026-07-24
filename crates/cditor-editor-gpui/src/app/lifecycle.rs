use std::time::Duration;

use gpui::Context;

use cditor_core::ids::BlockId;

use crate::app::cditor_v2_view::{CditorV2View, CditorViewState};
use crate::app::state::{
    EditorDiagnosticsState, EditorStatusUiState, FeatureUiState, FocusUiState, InteractionUiState,
    OverlayUiState, PlatformInputState, RenderCacheState,
};
use crate::overlay::table::TableViewportMeasurement;
use crate::persistence::{
    DEFAULT_STORAGE_SAVE_DEBOUNCE, EditorSaveStatus, PersistencePipeline, schedule_storage_autosave,
};
use cditor_runtime::DocumentRuntime;
use cditor_session::{EditorSession, EditorSessionHandle};

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
        Self {
            state: CditorViewState::Ready(session),
            focus: FocusUiState::new(cx),
            input: PlatformInputState::default(),
            features: FeatureUiState::default(),
            overlay: OverlayUiState::default(),
            diagnostics: EditorDiagnosticsState::new(show_debug),
            status: EditorStatusUiState::new(readonly, requested_readonly),
            interaction: InteractionUiState::default(),
            cache: RenderCacheState::default(),
        }
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
        Self {
            state: CditorViewState::Loading {
                message: message.into(),
            },
            focus: FocusUiState::new(cx),
            input: PlatformInputState::default(),
            features: FeatureUiState::default(),
            overlay: OverlayUiState::default(),
            diagnostics: EditorDiagnosticsState::new(show_debug),
            status: EditorStatusUiState::new(readonly, readonly),
            interaction: InteractionUiState::default(),
            cache: RenderCacheState::default(),
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
            focus: FocusUiState::new(cx),
            input: PlatformInputState::default(),
            features: FeatureUiState::default(),
            overlay: OverlayUiState::default(),
            diagnostics: EditorDiagnosticsState::new(show_debug),
            status: EditorStatusUiState::new(readonly, readonly),
            interaction: InteractionUiState::default(),
            cache: RenderCacheState::default(),
        }
    }

    pub fn apply_loaded_session(&mut self, session: EditorSessionHandle) {
        let session_readonly = session
            .snapshot()
            .map_or(self.status.requested_readonly, |snapshot| snapshot.readonly);
        self.state.apply_loaded_session(session);
        self.status.reset_for_session(session_readonly);
        self.focus.reset_session_projection();
        self.input.reset();
        self.interaction.reset();
        self.features.reset_session();
        self.overlay.reset();
        self.cache.reset_session();
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
        self.status.save_status = EditorSaveStatus::Dirty;
        schedule_storage_autosave(self, cx);
    }

    pub fn apply_load_failed(&mut self, message: impl Into<String>) {
        self.state.apply_load_failed(message);
        self.status.reset_after_load_failure();
        self.focus.reset_session_projection();
        self.input.reset();
        self.interaction.reset();
        self.features.reset_session();
        self.overlay.reset();
        self.cache.reset_session();
    }

    /// Return the persistent horizontal `ScrollHandle` for a table block.
    /// The handle is a GPUI adapter; the stable offset lives in table state.
    pub(in crate::app) fn table_scroll_handle(
        &mut self,
        block_id: BlockId,
        offset_x: f32,
    ) -> gpui::ScrollHandle {
        self.interaction
            .table_scroll_state
            .handle(block_id, offset_x)
    }

    pub(in crate::app) fn stable_table_viewport_measurement(
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
