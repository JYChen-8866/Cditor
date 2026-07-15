use gpui::{AppContext, Context, EventEmitter, Task, Window};

use crate::api::{
    Affinity, BlockTransform, CditorCommand, CditorDiagnostics, CditorError, CditorEvent,
    ChangeOrigin, CloseGuard, CommandOutcome, CommandState, DocumentInfo, DocumentPosition,
    DocumentSelection, SaveReport, SaveStatus, ScrollAlignment, TextOffset,
};
use crate::gui::app::CditorV2View;
use crate::gui::persistence::{EditorSaveStatus, PersistenceBarrierKind};

impl EventEmitter<CditorEvent> for CditorV2View {}

impl CditorV2View {
    pub(crate) fn sdk_configure_ai(
        &mut self,
        provider: Option<std::sync::Arc<dyn cditor_ai::AiProvider>>,
        enabled: bool,
    ) {
        if let Some(provider) = provider {
            self.ai_provider = provider;
        }
        self.ai_enabled = enabled;
    }

    pub(crate) fn sdk_is_ready(&self) -> bool {
        self.state.is_ready()
    }

    pub(crate) fn sdk_is_readonly(&self) -> bool {
        self.readonly
    }

    pub(crate) fn sdk_set_readonly(&mut self, readonly: bool, cx: &mut Context<Self>) {
        if self.readonly == readonly {
            return;
        }
        self.readonly = readonly;
        self.save_status = if readonly {
            EditorSaveStatus::Readonly
        } else if self.dirty {
            EditorSaveStatus::Dirty
        } else {
            EditorSaveStatus::Clean
        };
        if !readonly && self.dirty {
            self.storage_persistence.schedule(cx);
        }
        cx.notify();
    }

