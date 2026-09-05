use gpui::{App, AppContext, Context, EventEmitter, Task, Window};

use crate::CditorViewContract;
use crate::editor_view::CditorV2View;
use crate::persistence::{
    EditorSaveStatus, PersistenceBarrierKind, PersistencePipelineError, schedule_storage_autosave,
};
use cditor_core::edit::{ChangeOrigin, EditTransaction};
use cditor_core::ids::BlockId;
use cditor_core::rich_text::DocumentMetadata;
use cditor_editor_protocol::command::{CditorCommand, CommandOutcome, CommandSource};
use cditor_sdk::diagnostics::{
    CditorDiagnostics, ExactRasterDiagnostics, ImageCacheDiagnostics, MermaidDiagnostics,
    VideoDiagnostics,
};
use cditor_sdk::document::{
    Affinity, CloseGuard, DocumentInfo, DocumentPosition, DocumentSelection, HibernationGuard,
    RecoveryExport, SaveFailure, SaveFailureKind, SaveReport, SaveStatus, ScrollAlignment,
    SearchDecoration, TextOffset, TextStatistics,
};
use cditor_sdk::event::CditorEvent;
use cditor_sdk::{CditorError, command::CommandState};
use cditor_session::{AgentEditOutcome, AgentEditRequest, AgentOutline, AgentOutlineRequest};
use gpui::RenderImage;
use std::sync::Arc;

impl EventEmitter<CditorEvent> for CditorV2View {}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct ResidentMemoryEstimate {
    owned_bytes: u64,
    shared_bytes: u64,
}

impl ResidentMemoryEstimate {
    fn total_bytes(self) -> u64 {
        self.owned_bytes.saturating_add(self.shared_bytes)
    }
}

fn resident_memory_estimate(
    payload_and_undo_bytes: usize,
    owned_layout_bytes: usize,
    shared_layout_bytes: usize,
    exact_raster: &ExactRasterDiagnostics,
    images: &ImageCacheDiagnostics,
    mermaid: &MermaidDiagnostics,
    video: &VideoDiagnostics,
) -> ResidentMemoryEstimate {
    let owned_bytes = u64::try_from(
        payload_and_undo_bytes
            .saturating_add(owned_layout_bytes)
            .saturating_add(mermaid.resident_image_bytes)
            .saturating_add(video.resident_cpu_frame_bytes)
            .saturating_add(video.resident_render_image_bytes),
    )
    .unwrap_or(u64::MAX);
    let shared_bytes = u64::try_from(
        shared_layout_bytes
            .saturating_add(exact_raster.resident_image_bytes)
            .saturating_add(images.resident_decoded_bytes),
    )
    .unwrap_or(u64::MAX);
    ResidentMemoryEstimate {
        owned_bytes,
        shared_bytes,
    }
}

impl CditorViewContract for CditorV2View {
    fn sdk_configure_ai(
        &mut self,
        provider: Option<std::sync::Arc<dyn cditor_ai::AiProvider>>,
        enabled: bool,
    ) {
        CditorV2View::sdk_configure_ai(self, provider, enabled);
    }

    fn sdk_is_ready(&self) -> bool {
        CditorV2View::sdk_is_ready(self)
    }

    fn sdk_is_readonly(&self) -> bool {
        CditorV2View::sdk_is_readonly(self)
    }

    fn sdk_set_readonly(&mut self, readonly: bool, cx: &mut Context<Self>) {
        CditorV2View::sdk_set_readonly(self, readonly, cx);
    }

