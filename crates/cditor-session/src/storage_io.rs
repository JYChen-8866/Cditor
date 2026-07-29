use cditor_core::edit::{EditTransaction, ExternalUndoBlobRef, UndoExternalizationJob};
use cditor_storage::{LoadedPayloadBatch, StorageBackendKind, StorageError};

use crate::{DocumentPersistence, EditorSessionHandle, SessionIoExecutor, session::busy_error};

#[derive(Debug, Clone)]
pub struct PayloadStorageRequest {
    session: DocumentPersistence,
}

#[derive(Debug, Clone)]
pub struct UndoBlobReadRequest {
    session: DocumentPersistence,
    reference: ExternalUndoBlobRef,
}

#[derive(Debug)]
pub struct UndoBlobWriteRequest {
    session: DocumentPersistence,
    job: UndoExternalizationJob,
}

#[derive(Debug)]
pub struct UndoBlobDeleteRequest {
    session: DocumentPersistence,
    references: Vec<ExternalUndoBlobRef>,
}

impl EditorSessionHandle {
    pub fn payload_storage_request(
        &self,
    ) -> Result<Option<PayloadStorageRequest>, cditor_editor_protocol::ProtocolError> {
        let session = self.inner.try_borrow().map_err(|_| busy_error())?;
        Ok(session
            .persistence
            .session()
            .cloned()
            .map(|session| PayloadStorageRequest { session }))
    }

    pub fn undo_blob_read_request(
        &self,
        reference: ExternalUndoBlobRef,
    ) -> Result<Option<UndoBlobReadRequest>, cditor_editor_protocol::ProtocolError> {
        let session = self.inner.try_borrow().map_err(|_| busy_error())?;
        Ok(session
            .persistence
            .session()
            .cloned()
            .map(|storage| UndoBlobReadRequest {
                session: storage,
                reference,
            }))
    }

    pub fn undo_blob_write_request(
        &self,
    ) -> Result<Option<UndoBlobWriteRequest>, cditor_editor_protocol::ProtocolError> {
        let mut session = self.try_session_mut()?;
        let Some(storage) = session.persistence.session().cloned() else {
            return Ok(None);
        };
        Ok(
            crate::project_begin_undo_blob_spill(&mut session.runtime).map(|job| {
                UndoBlobWriteRequest {
                    session: storage,
                    job,
                }
            }),
        )
    }

    pub fn undo_blob_delete_request(
        &self,
    ) -> Result<Option<UndoBlobDeleteRequest>, cditor_editor_protocol::ProtocolError> {
        let mut session = self.try_session_mut()?;
        let Some(storage) = session.persistence.session().cloned() else {
            return Ok(None);
        };
        let references = crate::project_begin_undo_blob_cleanup(&mut session.runtime);
        Ok((!references.is_empty()).then_some(UndoBlobDeleteRequest {
            session: storage,
            references,
        }))
    }
}

impl PayloadStorageRequest {
    /// Local stores have a bounded, in-process payload path suitable for the
    /// scrollbar's frame-critical foreground read. Remote and custom stores
    /// must stay on the asynchronous bridge because their latency is not bounded.
    pub fn is_local(&self) -> bool {
        supports_scrollbar_foreground_load(self.session.backend_kind())
    }
}

const fn supports_scrollbar_foreground_load(backend: StorageBackendKind) -> bool {
    matches!(backend, StorageBackendKind::Local)
}

pub async fn execute_payload_load(
    request: PayloadStorageRequest,
    block_ids: &[u64],
) -> Result<LoadedPayloadBatch, StorageError> {
    request.session.load_payloads(block_ids).await
}

pub async fn execute_undo_blob_read(
    request: UndoBlobReadRequest,
) -> Result<(ExternalUndoBlobRef, EditTransaction), StorageError> {
    let transaction = request.session.load_undo_blob(&request.reference).await?;
    Ok((request.reference, transaction))
}