    pub(crate) fn sdk_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.focus.is_focused(window) {
            window.focus(&self.focus, cx);
        }
    }

    pub(crate) fn sdk_blur(&mut self, window: &mut Window, _cx: &mut Context<Self>) {
        if self.focus.is_focused(window) {
            window.blur();
        }
    }

    pub(crate) fn sdk_can_undo(&self) -> bool {
        !self.readonly
            && self
                .ready_runtime_ref()
                .is_some_and(|runtime| runtime.can_undo())
    }

    pub(crate) fn sdk_can_redo(&self) -> bool {
        !self.readonly
            && self
                .ready_runtime_ref()
                .is_some_and(|runtime| runtime.can_redo())
    }

    pub(crate) fn sdk_undo(&mut self, cx: &mut Context<Self>) -> Result<bool, CditorError> {
        self.sdk_history_action(ChangeOrigin::Undo, cx, |runtime| {
            runtime.undo_focused_block()
        })
    }

    pub(crate) fn sdk_redo(&mut self, cx: &mut Context<Self>) -> Result<bool, CditorError> {
        self.sdk_history_action(ChangeOrigin::Redo, cx, |runtime| {
            runtime.redo_focused_block()
        })
    }

    fn sdk_history_action(
        &mut self,
        origin: ChangeOrigin,
        cx: &mut Context<Self>,
        action: impl FnOnce(&mut cditor_runtime::DocumentRuntime) -> Result<bool, String>,
    ) -> Result<bool, CditorError> {
        if self.readonly {
            return Err(CditorError::Readonly);
        }
        let runtime = self.ready_runtime().ok_or(CditorError::NotReady)?;
        let changed = action(runtime).map_err(CditorError::Internal)?;
        if changed {
            self.mark_dirty_with_origin(origin, cx);
            cx.notify();
        }
        Ok(changed)
    }

    pub(crate) fn sdk_document_info(&self) -> Option<DocumentInfo> {
        let runtime = self.ready_runtime_ref()?;
        Some(DocumentInfo {
            document_id: runtime.document_id,
            title: runtime.document_title().map(ToOwned::to_owned),
            revision: runtime.revision(),
            block_count: runtime.document_block_count(),
            readonly: self.readonly,
        })
    }

    pub(crate) fn sdk_is_dirty(&self) -> bool {
        self.dirty
    }

    pub(crate) fn sdk_save_status(&self) -> SaveStatus {
        match &self.save_status {
            EditorSaveStatus::Clean => SaveStatus::Clean,
            EditorSaveStatus::Dirty => SaveStatus::Dirty,
            EditorSaveStatus::Saving => SaveStatus::Saving,
            EditorSaveStatus::Failed(message) => SaveStatus::Failed(message.clone()),
            EditorSaveStatus::Readonly => SaveStatus::Readonly,
        }
    }

    pub(crate) fn sdk_close_guard(&self) -> CloseGuard {
        let saving = self.storage_persistence.is_saving();
        let failed_operations =
            usize::from(matches!(self.save_status, EditorSaveStatus::Failed(_)));
        CloseGuard {
            dirty: self.dirty,
            saving,
            failed_operations,
            can_close_safely: !self.dirty && !saving && failed_operations == 0,
        }
    }

    pub(crate) fn sdk_save(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Task<Result<SaveReport, CditorError>> {
        self.sdk_persistence_barrier(PersistenceBarrierKind::Save, cx)
    }

    pub(crate) fn sdk_flush(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Task<Result<SaveReport, CditorError>> {
        self.sdk_persistence_barrier(PersistenceBarrierKind::Flush, cx)
    }

    fn sdk_persistence_barrier(
        &mut self,
        kind: PersistenceBarrierKind,
        cx: &mut Context<Self>,
    ) -> Task<Result<SaveReport, CditorError>> {
        if self.readonly {
            return Task::ready(Err(CditorError::Readonly));
        }
        let Some(revision) = self.ready_runtime_ref().map(|runtime| runtime.revision()) else {
            return Task::ready(Err(CditorError::NotReady));
        };
        if !self.storage_persistence.is_enabled() {
            return Task::ready(Err(CditorError::Unsupported(
                "save and flush require a persistent storage backend".to_owned(),
            )));
        }

        let receiver = self.storage_persistence.request_barrier(kind, revision);
        self.flush_storage_persistence(cx);
        cx.background_spawn(
            async move { receiver.await.unwrap_or(Err(CditorError::ComponentDropped)) },
        )
    }

    pub(crate) fn sdk_diagnostics(&self) -> Result<CditorDiagnostics, CditorError> {
        let runtime = self.ready_runtime_ref().ok_or(CditorError::NotReady)?;
        Ok(CditorDiagnostics {
            storage_backend: self
                .storage_persistence
                .session()
                .map(|session| session.backend_kind()),
            document_blocks: runtime.document_block_count(),
            loaded_payloads: runtime.loaded_payload_count(),
            rendered_blocks: self.projected_block_rects.len(),
            pending_layout_tasks: runtime.pending_layout_task_count(),
            pending_saves: self.storage_persistence.pending_operation_count(),
            dirty_blocks: runtime.dirty_payload_count(),
            estimated_document_height: runtime.estimated_document_height(),
            memory_estimate_bytes: u64::try_from(runtime.estimated_payload_memory_bytes())
                .unwrap_or(u64::MAX),
        })
    }

    pub(crate) fn sdk_selection(&self) -> Option<DocumentSelection> {
        let selection = self.ready_runtime_ref()?.document_selection_snapshot()?;
        Some(DocumentSelection {
            anchor: sdk_position(selection.anchor),
            head: sdk_position(selection.focus),
        })
    }

    pub(crate) fn sdk_set_selection(
        &mut self,
        selection: DocumentSelection,
        cx: &mut Context<Self>,
    ) -> Result<(), CditorError> {
        let runtime = self.ready_runtime().ok_or(CditorError::NotReady)?;
        let anchor = runtime_position(runtime, selection.anchor)?;
        let focus = runtime_position(runtime, selection.head)?;
        runtime
            .set_document_selection(cditor_core::edit::DocumentSelection { anchor, focus })
            .map_err(|_| CditorError::InvalidSelection)?;
        let applied = self.sdk_selection().ok_or(CditorError::InvalidSelection)?;
        self.last_emitted_selection = Some(applied);
        cx.emit(CditorEvent::SelectionChanged { selection: applied });
        cx.notify();
        Ok(())
    }

    pub(crate) fn sdk_selected_text(&self) -> Option<String> {
        self.ready_runtime_ref()?.selected_focused_text()
    }

    pub(crate) fn sdk_scroll_to_block(
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
        self.ready_runtime()
            .ok_or(CditorError::NotReady)?
            .scroll_to_block_with_alignment(block_id, alignment)
            .map_err(|_| CditorError::BlockNotFound(block_id))?;
        cx.notify();
        Ok(())
    }

    pub(crate) fn sdk_execute_command(
        &mut self,
        command: CditorCommand,
        cx: &mut Context<Self>,
    ) -> Result<CommandOutcome, CditorError> {
        if matches!(&command, CditorCommand::Undo) {
            return self.sdk_undo(cx).map(|changed| CommandOutcome {
                changed,
                transaction_id: None,
            });
        }
        if matches!(&command, CditorCommand::Redo) {
            return self.sdk_redo(cx).map(|changed| CommandOutcome {
                changed,
                transaction_id: None,
            });
        }
        let is_select_all = matches!(&command, CditorCommand::SelectAll);
        let mutating = !is_select_all;
        if mutating && self.readonly {
            return Err(CditorError::Readonly);
        }
        let runtime = self.ready_runtime().ok_or(CditorError::NotReady)?;
        let changed = match command {
            CditorCommand::SelectAll => runtime.select_all_command(),
            CditorCommand::DeleteSelection => runtime
                .delete_active_selection()
                .map_err(CditorError::Internal)?,
            CditorCommand::ToggleBold => {
                toggle_mark(runtime, cditor_core::rich_text::InlineMark::Bold)?
            }
            CditorCommand::ToggleItalic => {
                toggle_mark(runtime, cditor_core::rich_text::InlineMark::Italic)?
            }
            CditorCommand::ToggleUnderline => {
                toggle_mark(runtime, cditor_core::rich_text::InlineMark::Underline)?
            }
            CditorCommand::ToggleStrike => {
                toggle_mark(runtime, cditor_core::rich_text::InlineMark::Strike)?
            }
            CditorCommand::ToggleInlineCode => {
                toggle_mark(runtime, cditor_core::rich_text::InlineMark::Code)?
            }
            CditorCommand::TransformBlock(BlockTransform::Kind(kind)) => runtime
                .convert_focused_block_kind(kind)
                .map_err(CditorError::Internal)?,
            CditorCommand::DeleteSelectedBlocks => runtime
                .delete_selected_block_selection()
                .map_err(CditorError::Internal)?,
            CditorCommand::FoldHeading => {
                let block_id = runtime
                    .focused_block_id()
                    .ok_or(CditorError::InvalidSelection)?;
                if runtime.is_block_folded(block_id) {
                    false
                } else {
                    runtime
                        .toggle_block_fold(block_id)
                        .map_err(CditorError::Internal)?
                }
            }
            CditorCommand::UnfoldHeading => {
                let block_id = runtime
                    .focused_block_id()
                    .ok_or(CditorError::InvalidSelection)?;
                if !runtime.is_block_folded(block_id) {
                    false
                } else {
                    runtime
                        .toggle_block_fold(block_id)
                        .map_err(CditorError::Internal)?
                }
            }
            unsupported => {
                return Err(CditorError::Unsupported(format!(
                    "command {} is not connected to the SDK command router yet",
                    unsupported.stable_id()
                )));
            }
        };
        if changed && mutating {
            self.mark_dirty_with_origin(ChangeOrigin::Host, cx);
        }
        if is_select_all && let Some(selection) = self.sdk_selection() {
            cx.emit(CditorEvent::SelectionChanged { selection });
        }
        cx.notify();
        Ok(CommandOutcome {
            changed,
            transaction_id: None,
        })
    }

    pub(crate) fn sdk_command_state(&self, command: &CditorCommand) -> CommandState {
        let Some(runtime) = self.ready_runtime_ref() else {
            return CommandState::DISABLED;
        };
        let enabled = match command {
            CditorCommand::Undo => self.sdk_can_undo(),
            CditorCommand::Redo => self.sdk_can_redo(),
            CditorCommand::SelectAll => true,
            CditorCommand::DeleteSelection => !self.readonly && runtime.has_active_selection(),
            CditorCommand::ToggleBold
            | CditorCommand::ToggleItalic
            | CditorCommand::ToggleUnderline
            | CditorCommand::ToggleStrike
            | CditorCommand::ToggleInlineCode => {
                !self.readonly && runtime.focused_text_selection_range().is_some()
            }
            CditorCommand::FoldHeading | CditorCommand::UnfoldHeading => {
                !self.readonly
                    && runtime.focused_block_id().is_some_and(|block_id| {
                        matches!(
                            runtime.block_kind(block_id),
                            Some(cditor_core::rich_text::RichBlockKind::Heading { .. })
                        )
                    })
            }
            CditorCommand::TransformBlock(BlockTransform::Kind(kind)) => {
                !self.readonly
                    && runtime
                        .focused_block_id()
                        .is_some_and(|block_id| runtime.can_convert_block_kind(block_id, kind))
            }
            _ => false,
        };
        CommandState {
            enabled,
            active: false,
            visible: true,
        }
    }

    pub(in crate::gui::app) fn sdk_register_focus_observers(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.sdk_focus_observers_registered {
            return;
        }
        self.sdk_focus_observers_registered = true;
        let focus = self.focus.clone();
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

    pub(in crate::gui::app) fn sdk_emit_selection_if_changed(&mut self, cx: &mut Context<Self>) {
        let selection = self.sdk_selection();
        if selection == self.last_emitted_selection {
            return;
        }
        self.last_emitted_selection = selection;
        if let Some(selection) = selection {
            cx.emit(CditorEvent::SelectionChanged { selection });
        }
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

fn runtime_position(
    runtime: &cditor_runtime::DocumentRuntime,
    position: DocumentPosition,
) -> Result<cditor_core::edit::TextPosition, CditorError> {
    let text = runtime
        .block_payload_record(position.block_id)
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

fn toggle_mark(
    runtime: &mut cditor_runtime::DocumentRuntime,
    mark: cditor_core::rich_text::InlineMark,
) -> Result<bool, CditorError> {
    runtime
        .toggle_inline_mark_on_selection(mark)
        .map_err(CditorError::Internal)
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
        let position = |offset| DocumentPosition {
            block_id: 1,
            offset: TextOffset::Utf16CodeUnits(offset),
            affinity: Affinity::Downstream,
        };

        assert_eq!(runtime_position(&runtime, position(3)).unwrap().offset, 5);
        assert_eq!(
            runtime_position(&runtime, position(2)),
            Err(CditorError::InvalidSelection)
        );
    }
}