    fn sdk_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        CditorV2View::sdk_focus(self, window, cx);
    }

    fn sdk_blur(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        CditorV2View::sdk_blur(self, window, cx);
    }

    fn sdk_can_undo(&self) -> bool {
        CditorV2View::sdk_can_undo(self)
    }

    fn sdk_can_redo(&self) -> bool {
        CditorV2View::sdk_can_redo(self)
    }

    fn sdk_undo(&mut self, cx: &mut Context<Self>) -> Result<bool, CditorError> {
        CditorV2View::sdk_undo(self, cx)
    }

    fn sdk_redo(&mut self, cx: &mut Context<Self>) -> Result<bool, CditorError> {
        CditorV2View::sdk_redo(self, cx)
    }

    fn sdk_document_info(&self) -> Option<DocumentInfo> {
        CditorV2View::sdk_document_info(self)
    }

    fn sdk_set_document_name(
        &mut self,
        name: String,
        cx: &mut Context<Self>,
    ) -> Result<bool, CditorError> {
        CditorV2View::sdk_set_document_name(self, name, cx)
    }

    fn sdk_text_statistics(&self) -> Option<TextStatistics> {
        CditorV2View::sdk_text_statistics(self)
    }

    fn sdk_is_dirty(&self) -> bool {
        CditorV2View::sdk_is_dirty(self)
    }

    fn sdk_save_status(&self) -> SaveStatus {
        CditorV2View::sdk_save_status(self)
    }

    fn sdk_close_guard(&self) -> CloseGuard {
        CditorV2View::sdk_close_guard(self)
    }

    fn sdk_hibernation_guard(&self) -> HibernationGuard {
        CditorV2View::sdk_hibernation_guard(self)
    }

    fn sdk_prepare_for_shutdown(&mut self, cx: &mut Context<Self>) -> Result<(), CditorError> {
        CditorV2View::sdk_prepare_for_shutdown(self, cx)
    }

    fn sdk_export_markdown(&self) -> Result<String, CditorError> {
        CditorV2View::sdk_export_markdown(self)
    }

    fn sdk_content_height(&self) -> Result<f64, CditorError> {
        CditorV2View::sdk_content_height(self)
    }

    fn sdk_export_recovery(&self) -> Result<RecoveryExport, CditorError> {
        CditorV2View::sdk_export_recovery(self)
    }

    fn sdk_save(&mut self, cx: &mut Context<Self>) -> Task<Result<SaveReport, CditorError>> {
        CditorV2View::sdk_save(self, cx)
    }

    fn sdk_flush(&mut self, cx: &mut Context<Self>) -> Task<Result<SaveReport, CditorError>> {
        CditorV2View::sdk_flush(self, cx)
    }

    fn sdk_diagnostics(&self) -> Result<CditorDiagnostics, CditorError> {
        CditorV2View::sdk_diagnostics(self)
    }

    fn sdk_selection(&self) -> Option<DocumentSelection> {
        CditorV2View::sdk_selection(self)
    }

    fn sdk_set_selection(
        &mut self,
        selection: DocumentSelection,
        cx: &mut Context<Self>,
    ) -> Result<(), CditorError> {
        CditorV2View::sdk_set_selection(self, selection, cx)
    }

    fn sdk_selected_text(&self) -> Option<String> {
        CditorV2View::sdk_selected_text(self)
    }

    fn sdk_scroll_to_block(
        &mut self,
        block_id: cditor_core::ids::BlockId,
        alignment: ScrollAlignment,
        cx: &mut Context<Self>,
    ) -> Result<(), CditorError> {
        CditorV2View::sdk_scroll_to_block(self, block_id, alignment, cx)
    }

    fn sdk_set_search_decorations(
        &mut self,
        decorations: Vec<SearchDecoration>,
        cx: &mut Context<Self>,
    ) {
        CditorV2View::sdk_set_search_decorations(self, decorations, cx);
    }

    fn sdk_highlight_block(&mut self, block_id: cditor_core::ids::BlockId, cx: &mut Context<Self>) {
        CditorV2View::sdk_highlight_block(self, block_id, cx);
    }

    fn sdk_execute_command(
        &mut self,
        command: CditorCommand,
        cx: &mut Context<Self>,
    ) -> Result<CommandOutcome, CditorError> {
        CditorV2View::sdk_execute_command(self, command, cx)
    }

    fn sdk_command_state(&self, command: &CditorCommand) -> CommandState {
        CditorV2View::sdk_command_state(self, command)
    }

    fn sdk_agent_outline(&self, request: AgentOutlineRequest) -> Result<AgentOutline, CditorError> {
        CditorV2View::sdk_agent_outline(self, request)
    }

    fn sdk_agent_edit(
        &mut self,
        request: AgentEditRequest,
        cx: &mut Context<Self>,
    ) -> Result<AgentEditOutcome, CditorError> {
        CditorV2View::sdk_agent_edit(self, request, cx)
    }
}

impl CditorV2View {
    /// Focuses the reserved document-name block through the normal document
    /// selection/input pipeline. Hosts use this after installing a new page.
    pub fn sdk_focus_document_name(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let block_id = self
            .ready_session()
            .and_then(|session| session.document_title_block_id().ok().flatten());
        let Some(block_id) = block_id else {
            return;
        };
        let position = DocumentPosition {
            block_id,
            offset: TextOffset::Utf8Bytes(0),
            affinity: Affinity::Downstream,
        };
        let _ = self.sdk_set_selection(DocumentSelection::caret(position), cx);
        self.sdk_focus(window, cx);
    }

    pub fn sdk_document_metadata(&self) -> Option<DocumentMetadata> {
        self.ready_session()?.document_metadata().ok()
    }

