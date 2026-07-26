use std::time::Duration;

use cditor_editor_protocol::ProtocolError;
use cditor_storage::StorageBackendKind;
use cditor_storage::StorageSaveOutcome;

use crate::{
    DocumentPersistence, EditorSessionHandle, PersistenceBarrierKind, PersistenceFailure,
    PersistenceFailureKind, PersistencePipelineError, ReadyPersistenceBarrier, SessionIoExecutor,
    StorageSaveRequest, project_persistence_save_failure, project_persistence_save_success,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PersistenceSessionSnapshot {
    pub backend: Option<StorageBackendKind>,
    pub enabled: bool,
    pub saving: bool,
    pub pending_operations: usize,
    pub has_unpersisted_changes: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PersistenceSaveApply {
    pub should_reschedule: bool,
    pub saved_layout_or_structure: bool,
}

#[derive(Debug, Clone)]
pub struct StorageFlushRequest {
    session: DocumentPersistence,
}

pub async fn execute_storage_flush(request: StorageFlushRequest) -> Result<(), PersistenceFailure> {
    request
        .session
        .flush()
        .await
        .map_err(PersistenceFailure::from)?;
    request
        .session
        .prune_undo_blobs(100)
        .await
        .map_err(PersistenceFailure::from)?;
    Ok(())
}

pub fn run_storage_flush_with_timeout(
    request: StorageFlushRequest,
    timeout: Duration,
) -> Result<(), PersistenceFailure> {
    SessionIoExecutor::shared()
        .run(async move {
            tokio::time::timeout(timeout, execute_storage_flush(request))
                .await
                .map_err(|_| cditor_storage::StorageError::Timeout {
                    operation: "storage flush",
                    timeout,
                })
                .map_err(PersistenceFailure::from)?
        })
        .map_err(|message| PersistenceFailure::new(PersistenceFailureKind::Io, message))
        .and_then(|result| result)
}

impl EditorSessionHandle {
    pub fn persistence_snapshot(&self) -> Result<PersistenceSessionSnapshot, ProtocolError> {
        let session = self
            .inner
            .try_borrow()
            .map_err(|_| super::session::busy_error())?;
        Ok(PersistenceSessionSnapshot {
            backend: session
                .persistence
                .session()
                .map(DocumentPersistence::backend_kind),
            enabled: session.persistence.is_enabled(),
            saving: session.persistence.is_saving(),
            pending_operations: session.persistence.pending_operation_count(),
            has_unpersisted_changes: session.persistence.has_unpersisted_changes(),
        })
    }

    pub fn mark_persistence_dirty(&self) -> Result<PersistenceSessionSnapshot, ProtocolError> {
        let mut session = self.try_session_mut()?;
        session.persistence.mark_dirty();
        Ok(PersistenceSessionSnapshot {
            backend: session
                .persistence
                .session()
                .map(DocumentPersistence::backend_kind),
            enabled: session.persistence.is_enabled(),
            saving: session.persistence.is_saving(),
            pending_operations: session.persistence.pending_operation_count(),
            has_unpersisted_changes: session.persistence.has_unpersisted_changes(),
        })
    }

    pub fn request_autosave_timer(&self) -> Result<Option<Duration>, ProtocolError> {
        Ok(self.try_session_mut()?.persistence.request_autosave_timer())
    }

    pub fn clear_scheduled_save(&self) -> Result<(), ProtocolError> {
        self.try_session_mut()?.persistence.clear_scheduled_save();
        Ok(())
    }

    pub fn mark_loaded_structure_version(&self, version: u64) -> Result<(), ProtocolError> {
        self.try_session_mut()?
            .persistence
            .mark_loaded_structure_version(version);
        Ok(())
    }

    pub fn capture_storage_save(&self) -> Result<Option<StorageSaveRequest>, ProtocolError> {
        let mut session = self.try_session_mut()?;
        let crate::EditorSession {
            runtime,
            persistence,
            ..
        } = &mut *session;
        Ok(persistence.begin_batch(runtime))
    }

    pub fn apply_storage_save_success(
        &self,
        request: &StorageSaveRequest,
        outcome: &StorageSaveOutcome,
    ) -> Result<PersistenceSaveApply, ProtocolError> {
        let mut session = self.try_session_mut()?;
        let crate::EditorSession {
            runtime,
            persistence,
            ..
        } = &mut *session;
        let saved_layout_or_structure = outcome.saved_structure_version.is_some();
        let should_reschedule =
            persistence.finish_success(request, outcome.saved_structure_version);
        project_persistence_save_success(
            runtime,
            outcome,
            saved_layout_or_structure,
            should_reschedule,
        );
        Ok(PersistenceSaveApply {
            should_reschedule,
            saved_layout_or_structure,
        })
    }

    pub fn apply_storage_save_failure(
        &self,
        request: &StorageSaveRequest,
        failure: &PersistenceFailure,
    ) -> Result<bool, ProtocolError> {
        let mut session = self.try_session_mut()?;
        let crate::EditorSession {
            runtime,
            persistence,
            ..
        } = &mut *session;
        project_persistence_save_failure(runtime, request.transactions().to_vec());
        let should_reschedule = persistence.finish_failed(request);
        persistence.fail_barriers(failure);
        Ok(should_reschedule)
    }

    pub fn request_persistence_barrier(
        &self,
        kind: PersistenceBarrierKind,
        revision: u64,
    ) -> Result<
        tokio::sync::oneshot::Receiver<
            Result<crate::PersistenceBarrierReport, PersistencePipelineError>,
        >,
        ProtocolError,
    > {
        Ok(self
            .try_session_mut()?
            .persistence
            .request_barrier(kind, revision))
    }

    pub fn drain_ready_persistence_barriers(
        &self,
    ) -> Result<(Vec<ReadyPersistenceBarrier>, Vec<ReadyPersistenceBarrier>), ProtocolError> {
        Ok(self.try_session_mut()?.persistence.drain_ready_barriers())
    }

    pub fn begin_storage_flush(&self) -> Result<Option<StorageFlushRequest>, ProtocolError> {
        let mut session = self.try_session_mut()?;
        let Some(storage) = session.persistence.session().cloned() else {
            return Ok(None);
        };
        session.persistence.begin_backend_flush();
        Ok(Some(StorageFlushRequest { session: storage }))
    }

    pub fn finish_storage_flush(&self) -> Result<(), ProtocolError> {
        self.try_session_mut()?.persistence.finish_backend_flush();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use cditor_editor_protocol::command::{CommandEnvelope, CommandSource, EditorCommand};
    use cditor_runtime::DocumentRuntime;
    use cditor_storage::{
        DocumentStorage, LoadDocumentRequest, LoadedDocument, LoadedPayloadBatch,
        StorageBackendKind, StorageCapabilities, StorageError, StorageResult, StorageSaveBatch,
        StorageSaveOutcome,
    };

    use crate::{EditorSession, PersistencePipeline};

    use super::*;

    #[derive(Debug)]
    struct NoopStorage;

    #[async_trait]
    impl DocumentStorage for NoopStorage {
        fn backend_kind(&self) -> StorageBackendKind {
            StorageBackendKind::Custom
        }

        fn capabilities(&self) -> StorageCapabilities {
            StorageCapabilities::POSTGRES
        }

        async fn load_document(
            &self,
            _request: LoadDocumentRequest,
        ) -> StorageResult<LoadedDocument> {
            Err(StorageError::InvalidConfiguration("test".to_owned()))
        }

        async fn load_payloads(
            &self,
            _document_id: u64,
            _block_ids: &[u64],
        ) -> StorageResult<LoadedPayloadBatch> {
            Err(StorageError::InvalidConfiguration("test".to_owned()))
        }

        async fn commit(&self, _batch: StorageSaveBatch) -> StorageResult<StorageSaveOutcome> {
            Err(StorageError::InvalidConfiguration("test".to_owned()))
        }
    }

    #[test]
    fn session_owns_capture_and_failure_restore_as_one_borrow() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            (1..=3)
                .map(|block_id| {
                    cditor_core::rich_text::BlockPayloadRecord::rich_text(
                        block_id,
                        cditor_core::rich_text::RichBlockKind::Paragraph,
                        block_id.to_string(),
                    )
                })
                .collect(),
            720.0,
        );
        runtime
            .dispatch(CommandEnvelope::new(
                EditorCommand::MoveBlockBefore {
                    block_id: 1,
                    before_block_id: Some(3),
                },
                CommandSource::Sdk,
            ))
            .unwrap();
        let storage = DocumentPersistence::new(Arc::new(NoopStorage), 1);
        let pipeline = PersistencePipeline::for_session(storage, None);
        let handle = EditorSession::with_persistence(runtime, false, pipeline).into_handle();
        handle.mark_loaded_structure_version(1).unwrap();
        handle.mark_persistence_dirty().unwrap();

        let first = handle.capture_storage_save().unwrap().unwrap();
        assert_eq!(first.transactions().len(), 1);
        handle
            .apply_storage_save_failure(
                &first,
                &PersistenceFailure::new(crate::PersistenceFailureKind::Other, "forced failure"),
            )
            .unwrap();
        let retry = handle.capture_storage_save().unwrap().unwrap();

        assert_eq!(retry.transactions(), first.transactions());
        assert!(
            handle
                .persistence_snapshot()
                .unwrap()
                .has_unpersisted_changes
        );
    }
}