pub fn run_payload_load(
    request: PayloadStorageRequest,
    block_ids: &[u64],
    timeout: std::time::Duration,
    operation: &'static str,
) -> Result<LoadedPayloadBatch, String> {
    SessionIoExecutor::shared()
        .run(async move {
            tokio::time::timeout(timeout, execute_payload_load(request, block_ids))
                .await
                .map_err(|_| StorageError::Timeout { operation, timeout })?
        })
        .and_then(|result| result.map_err(|error| error.to_string()))
}

pub fn run_undo_blob_read(
    request: UndoBlobReadRequest,
    timeout: std::time::Duration,
) -> Result<(ExternalUndoBlobRef, EditTransaction), String> {
    SessionIoExecutor::shared()
        .run(async move {
            tokio::time::timeout(timeout, execute_undo_blob_read(request))
                .await
                .map_err(|_| StorageError::Timeout {
                    operation: "history hydration",
                    timeout,
                })?
        })
        .and_then(|result| result.map_err(|error| error.to_string()))
}

pub async fn execute_undo_blob_write(
    request: UndoBlobWriteRequest,
) -> (
    UndoExternalizationJob,
    Result<ExternalUndoBlobRef, StorageError>,
) {
    let result = request
        .session
        .write_undo_blob(
            request.job.snapshot_id,
            request.job.block_count,
            &request.job.transaction,
        )
        .await;
    (request.job, result)
}

pub fn run_undo_blob_write(
    request: UndoBlobWriteRequest,
    timeout: std::time::Duration,
) -> (UndoExternalizationJob, Result<ExternalUndoBlobRef, String>) {
    let UndoBlobWriteRequest { session, job } = request;
    let result = SessionIoExecutor::shared()
        .run(async {
            tokio::time::timeout(
                timeout,
                session.write_undo_blob(job.snapshot_id, job.block_count, &job.transaction),
            )
            .await
            .map_err(|_| StorageError::Timeout {
                operation: "undo blob write",
                timeout,
            })?
        })
        .and_then(|result| result.map_err(|error| error.to_string()));
    (job, result)
}

pub async fn execute_undo_blob_delete(request: UndoBlobDeleteRequest) -> Vec<ExternalUndoBlobRef> {
    let mut failed = Vec::new();
    for reference in request.references {
        if request
            .session
            .delete_undo_blob(reference.snapshot_id)
            .await
            .is_err()
        {
            failed.push(reference);
        }
    }
    failed
}

pub fn run_undo_blob_delete(
    request: UndoBlobDeleteRequest,
    timeout: std::time::Duration,
) -> Vec<ExternalUndoBlobRef> {
    let references = request.references.clone();
    SessionIoExecutor::shared()
        .run(async move {
            tokio::time::timeout(timeout, execute_undo_blob_delete(request))
                .await
                .map_err(|_| StorageError::Timeout {
                    operation: "undo blob cleanup",
                    timeout,
                })
        })
        .ok()
        .and_then(Result::ok)
        .unwrap_or(references)
}

#[cfg(test)]
mod tests {
    use cditor_runtime::DocumentRuntime;
    use cditor_storage::StorageBackendKind;

    use super::supports_scrollbar_foreground_load;
    use crate::EditorSession;

    #[test]
    fn disabled_session_never_exposes_a_storage_request() {
        let handle = EditorSession::new(DocumentRuntime::empty(), false).into_handle();

        assert!(handle.payload_storage_request().unwrap().is_none());
        assert!(handle.undo_blob_write_request().unwrap().is_none());
        assert!(handle.undo_blob_delete_request().unwrap().is_none());
    }

    #[test]
    fn only_local_storage_uses_the_scrollbar_foreground_load() {
        assert!(supports_scrollbar_foreground_load(
            StorageBackendKind::Local
        ));
        assert!(!supports_scrollbar_foreground_load(
            StorageBackendKind::Remote
        ));
        assert!(!supports_scrollbar_foreground_load(
            StorageBackendKind::Custom
        ));
    }
}