    pub fn sdk_document_cover_render_image(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<Arc<RenderImage>> {
        let source = match self.sdk_document_metadata()?.cover? {
            cditor_core::rich_text::PageCover::External { url, .. } => url,
            cditor_core::rich_text::PageCover::Asset { asset, .. } => asset.source,
        };
        crate::image_loader::load_host_render_image(
            &source,
            &self.scheduling.workers,
            self.features.asset_provider.clone(),
            cx,
        )
    }

    pub fn sdk_set_search_decorations(
        &mut self,
        decorations: Vec<SearchDecoration>,
        cx: &mut Context<Self>,
    ) {
        self.features.search_decorations.replace(decorations);
        cx.notify();
    }

    pub fn sdk_highlight_block(
        &mut self,
        block_id: cditor_core::ids::BlockId,
        cx: &mut Context<Self>,
    ) {
        self.features
            .search_decorations
            .set_jump_highlight(Some(block_id));
        cx.notify();
    }

    pub fn sdk_agent_outline(
        &self,
        request: AgentOutlineRequest,
    ) -> Result<AgentOutline, CditorError> {
        self.ready_session()
            .ok_or(CditorError::NotReady)?
            .agent_outline(request)
            .map_err(|error| CditorError::Internal(error.to_string()))
    }

    pub fn sdk_agent_edit(
        &mut self,
        request: AgentEditRequest,
        cx: &mut Context<Self>,
    ) -> Result<AgentEditOutcome, CditorError> {
        let outcome = self
            .ready_session()
            .ok_or(CditorError::NotReady)?
            .agent_edit(request)
            .map_err(|error| CditorError::Internal(error.to_string()))?;
        if outcome.changed {
            self.mark_dirty_with_origin(ChangeOrigin::Ai, cx);
        }
        Ok(outcome)
    }

    pub fn apply_remote_transaction(
        &mut self,
        transaction: &EditTransaction,
        cx: &mut Context<Self>,
    ) -> Result<cditor_session::RemoteTransactionSnapshot, CditorError> {
        let applied = self
            .ready_session()
            .ok_or(CditorError::NotReady)?
            .apply_remote_transaction(transaction)
            .map_err(|error| CditorError::Internal(error.to_string()))?;
        let was_dirty = self.status.dirty;
        self.status.dirty = true;
        self.status.save_status = EditorSaveStatus::DirtyMemory;
        if let Some(session) = self.ready_session() {
            let _ = session.mark_persistence_dirty();
        }
        cx.emit(CditorEvent::ContentChanged {
            revision: applied.revision,
            origin: ChangeOrigin::Remote,
        });
        if !was_dirty {
            cx.emit(CditorEvent::DirtyChanged { dirty: true });
        }
        cx.notify();
        Ok(applied)
    }

    pub fn sdk_configure_ai(
        &mut self,
        provider: Option<std::sync::Arc<dyn cditor_ai::AiProvider>>,
        enabled: bool,
    ) {
        if let Some(provider) = provider {
            self.features.ai_provider = provider;
        }
        self.features.ai_enabled = enabled;
    }

    pub fn sdk_configure_asset_provider(
        &mut self,
        provider: Option<std::sync::Arc<dyn cditor_sdk::providers::AssetProvider>>,
    ) {
        self.features.asset_provider = provider;
    }

    pub fn sdk_configure_block_link_provider(
        &mut self,
        provider: Option<
            std::sync::Arc<
                dyn Fn(BlockId) -> cditor_core::internal_link::BlockLinkPresentation + Send + Sync,
            >,
        >,
    ) {
        self.features.block_link_provider = provider;
    }

    pub fn sdk_configure_link_opener(
        &mut self,
        opener: Option<std::sync::Arc<dyn Fn(&str, &mut Window, &mut App) -> bool>>,
    ) {
        self.features.link_opener = opener;
    }

    pub fn sdk_is_ready(&self) -> bool {
        self.state.is_ready()
    }

    pub fn sdk_is_readonly(&self) -> bool {
        self.status.readonly
    }

    pub fn sdk_set_readonly(&mut self, readonly: bool, cx: &mut Context<Self>) {
        self.status.requested_readonly = readonly;
        let effective_readonly = readonly || self.status.readonly_reason.is_some();
        if self.status.readonly == effective_readonly {
            return;
        }
        self.status.readonly = effective_readonly;
        if let Some(session) = self.ready_session() {
            let _ = session.set_readonly(effective_readonly);
        }
        self.status.save_status = if effective_readonly {
            EditorSaveStatus::Readonly
        } else if self.status.dirty {
            EditorSaveStatus::DirtyMemory
        } else {
            EditorSaveStatus::LocallySaved
        };
        if !effective_readonly && self.status.dirty {
            schedule_storage_autosave(self, cx);
        }
        cx.notify();
    }

    pub fn enforce_newer_schema_readonly(&mut self, written_major: u64, supported_major: u32) {
        self.status.readonly_reason = Some(
            crate::editor_view::EditorReadonlyReason::NewerDocumentSchema {
                written_major,
                supported_major,
            },
        );
        self.status.readonly = true;
        if let Some(session) = self.ready_session() {
            let _ = session.set_readonly(true);
        }
        self.status.save_status = EditorSaveStatus::Readonly;
    }

    pub fn enforce_newer_operation_schema_readonly(
        &mut self,
        written_major: u32,
        supported_major: u32,
    ) {
        self.status.readonly_reason = Some(
            crate::editor_view::EditorReadonlyReason::NewerOperationSchema {
                written_major,
                supported_major,
            },
        );
        self.status.readonly = true;
        if let Some(session) = self.ready_session() {
            let _ = session.set_readonly(true);
        }
        self.status.save_status = EditorSaveStatus::Readonly;
    }

    pub fn sdk_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let was_focused = self.focus.editor.is_focused(window);
        if !was_focused {
            window.focus(&self.focus.editor, cx);
        }
    }

