use gpui::{AppContext, Context, EventEmitter, Task, Window};

use crate::CditorViewContract;
use crate::editor_view::CditorV2View;
use crate::persistence::{
    EditorSaveStatus, PersistenceBarrierKind, PersistencePipelineError, schedule_storage_autosave,
};
use cditor_editor_protocol::command::{CditorCommand, CommandOutcome, CommandSource};
use cditor_sdk::diagnostics::CditorDiagnostics;
use cditor_sdk::document::{
    Affinity, CloseGuard, DocumentInfo, DocumentPosition, DocumentSelection, RecoveryExport,
    SaveFailure, SaveFailureKind, SaveReport, SaveStatus, ScrollAlignment, TextOffset,
};
use cditor_sdk::event::CditorEvent;
use cditor_sdk::{CditorError, command::CommandState};

impl EventEmitter<CditorEvent> for CditorV2View {}

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

    fn sdk_is_dirty(&self) -> bool {
        CditorV2View::sdk_is_dirty(self)
    }

    fn sdk_save_status(&self) -> SaveStatus {
        CditorV2View::sdk_save_status(self)
    }

    fn sdk_close_guard(&self) -> CloseGuard {
        CditorV2View::sdk_close_guard(self)
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
}

impl CditorV2View {
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
        if !self.focus.editor.is_focused(window) {
            window.focus(&self.focus.editor, cx);
        }
    }

    pub fn sdk_blur(&mut self, window: &mut Window, _cx: &mut Context<Self>) {
        if self.focus.editor.is_focused(window) {
            window.blur();
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
            title: snapshot.title,
            revision: snapshot.revision,
            block_count: snapshot.block_count,
            readonly: snapshot.readonly,
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
            memory_estimate_bytes: u64::try_from(
                diagnostics
                    .payload_and_undo_bytes
                    .saturating_add(crate::text::text_layout_cache_stats().estimated_bytes)
                    .saturating_add(self.cache.text_layouts.estimated_metadata_bytes())
                    .saturating_add(self.cache.table_cell_layouts.estimated_metadata_bytes())
                    .saturating_add(self.cache.text_surface_layouts.estimated_metadata_bytes()),
            )
            .unwrap_or(u64::MAX),
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
}
