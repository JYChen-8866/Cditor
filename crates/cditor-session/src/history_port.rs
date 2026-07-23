use cditor_core::edit::{EditTransaction, ExternalUndoBlobRef, UndoExternalizationJob};
use cditor_editor_protocol::{
    ProtocolError, ProtocolErrorCode,
    command::{CommandEnvelope, CommandSource, EditorCommand},
};
use cditor_runtime::DocumentRuntime;

use crate::{CommandDispatchSnapshot, EditorSessionHandle, project_command_dispatch};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryDirection {
    Undo,
    Redo,
}

impl HistoryDirection {
    fn command(self) -> EditorCommand {
        match self {
            Self::Undo => EditorCommand::Undo,
            Self::Redo => EditorCommand::Redo,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryActionSnapshot {
    Applied(CommandDispatchSnapshot),
    HydrationRequired {
        reference: ExternalUndoBlobRef,
        dispatch_error: String,
    },
}

#[derive(Debug)]
pub enum UndoBlobWriteResult {
    Stored(ExternalUndoBlobRef),
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UndoBlobSpillApplySnapshot {
    pub completed: bool,
    pub transaction_restored: bool,
    pub cleanup_required: bool,
}

pub fn project_begin_undo_blob_spill(
    runtime: &mut DocumentRuntime,
) -> Option<UndoExternalizationJob> {
    runtime.begin_external_undo_spill()
}

pub fn project_apply_undo_blob_write_result(
    runtime: &mut DocumentRuntime,
    job: UndoExternalizationJob,
    result: UndoBlobWriteResult,
) -> UndoBlobSpillApplySnapshot {
    match result {
        UndoBlobWriteResult::Stored(reference) => {
            let orphaned = reference.clone();
            match runtime.complete_external_undo_spill(job, reference) {
                Ok(()) => UndoBlobSpillApplySnapshot {
                    completed: true,
                    transaction_restored: false,
                    cleanup_required: false,
                },
                Err(job) => {
                    let transaction_restored = runtime.abort_external_undo_spill(job);
                    runtime.restore_orphaned_external_undo_blobs([orphaned]);
                    UndoBlobSpillApplySnapshot {
                        completed: false,
                        transaction_restored,
                        cleanup_required: true,
                    }
                }
            }
        }
        UndoBlobWriteResult::Failed => UndoBlobSpillApplySnapshot {
            completed: false,
            transaction_restored: runtime.abort_external_undo_spill(job),
            cleanup_required: false,
        },
    }
}

pub fn project_begin_undo_blob_cleanup(runtime: &mut DocumentRuntime) -> Vec<ExternalUndoBlobRef> {
    runtime.drain_orphaned_external_undo_blobs()
}

pub fn project_finish_undo_blob_cleanup(
    runtime: &mut DocumentRuntime,
    failed: Vec<ExternalUndoBlobRef>,
) {
    runtime.restore_orphaned_external_undo_blobs(failed);
}

pub fn project_history_action(
    runtime: &mut DocumentRuntime,
    source: CommandSource,
    direction: HistoryDirection,
) -> Result<HistoryActionSnapshot, ProtocolError> {
    match project_command_dispatch(runtime, CommandEnvelope::new(direction.command(), source)) {
        Ok(snapshot) => Ok(HistoryActionSnapshot::Applied(snapshot)),
        Err(error) => {
            let reference = match direction {
                HistoryDirection::Undo => runtime.pending_undo_hydration(),
                HistoryDirection::Redo => runtime.pending_redo_hydration(),
            };
            reference
                .map(|reference| HistoryActionSnapshot::HydrationRequired {
                    reference,
                    dispatch_error: error.message.clone(),
                })
                .ok_or(error)
        }
    }
}

pub fn project_hydrated_history_action(
    runtime: &mut DocumentRuntime,
    reference: &ExternalUndoBlobRef,
    transaction: EditTransaction,
    source: CommandSource,
    direction: HistoryDirection,
) -> Result<CommandDispatchSnapshot, ProtocolError> {
    if !runtime.hydrate_external_undo(reference.snapshot_id, transaction) {
        return Err(ProtocolError::new(
            ProtocolErrorCode::StalePrecondition,
            "undo reference changed while hydrating",
        )
        .with_document(runtime.document_id()));
    }
    match project_history_action(runtime, source, direction)? {
        HistoryActionSnapshot::Applied(snapshot) => Ok(snapshot),
        HistoryActionSnapshot::HydrationRequired { .. } => Err(ProtocolError::new(
            ProtocolErrorCode::ApplyFailed,
            "hydrated history action still requires external state",
        )
        .with_document(runtime.document_id())),
    }
}

impl EditorSessionHandle {
    pub fn apply_history(
        &self,
        source: CommandSource,
        direction: HistoryDirection,
    ) -> Result<HistoryActionSnapshot, ProtocolError> {
        let mut session = self.try_session_mut()?;
        if session.readonly {
            return Err(
                ProtocolError::new(ProtocolErrorCode::Readonly, "document is read-only")
                    .with_document(session.runtime.document_id()),
            );
        }
        project_history_action(&mut session.runtime, source, direction)
    }

    pub fn begin_undo_blob_spill(&self) -> Result<Option<UndoExternalizationJob>, ProtocolError> {
        Ok(project_begin_undo_blob_spill(
            &mut self.try_session_mut()?.runtime,
        ))
    }

    pub fn apply_undo_blob_write_result(
        &self,
        job: UndoExternalizationJob,
        result: UndoBlobWriteResult,
    ) -> Result<UndoBlobSpillApplySnapshot, ProtocolError> {
        Ok(project_apply_undo_blob_write_result(
            &mut self.try_session_mut()?.runtime,
            job,
            result,
        ))
    }

    pub fn begin_undo_blob_cleanup(&self) -> Result<Vec<ExternalUndoBlobRef>, ProtocolError> {
        Ok(project_begin_undo_blob_cleanup(
            &mut self.try_session_mut()?.runtime,
        ))
    }

    pub fn finish_undo_blob_cleanup(
        &self,
        failed: Vec<ExternalUndoBlobRef>,
    ) -> Result<(), ProtocolError> {
        project_finish_undo_blob_cleanup(&mut self.try_session_mut()?.runtime, failed);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use cditor_core::rich_text::{BlockPayloadRecord, RichBlockKind};
    use cditor_editor_protocol::command::{CommandEnvelope, CommandSource, EditorCommand};

    use super::*;
    use crate::EditorSession;

    fn runtime_with_externalizable_history() -> DocumentRuntime {
        let block_count = 1_026;
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            (1..=block_count)
                .map(|block_id| {
                    BlockPayloadRecord::rich_text(
                        block_id,
                        RichBlockKind::Paragraph,
                        block_id.to_string(),
                    )
                })
                .collect(),
            720.0,
        );
        runtime
            .dispatch(CommandEnvelope::new(
                EditorCommand::SetBlockSelectionRange {
                    anchor_block_id: 1,
                    focus_block_id: block_count,
                },
                CommandSource::Sdk,
            ))
            .unwrap();
        runtime
            .dispatch(CommandEnvelope::new(
                EditorCommand::DeleteSelectedBlocks,
                CommandSource::Sdk,
            ))
            .unwrap();
        runtime
    }

    fn blob_reference(snapshot_id: u64, block_count: usize) -> ExternalUndoBlobRef {
        ExternalUndoBlobRef {
            snapshot_id,
            storage_key: format!("undo:{snapshot_id}"),
            checksum: format!("checksum:{snapshot_id}"),
            encoded_len: 128,
            block_count,
        }
    }

    #[test]
    fn empty_history_is_a_successful_no_op_snapshot() {
        let handle = EditorSession::new(DocumentRuntime::empty(), false).into_handle();
        let HistoryActionSnapshot::Applied(snapshot) = handle
            .apply_history(CommandSource::Sdk, HistoryDirection::Undo)
            .unwrap()
        else {
            panic!("empty local history must not request hydration");
        };

        assert!(!snapshot.outcome.changed());
        assert_eq!(snapshot.before_revision, snapshot.revision);
    }

    #[test]
    fn local_history_returns_the_applied_revision_snapshot() {
        let runtime = DocumentRuntime::demo();
        let block_id = runtime.visible_block_ids()[0];
        let handle = EditorSession::new(runtime, false).into_handle();
        handle
            .dispatch(CommandEnvelope::new(
                EditorCommand::FocusBlock { block_id },
                CommandSource::Sdk,
            ))
            .unwrap();
        handle
            .dispatch(CommandEnvelope::new(
                EditorCommand::InsertParagraphAfterFocused,
                CommandSource::Sdk,
            ))
            .unwrap();

        let HistoryActionSnapshot::Applied(snapshot) = handle
            .apply_history(CommandSource::Sdk, HistoryDirection::Undo)
            .unwrap()
        else {
            panic!("resident local history must not request hydration");
        };
        assert!(snapshot.outcome.changed());
        assert!(snapshot.revision > snapshot.before_revision);
    }

    #[test]
    fn successful_blob_write_atomically_commits_externalized_history() {
        let mut runtime = runtime_with_externalizable_history();
        let job = project_begin_undo_blob_spill(&mut runtime).unwrap();
        let reference = blob_reference(job.snapshot_id, job.block_count);

        let outcome = project_apply_undo_blob_write_result(
            &mut runtime,
            job,
            UndoBlobWriteResult::Stored(reference.clone()),
        );

        assert!(outcome.completed);
        assert!(!outcome.transaction_restored);
        assert!(!outcome.cleanup_required);
        assert_eq!(runtime.pending_undo_hydration(), Some(reference));
    }

    #[test]
    fn failed_or_stale_blob_write_restores_history_and_tracks_orphan_cleanup() {
        let mut failed_runtime = runtime_with_externalizable_history();
        let failed_job = project_begin_undo_blob_spill(&mut failed_runtime).unwrap();
        let failed = project_apply_undo_blob_write_result(
            &mut failed_runtime,
            failed_job,
            UndoBlobWriteResult::Failed,
        );
        assert!(!failed.completed);
        assert!(failed.transaction_restored);
        assert!(project_begin_undo_blob_spill(&mut failed_runtime).is_some());

        let mut stale_runtime = runtime_with_externalizable_history();
        let stale_job = project_begin_undo_blob_spill(&mut stale_runtime).unwrap();
        let orphan = blob_reference(
            stale_job.snapshot_id.saturating_add(1),
            stale_job.block_count,
        );
        let stale = project_apply_undo_blob_write_result(
            &mut stale_runtime,
            stale_job,
            UndoBlobWriteResult::Stored(orphan.clone()),
        );
        assert!(!stale.completed);
        assert!(stale.transaction_restored);
        assert!(stale.cleanup_required);
        assert_eq!(
            project_begin_undo_blob_cleanup(&mut stale_runtime),
            vec![orphan]
        );
    }

    #[test]
    fn failed_cleanup_requeues_owned_references_without_duplicates() {
        let mut runtime = DocumentRuntime::empty();
        let reference = blob_reference(9, 3);

        project_finish_undo_blob_cleanup(&mut runtime, vec![reference.clone(), reference.clone()]);

        assert_eq!(
            project_begin_undo_blob_cleanup(&mut runtime),
            vec![reference]
        );
        assert!(project_begin_undo_blob_cleanup(&mut runtime).is_empty());
    }
}