    pub fn sdk_blur(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.focus.editor.is_focused(window) {
            window.blur(cx);
        }
    }

    pub fn sdk_can_undo(&self) -> bool {
        self.ready_session().is_some_and(|session| {
            session
                .document_snapshot()
                .is_ok_and(|snapshot| snapshot.can_undo)
        })
    }

    pub fn sdk_can_redo(&self) -> bool {
        self.ready_session().is_some_and(|session| {
            session
                .document_snapshot()
                .is_ok_and(|snapshot| snapshot.can_redo)
        })
    }

    pub fn sdk_undo(&mut self, cx: &mut Context<Self>) -> Result<bool, CditorError> {
        self.dispatch_command(CditorCommand::Undo, CommandSource::Sdk, cx)
            .map(|outcome| outcome.changed())
    }

    pub fn sdk_redo(&mut self, cx: &mut Context<Self>) -> Result<bool, CditorError> {
        self.dispatch_command(CditorCommand::Redo, CommandSource::Sdk, cx)
            .map(|outcome| outcome.changed())
    }

    pub fn sdk_document_info(&self) -> Option<DocumentInfo> {
        let snapshot = self.ready_session()?.document_snapshot().ok()?;
        Some(DocumentInfo {
            document_id: snapshot.document_id,
            name: snapshot.name,
            title: snapshot.title,
            title_from_heading: snapshot.title_from_heading,
            icon: snapshot.icon.clone(),
            revision: snapshot.revision,
            block_count: snapshot.block_count,
            readonly: snapshot.readonly,
        })
    }

    pub fn sdk_set_document_name(
        &mut self,
        name: String,
        cx: &mut Context<Self>,
    ) -> Result<bool, CditorError> {
        let revision = self
            .ready_session()
            .ok_or(CditorError::NotReady)?
            .set_document_name(name.clone())
            .map_err(|error| CditorError::Internal(error.to_string()))?;
        let Some(revision) = revision else {
            return Ok(false);
        };
        cx.emit(CditorEvent::DocumentNameChanged { name, revision });
        cx.notify();
        Ok(true)
    }

    pub fn sdk_text_statistics(&self) -> Option<TextStatistics> {
        let (word_count, line_count) = self.ready_session()?.text_statistics().ok()?;
        Some(TextStatistics {
            word_count,
            line_count,
        })
    }

    pub fn sdk_is_dirty(&self) -> bool {
        self.status.dirty
    }

    pub fn sdk_save_status(&self) -> SaveStatus {
        match &self.status.save_status {
            EditorSaveStatus::LocallySaved => SaveStatus::LocallySaved,
            EditorSaveStatus::DirtyMemory => SaveStatus::DirtyMemory,
            EditorSaveStatus::SavingLocal => SaveStatus::SavingLocal,
            EditorSaveStatus::Syncing => SaveStatus::Syncing,
            EditorSaveStatus::Synced => SaveStatus::Synced,
            EditorSaveStatus::FailedLocal(failure) => {
                SaveStatus::FailedLocal(sdk_save_failure(failure))
            }
            EditorSaveStatus::Failed(message) => SaveStatus::Failed(message.clone()),
            EditorSaveStatus::Readonly => SaveStatus::Readonly,
        }
    }

    pub fn sdk_close_guard(&self) -> CloseGuard {
        let saving = self
            .ready_session()
            .and_then(|session| session.persistence_snapshot().ok())
            .is_some_and(|snapshot| snapshot.saving);
        let local_failure = match &self.status.save_status {
            EditorSaveStatus::FailedLocal(failure) => Some(sdk_save_failure(failure)),
            _ => None,
        };
        let failed_operations = usize::from(matches!(
            self.status.save_status,
            EditorSaveStatus::FailedLocal(_) | EditorSaveStatus::Failed(_)
        ));
        let requires_recovery_export = local_failure
            .as_ref()
            .is_some_and(|failure| failure.requires_recovery_export);
        CloseGuard {
            dirty: self.status.dirty,
            saving,
            failed_operations,
            local_failure,
            requires_recovery_export,
            can_close_safely: !self.status.dirty && !saving && failed_operations == 0,
        }
    }

    /// Commits any provisional document IME composition before a host starts
    /// its final persistence barrier. A failed identity check is fail-closed:
    /// the host must keep the process alive or export recovery instead of
    /// silently discarding text that was still visible to the user.
    pub fn sdk_prepare_for_shutdown(&mut self, cx: &mut Context<Self>) -> Result<(), CditorError> {
        self.commit_document_composition_before_external_focus(cx)
            .then_some(())
            .ok_or_else(|| {
                CditorError::Internal(
                    "could not commit the active IME composition before shutdown".to_owned(),
                )
            })
    }

