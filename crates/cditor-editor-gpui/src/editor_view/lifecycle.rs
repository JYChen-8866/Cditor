use std::time::Duration;

use gpui::{App, Context, Entity};

use cditor_core::ids::BlockId;
use cditor_sdk::document::{CloseGuard, HibernationGuard, SaveFailureKind};

use crate::cache::{RenderCacheState, RetiredRenderResources};
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

fn retire_images_after_effect(resources: RetiredRenderResources, cx: &mut App) {
    #[cfg(feature = "gpui-dynamic-image")]
    let has_dynamic_images = !resources.dynamic_images.is_empty();
    #[cfg(not(feature = "gpui-dynamic-image"))]
    let has_dynamic_images = false;
    if resources.images.is_empty() && !has_dynamic_images {
        return;
    }
    cx.defer(move |cx| {
        #[cfg(feature = "gpui-dynamic-image")]
        for image in resources.dynamic_images {
            cx.drop_dynamic_image(image, None);
        }
        for image in resources.images {
            cx.drop_image(image, None);
        }
    });
}

impl CditorV2View {
    /// Applies the host tab's activity state.
    ///
    /// Inactive document tabs keep their model and undo state, but release
    /// render-derived caches that can be rebuilt on the next paint. This keeps
    /// memory proportional to the active viewport instead of the number of
    /// tabs that have been opened during the process lifetime.
    pub fn sdk_set_host_active(&mut self, active: bool, cx: &mut Context<Self>) {
        self.status.host_active = active;
        self.set_caret_blink_enabled(active, cx);
        if let Some(session) = self.ready_session() {
            let _ = session.set_host_active(active);
        }
        if active {
            cx.notify();
            return;
        }

        self.input.reset();
        self.interaction.projected_block_rects.clear();
        self.interaction.projected_table_cells.clear();
        retire_images_after_effect(self.cache.reset_session(), cx);
        self.scheduling.main_thread.clear();
        self.schedule_persistent_payload_cache_trim(cx);
    }

    /// Returns the host-facing facts required for a safe two-phase
    /// hibernation. Any failed synchronous session read is treated as busy;
    /// callers must never infer "clean" from an unavailable snapshot.
    pub fn sdk_hibernation_guard(&self) -> HibernationGuard {
        let ready = self.state.is_ready();
        let close_guard = self.sdk_close_guard();
        let durable_storage = self
            .ready_session()
            .and_then(|session| session.persistence_snapshot().ok())
            .is_some_and(|snapshot| snapshot.enabled);
        let flush_required = durable_storage && !self.status.readonly;
        let (composing, selected, runtime_busy) = match self.ready_session() {
            Some(session) => match session.input_context() {
                Ok(input) => (
                    input.has_pending_composition || input.composition.is_some(),
                    input.has_active_selection,
                    false,
                ),
                Err(_) => (false, false, true),
            },
            None => (false, false, false),
        };
        HibernationGuard {
            ready,
            loading: self.state.is_loading(),
            load_failed: self.state.is_load_failed(),
            durable_storage,
            flush_required,
            host_active: self.status.host_active,
            dirty: close_guard.dirty,
            saving: close_guard.saving,
            conflict: close_guard
                .local_failure
                .as_ref()
                .is_some_and(|failure| failure.kind == SaveFailureKind::Conflict),
            failed_operations: close_guard.failed_operations,
            requires_recovery_export: close_guard.requires_recovery_export,
            can_close_safely: close_guard.can_close_safely,
            composing,
            selected,
            runtime_busy,
            can_hibernate_after_flush: hibernation_guard_allows_release(
                ready,
                durable_storage,
                self.status.host_active,
                &close_guard,
                composing,
                selected,
                runtime_busy,
            ),
        }
    }

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

    /// 插入符的位移补间。绘制阶段只拿得到视图的只读引用，所以补间状态自己
    /// 用 `Cell` 做内部可变。
    pub(crate) fn caret_motion(&self) -> &crate::text::CaretMotion {
        &self.focus.caret_motion
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
        // Hosts normally deactivate an editor before releasing its entity, but
        // the SDK cannot require every embedder to honor that ordering. Keep a
        // final GPUI-context cleanup so stable video slots and fallback image
        // tiles never survive an abruptly released editor.
        cx.on_release(|view, cx| {
            retire_images_after_effect(view.cache.reset_session(), cx);
        })
        .detach();
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
            page_chrome_extras: None,
            embedded_composer: false,
        }
    }

    fn reset_document_ui(&mut self, cx: &mut Context<Self>) {
        self.focus.reset_session_projection();
        self.input.reset();
        self.interaction.reset();
        self.features.reset_session();
        self.overlay.reset();
        retire_images_after_effect(self.cache.reset_session(), cx);
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

    pub fn apply_loaded_session(&mut self, session: EditorSessionHandle, cx: &mut Context<Self>) {
        let session_readonly = session
            .snapshot()
            .map_or(self.status.requested_readonly, |snapshot| snapshot.readonly);
        self.state.apply_loaded_session(session);
        self.status.reset_for_session(session_readonly);
        self.reset_document_ui(cx);
    }

    pub fn apply_recovered_session(
        &mut self,
        session: EditorSessionHandle,
        recovered_transactions: usize,
        cx: &mut Context<Self>,
    ) {
        self.apply_loaded_session(session, cx);
        if recovered_transactions == 0 || self.status.readonly {
            return;
        }
        self.status.dirty = true;
        self.status.save_status = EditorSaveStatus::DirtyMemory;
        schedule_storage_autosave(self, cx);
    }

    pub fn apply_load_failed(&mut self, message: impl Into<String>, cx: &mut Context<Self>) {
        self.state.apply_load_failed(message);
        self.status.reset_after_load_failure();
        self.reset_document_ui(cx);
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

fn hibernation_guard_allows_release(
    ready: bool,
    durable_storage: bool,
    host_active: bool,
    close_guard: &CloseGuard,
    composing: bool,
    selected: bool,
    runtime_busy: bool,
) -> bool {
    ready
        && durable_storage
        && !host_active
        && close_guard.can_close_safely
        && !composing
        && !selected
        && !runtime_busy
}

#[cfg(test)]
mod hibernation_guard_tests {
    use super::hibernation_guard_allows_release;
    use cditor_sdk::document::CloseGuard;

    fn clean_close_guard() -> CloseGuard {
        CloseGuard {
            dirty: false,
            saving: false,
            failed_operations: 0,
            local_failure: None,
            requires_recovery_export: false,
            can_close_safely: true,
        }
    }

    #[test]
    fn only_an_inactive_clean_idle_session_can_be_released() {
        let clean = clean_close_guard();
        assert!(hibernation_guard_allows_release(
            true, true, false, &clean, false, false, false,
        ));
        assert!(!hibernation_guard_allows_release(
            false, true, false, &clean, false, false, false,
        ));
        assert!(!hibernation_guard_allows_release(
            true, false, false, &clean, false, false, false,
        ));
        assert!(!hibernation_guard_allows_release(
            true, true, true, &clean, false, false, false,
        ));
        assert!(!hibernation_guard_allows_release(
            true, true, false, &clean, true, false, false,
        ));
        assert!(!hibernation_guard_allows_release(
            true, true, false, &clean, false, true, false,
        ));
        assert!(!hibernation_guard_allows_release(
            true, true, false, &clean, false, false, true,
        ));

        let dirty = CloseGuard {
            dirty: true,
            can_close_safely: false,
            ..clean
        };
        assert!(!hibernation_guard_allows_release(
            true, true, false, &dirty, false, false, false,
        ));
    }
}