    pub fn sdk_export_recovery(&self) -> Result<RecoveryExport, CditorError> {
        let artifact = self
            .ready_session()
            .ok_or(CditorError::NotReady)?
            .export_emergency_recovery()
            .map_err(CditorError::Export)?;
        Ok(RecoveryExport {
            document_id: artifact.document_id,
            revision: artifact.revision,
            transaction_count: artifact.transaction_count,
            suggested_file_name: artifact.suggested_file_name,
            media_type: "application/vnd.cditor.recovery+json",
            bytes: artifact.bytes,
        })
    }

    /// Exports the current in-memory document as GitHub-Flavored Markdown.
    pub fn sdk_export_markdown(&self) -> Result<String, CditorError> {
        self.ready_session()
            .ok_or(CditorError::NotReady)?
            .export_markdown()
            .map_err(|error| CditorError::Export(error.to_string()))
    }

    /// Returns the editor's current laid-out content height in logical pixels.
    ///
    /// Unlike estimating from exported text, this includes soft wrapping and
    /// the measured heights of non-paragraph blocks.
    pub fn sdk_content_height(&self) -> Result<f64, CditorError> {
        self.ready_session()
            .ok_or(CditorError::NotReady)?
            .ui_snapshot()
            .map(|snapshot| snapshot.content_height)
            .map_err(|error| CditorError::Internal(error.to_string()))
    }

    pub fn sdk_save(&mut self, cx: &mut Context<Self>) -> Task<Result<SaveReport, CditorError>> {
        self.sdk_persistence_barrier(PersistenceBarrierKind::Save, cx)
    }

    pub fn sdk_flush(&mut self, cx: &mut Context<Self>) -> Task<Result<SaveReport, CditorError>> {
        self.sdk_persistence_barrier(PersistenceBarrierKind::Flush, cx)
    }

    fn sdk_persistence_barrier(
        &mut self,
        kind: PersistenceBarrierKind,
        cx: &mut Context<Self>,
    ) -> Task<Result<SaveReport, CditorError>> {
        if self.status.readonly {
            return Task::ready(Err(CditorError::Readonly));
        }
        let Some(revision) = self
            .ready_session()
            .and_then(|session| session.document_snapshot().ok())
            .map(|snapshot| snapshot.revision)
        else {
            return Task::ready(Err(CditorError::NotReady));
        };
        let Some(session) = self.ready_session().cloned() else {
            return Task::ready(Err(CditorError::NotReady));
        };
        if !session
            .persistence_snapshot()
            .is_ok_and(|snapshot| snapshot.enabled)
        {
            return Task::ready(Err(CditorError::Unsupported(
                "save and flush require a persistent storage backend".to_owned(),
            )));
        }

        let receiver = match session.request_persistence_barrier(kind, revision) {
            Ok(receiver) => receiver,
            Err(error) => return Task::ready(Err(CditorError::Internal(error.to_string()))),
        };
        self.flush_storage_persistence(cx);
        cx.background_spawn(async move {
            match receiver.await {
                Ok(Ok(report)) => Ok(SaveReport {
                    revision: report.revision,
                    saved_blocks: report.saved_blocks,
                    duration: report.duration,
                }),
                Ok(Err(error)) => Err(persistence_pipeline_error(error)),
                Err(_) => Err(CditorError::ComponentDropped),
            }
        })
    }

    pub fn sdk_diagnostics(&self) -> Result<CditorDiagnostics, CditorError> {
        let diagnostics = self
            .ready_session()
            .and_then(|session| session.diagnostics_snapshot().ok())
            .ok_or(CditorError::NotReady)?;
        let images = crate::image_loader::image_cache_diagnostics();
        let mermaid = self.cache.mermaid_renders.diagnostics();
        let video = self.cache.video_playbacks.diagnostics();
        let exact_raster_stats = crate::text::exact_raster_cache_stats();
        let exact_raster = ExactRasterDiagnostics {
            entries: exact_raster_stats.entries,
            resident_image_bytes: exact_raster_stats.estimated_bytes,
            max_entries: exact_raster_stats.max_entries,
            image_byte_budget: exact_raster_stats.max_bytes,
            hits: exact_raster_stats.hits,
            misses: exact_raster_stats.misses,
            evictions: exact_raster_stats.evictions,
        };
        let shared_layout_bytes = crate::text::text_layout_cache_stats().estimated_bytes;
        let owned_layout_bytes = self
            .cache
            .text_layouts
            .estimated_metadata_bytes()
            .saturating_add(self.cache.table_cell_layouts.estimated_metadata_bytes())
            .saturating_add(self.cache.text_surface_layouts.estimated_metadata_bytes());
        let memory = resident_memory_estimate(
            diagnostics.payload_and_undo_bytes,
            owned_layout_bytes,
            shared_layout_bytes,
            &exact_raster,
            &images,
            &mermaid,
            &video,
        );
        Ok(CditorDiagnostics {
            storage_backend: self
                .ready_session()
                .and_then(|session| session.persistence_snapshot().ok())
                .and_then(|snapshot| snapshot.backend),
            document_blocks: diagnostics.document_blocks,
            loaded_payloads: diagnostics.loaded_payloads,
            rendered_blocks: self.interaction.projected_block_rects.len(),
            pending_layout_tasks: diagnostics.pending_layout_tasks,
            pending_saves: self
                .ready_session()
                .and_then(|session| session.persistence_snapshot().ok())
                .map_or(0, |snapshot| snapshot.pending_operations),
            dirty_blocks: diagnostics.dirty_payloads,
            estimated_document_height: diagnostics.estimated_document_height,
            memory_estimate_bytes: memory.total_bytes(),
            owned_memory_estimate_bytes: memory.owned_bytes,
            shared_memory_estimate_bytes: memory.shared_bytes,
            exact_raster,
            images,
            mermaid,
            video,
        })
    }

    pub fn sdk_selection(&self) -> Option<DocumentSelection> {
        let selection = self.ready_session()?.document_snapshot().ok()?.selection?;
        Some(DocumentSelection {
            anchor: sdk_position(selection.anchor),
            head: sdk_position(selection.focus),
        })
    }

    pub fn sdk_set_selection(
        &mut self,
        selection: DocumentSelection,
        cx: &mut Context<Self>,
    ) -> Result<(), CditorError> {
        let session = self.ready_session().ok_or(CditorError::NotReady)?;
        let anchor = session_position(session, selection.anchor)?;
        let focus = session_position(session, selection.head)?;
        session
            .dispatch(cditor_editor_protocol::command::CommandEnvelope::new(
                CditorCommand::SetDocumentSelection {
                    selection: cditor_core::edit::DocumentSelection { anchor, focus },
                },
                CommandSource::Sdk,
            ))
            .map_err(|_| CditorError::InvalidSelection)?;
        let applied = self.sdk_selection().ok_or(CditorError::InvalidSelection)?;
        self.focus.last_emitted_selection = Some(applied);
        cx.emit(CditorEvent::SelectionChanged { selection: applied });
        cx.notify();
        Ok(())
    }

    pub fn sdk_selected_text(&self) -> Option<String> {
        self.ready_session()?.selected_text().ok().flatten()
    }

    pub fn sdk_scroll_to_block(
        &mut self,
        block_id: cditor_core::ids::BlockId,
        alignment: ScrollAlignment,
        cx: &mut Context<Self>,
    ) -> Result<(), CditorError> {
        let alignment = match alignment {
            ScrollAlignment::Start => Some(0.0),
            ScrollAlignment::Center => Some(0.5),
            ScrollAlignment::End => Some(1.0),
            ScrollAlignment::Nearest => None,
        };
        self.ready_session()
            .ok_or(CditorError::NotReady)?
            .scroll_to_block(block_id, alignment)
            .map_err(|_| CditorError::BlockNotFound(block_id))?;
        cx.notify();
        Ok(())
    }

    pub(in crate::app) fn execute_sdk_command_handler(
        &mut self,
        command: CditorCommand,
        _source: CommandSource,
        _cx: &mut Context<Self>,
    ) -> Result<CommandOutcome, CditorError> {
        Err(CditorError::Unsupported(format!(
            "command {} is not connected to the SDK command router yet",
            command.stable_id()
        )))
    }

    pub(crate) fn sdk_register_focus_observers(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.focus.sdk_observers_registered {
            return;
        }
        self.focus.sdk_observers_registered = true;
        let focus = self.focus.editor.clone();
        let initially_focused = focus.is_focused(window);
        cx.on_focus(&focus, window, |_, _, cx| {
            cx.emit(CditorEvent::FocusChanged { focused: true });
        })
        .detach();
        cx.on_blur(&focus, window, |_, _, cx| {
            cx.emit(CditorEvent::FocusChanged { focused: false });
        })
        .detach();
        if initially_focused {
            cx.emit(CditorEvent::FocusChanged { focused: true });
        }
    }

    pub(crate) fn sdk_emit_selection_if_changed(&mut self, cx: &mut Context<Self>) {
        let selection = self.sdk_selection();
        if selection == self.focus.last_emitted_selection {
            return;
        }
        self.focus.last_emitted_selection = selection;
        if let Some(selection) = selection {
            cx.emit(CditorEvent::SelectionChanged { selection });
        }
    }
}

fn persistence_pipeline_error(error: PersistencePipelineError) -> CditorError {
    match error {
        PersistencePipelineError::Cancelled => CditorError::Cancelled,
        PersistencePipelineError::Unavailable(message) => CditorError::Unsupported(message),
        PersistencePipelineError::Storage(failure) => CditorError::Persistence(failure.to_string()),
    }
}

fn sdk_save_failure(failure: &cditor_session::PersistenceFailure) -> SaveFailure {
    SaveFailure {
        kind: match failure.kind {
            cditor_session::PersistenceFailureKind::Busy => SaveFailureKind::Busy,
            cditor_session::PersistenceFailureKind::CapacityExhausted => {
                SaveFailureKind::CapacityExhausted
            }
            cditor_session::PersistenceFailureKind::PermissionDenied => {
                SaveFailureKind::PermissionDenied
            }
            cditor_session::PersistenceFailureKind::Conflict => SaveFailureKind::Conflict,
            cditor_session::PersistenceFailureKind::Corruption => SaveFailureKind::Corruption,
            cditor_session::PersistenceFailureKind::Timeout => SaveFailureKind::Timeout,
            cditor_session::PersistenceFailureKind::Io => SaveFailureKind::Io,
            cditor_session::PersistenceFailureKind::Other => SaveFailureKind::Other,
        },
        message: failure.message.clone(),
        retryable: failure.retryable(),
        requires_recovery_export: failure.requires_recovery_export(),
    }
}

fn sdk_position(position: cditor_core::edit::TextPosition) -> DocumentPosition {
    DocumentPosition {
        block_id: position.block_id,
        offset: TextOffset::Utf8Bytes(position.offset),
        affinity: match position.affinity {
            cditor_core::edit::TextAffinity::Upstream => Affinity::Upstream,
            cditor_core::edit::TextAffinity::Downstream => Affinity::Downstream,
        },
    }
}

fn session_position(
    session: &cditor_session::EditorSessionHandle,
    position: DocumentPosition,
) -> Result<cditor_core::edit::TextPosition, CditorError> {
    let text = session
        .loaded_payload_record(position.block_id)
        .map_err(|_| CditorError::NotReady)?
        .ok_or(CditorError::BlockNotFound(position.block_id))?
        .plain_text();
    let offset = match position.offset {
        TextOffset::Utf8Bytes(offset) => {
            if offset > text.len() || !text.is_char_boundary(offset) {
                return Err(CditorError::InvalidSelection);
            }
            offset
        }
        TextOffset::Utf16CodeUnits(offset) => {
            cditor_core::edit::TextOffsetMap::build(&text)
                .utf16_to_internal(cditor_core::edit::PlatformUtf16Offset(offset))
                .map_err(|_| CditorError::InvalidSelection)?
                .0
        }
    };
    Ok(cditor_core::edit::TextPosition {
        block_id: position.block_id,
        offset,
        affinity: match position.affinity {
            Affinity::Upstream => cditor_core::edit::TextAffinity::Upstream,
            Affinity::Downstream => cditor_core::edit::TextAffinity::Downstream,
        },
    })
}

#[cfg(test)]
mod tests {
    use gpui::{AppContext, Entity, Subscription, TestAppContext};

    use super::*;

    struct EventLog {
        events: Vec<CditorEvent>,
        _subscription: Subscription,
    }

    #[gpui::test]
    fn content_events_have_monotonic_revisions_and_coalesced_dirty_state(cx: &mut TestAppContext) {
        let view = cx.new(|cx| {
            CditorV2View::from_runtime_with_options(
                cditor_runtime::DocumentRuntime::empty(),
                false,
                false,
                cx,
            )
        });
        let log: Entity<EventLog> = cx.new(|cx| EventLog {
            events: Vec::new(),
            _subscription: cx.subscribe(&view, |log: &mut EventLog, _, event: &CditorEvent, _| {
                log.events.push(event.clone());
            }),
        });

        view.update(cx, |view, cx| view.mark_dirty(cx));
        view.update(cx, |view, cx| view.mark_dirty(cx));

        let events = log.read_with(cx, |log, _| log.events.clone());
        let revisions = events
            .iter()
            .filter_map(|event| match event {
                CditorEvent::ContentChanged { revision, .. } => Some(*revision),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(revisions.len(), 2);
        assert_eq!(revisions[1], revisions[0] + 1);
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, CditorEvent::DirtyChanged { dirty: true }))
                .count(),
            1
        );
    }

    #[gpui::test]
    fn sdk_document_name_focus_uses_the_document_selection_pipeline(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|_, cx| {
            CditorV2View::from_runtime_with_options(
                cditor_runtime::DocumentRuntime::empty(),
                false,
                false,
                cx,
            )
        });

        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.sdk_focus_document_name(window, cx));
        });

        cx.update(|window, cx| {
            let view = view.read(cx);
            let selection = view.sdk_selection().expect("document name selection");
            assert_eq!(selection.head.block_id, 2);
            assert_eq!(selection.head.offset, TextOffset::Utf8Bytes(0));
            assert!(view.focus.editor.is_focused(window));
        });
    }

    #[gpui::test]
    fn sdk_document_info_exposes_title_and_icon_changes(cx: &mut TestAppContext) {
        let view = cx.new(|cx| {
            CditorV2View::from_runtime_with_options(
                cditor_runtime::DocumentRuntime::from_payloads(
                    1,
                    vec![cditor_core::rich_text::BlockPayloadRecord::rich_text(
                        1,
                        cditor_core::rich_text::RichBlockKind::Heading { level: 1 },
                        "My Doc",
                    )],
                    720.0,
                ),
                false,
                false,
                cx,
            )
        });

        view.update(cx, |view, _| {
            let info = view.sdk_document_info().unwrap();
            assert_eq!(info.title.as_deref(), Some("My Doc"));
            assert_eq!(info.icon, None);
            assert!(info.title_from_heading);
        });

        view.update(cx, |view, cx| {
            view.dispatch_command(
                cditor_editor_protocol::command::EditorCommand::SetPageIconEmoji {
                    emoji: Some("😀".to_owned()),
                },
                cditor_editor_protocol::command::CommandSource::Toolbar,
                cx,
            )
            .unwrap();
        });

        view.update(cx, |view, _| {
            let info = view.sdk_document_info().unwrap();
            assert_eq!(
                info.icon,
                Some(cditor_core::rich_text::PageIcon::Emoji {
                    emoji: "😀".to_owned()
                })
            );
            assert_eq!(info.title.as_deref(), Some("My Doc"));
        });
    }

    #[test]
    fn utf16_sdk_offsets_reject_surrogate_splits() {
        let runtime = cditor_runtime::DocumentRuntime::from_payloads(
            1,
            vec![cditor_core::rich_text::BlockPayloadRecord::rich_text(
                1,
                cditor_core::rich_text::RichBlockKind::Paragraph,
                "A😀中",
            )],
            720.0,
        );
        let session = cditor_session::EditorSession::new(runtime, false).into_handle();
        let position = |offset| DocumentPosition {
            block_id: 1,
            offset: TextOffset::Utf16CodeUnits(offset),
            affinity: Affinity::Downstream,
        };

        assert_eq!(session_position(&session, position(3)).unwrap().offset, 5);
        assert_eq!(
            session_position(&session, position(2)),
            Err(CditorError::InvalidSelection)
        );
    }

    #[test]
    fn persistence_failure_kinds_round_trip_to_the_public_sdk() {
        let cases = [
            (
                cditor_session::PersistenceFailureKind::Busy,
                SaveFailureKind::Busy,
            ),
            (
                cditor_session::PersistenceFailureKind::CapacityExhausted,
                SaveFailureKind::CapacityExhausted,
            ),
            (
                cditor_session::PersistenceFailureKind::PermissionDenied,
                SaveFailureKind::PermissionDenied,
            ),
            (
                cditor_session::PersistenceFailureKind::Conflict,
                SaveFailureKind::Conflict,
            ),
            (
                cditor_session::PersistenceFailureKind::Corruption,
                SaveFailureKind::Corruption,
            ),
            (
                cditor_session::PersistenceFailureKind::Timeout,
                SaveFailureKind::Timeout,
            ),
            (
                cditor_session::PersistenceFailureKind::Io,
                SaveFailureKind::Io,
            ),
            (
                cditor_session::PersistenceFailureKind::Other,
                SaveFailureKind::Other,
            ),
        ];
        for (session_kind, sdk_kind) in cases {
            let failure = sdk_save_failure(&cditor_session::PersistenceFailure::new(
                session_kind,
                "failure",
            ));
            assert_eq!(failure.kind, sdk_kind);
            assert!(failure.requires_recovery_export);
        }
    }

    #[test]
    fn memory_estimate_separates_editor_owned_and_shared_resident_bytes() {
        let exact_raster = ExactRasterDiagnostics {
            resident_image_bytes: 25,
            image_byte_budget: usize::MAX,
            ..Default::default()
        };
        let images = ImageCacheDiagnostics {
            resident_decoded_bytes: 30,
            decoded_byte_budget: usize::MAX,
            ..Default::default()
        };
        let mermaid = MermaidDiagnostics {
            resident_image_bytes: 40,
            reserved_render_bytes: usize::MAX,
            render_byte_budget: usize::MAX,
            ..Default::default()
        };
        let video = VideoDiagnostics {
            resident_cpu_frame_bytes: 50,
            resident_render_image_bytes: 60,
            reserved_decoder_bytes: usize::MAX,
            decoder_budget_bytes: usize::MAX,
            ..Default::default()
        };

        let estimate =
            resident_memory_estimate(10, 20, 5, &exact_raster, &images, &mermaid, &video);
        assert_eq!(estimate.owned_bytes, 10 + 20 + 40 + 50 + 60);
        assert_eq!(estimate.shared_bytes, 5 + 25 + 30);
        assert_eq!(estimate.total_bytes(), 10 + 20 + 5 + 25 + 30 + 40 + 50 + 60);
    }
}
