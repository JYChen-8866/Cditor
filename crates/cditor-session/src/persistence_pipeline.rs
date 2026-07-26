use std::time::{Duration, Instant};
use std::{error::Error, fmt};

use cditor_runtime::DocumentRuntime;
use cditor_storage::{StorageError, StorageSaveBatch, StorageSaveOutcome};
use tokio::sync::oneshot;

use crate::{
    DocumentPersistence, PersistenceCaptureRequest, PersistenceFailure, PersistenceFailureKind,
    SessionIoExecutor, project_persistence_save_capture,
};

pub const DEFAULT_STORAGE_SAVE_DEBOUNCE: Duration = Duration::from_millis(250);

#[derive(Debug, Clone)]
pub struct StorageSaveRequest {
    session: DocumentPersistence,
    batch: StorageSaveBatch,
    generation: u64,
    revision: u64,
}

impl StorageSaveRequest {
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn saved_block_count(&self) -> usize {
        self.batch
            .payloads
            .len()
            .max(self.batch.index_records.len())
    }

    pub fn transactions(&self) -> &[cditor_core::edit::EditTransaction] {
        &self.batch.transactions
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersistenceBarrierKind {
    Save,
    Flush,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PersistencePipelineError {
    Cancelled,
    Unavailable(String),
    Storage(PersistenceFailure),
}

impl fmt::Display for PersistencePipelineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("persistence operation was cancelled"),
            Self::Unavailable(message) => formatter.write_str(message),
            Self::Storage(failure) => failure.fmt(formatter),
        }
    }
}

impl Error for PersistencePipelineError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistenceBarrierReport {
    pub revision: u64,
    pub saved_blocks: usize,
    pub duration: Duration,
}

#[derive(Debug)]
struct PendingPersistenceBarrier {
    kind: PersistenceBarrierKind,
    target_generation: u64,
    revision: u64,
    saved_blocks: usize,
    started_at: Instant,
    sender: oneshot::Sender<Result<PersistenceBarrierReport, PersistencePipelineError>>,
}

#[derive(Debug)]
pub struct ReadyPersistenceBarrier {
    revision: u64,
    saved_blocks: usize,
    started_at: Instant,
    sender: oneshot::Sender<Result<PersistenceBarrierReport, PersistencePipelineError>>,
}

impl ReadyPersistenceBarrier {
    pub fn resolve(self, result: Result<(), PersistencePipelineError>) {
        let report = result.map(|()| PersistenceBarrierReport {
            revision: self.revision,
            saved_blocks: self.saved_blocks,
            duration: self.started_at.elapsed(),
        });
        let _ = self.sender.send(report);
    }
}

#[derive(Debug, Default)]
pub struct PersistencePipeline {
    session: Option<DocumentPersistence>,
    debounce_scheduled: bool,
    saving: bool,
    flushes_in_flight: usize,
    last_saved_structure_version: Option<u64>,
    in_flight_structure_version: Option<u64>,
    dirty_generation: u64,
    persisted_generation: u64,
    last_saved_block_count: usize,
    barriers: Vec<PendingPersistenceBarrier>,
    autosave_interval: Option<Duration>,
}

impl PersistencePipeline {
    pub fn disabled() -> Self {
        Self::default()
    }

    pub fn for_session(session: DocumentPersistence, autosave_interval: Option<Duration>) -> Self {
        Self {
            session: Some(session),
            autosave_interval,
            ..Self::disabled()
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.session.is_some()
    }

    pub fn is_saving(&self) -> bool {
        self.saving || self.flushes_in_flight > 0
    }

    pub fn pending_operation_count(&self) -> usize {
        usize::from(self.saving)
            + usize::from(self.debounce_scheduled)
            + self.flushes_in_flight
            + self.barriers.len()
    }

    pub fn clear_scheduled_save(&mut self) {
        self.debounce_scheduled = false;
    }

    pub fn session(&self) -> Option<&DocumentPersistence> {
        self.session.as_ref()
    }

    pub fn set_session(
        &mut self,
        session: Option<DocumentPersistence>,
        autosave_interval: Option<Duration>,
    ) {
        self.cancel_barriers(PersistencePipelineError::Cancelled);
        self.session = session;
        self.autosave_interval = autosave_interval;
        self.debounce_scheduled = false;
        self.saving = false;
        self.flushes_in_flight = 0;
        self.last_saved_structure_version = None;
        self.in_flight_structure_version = None;
        self.dirty_generation = 0;
        self.persisted_generation = 0;
        self.last_saved_block_count = 0;
    }

    pub fn mark_loaded_structure_version(&mut self, structure_version: u64) {
        if self.session.is_some() {
            self.last_saved_structure_version = Some(structure_version);
        }
    }

    pub fn mark_dirty(&mut self) {
        self.dirty_generation = self.dirty_generation.saturating_add(1);
    }

    pub fn has_unpersisted_changes(&self) -> bool {
        self.dirty_generation > self.persisted_generation
    }

    /// Reserves one host timer. The host owns only the timer wake-up; dedupe
    /// and save-state policy remain here.
    pub fn request_autosave_timer(&mut self) -> Option<Duration> {
        if self.session.is_none() || self.saving || self.debounce_scheduled {
            return None;
        }
        let interval = self.autosave_interval?;
        self.debounce_scheduled = true;
        Some(interval)
    }

    pub fn request_barrier(
        &mut self,
        kind: PersistenceBarrierKind,
        revision: u64,
    ) -> oneshot::Receiver<Result<PersistenceBarrierReport, PersistencePipelineError>> {
        let (sender, receiver) = oneshot::channel();
        self.barriers.push(PendingPersistenceBarrier {
            kind,
            target_generation: self.dirty_generation,
            revision,
            saved_blocks: 0,
            started_at: Instant::now(),
            sender,
        });
        receiver
    }

    pub fn drain_ready_barriers(
        &mut self,
    ) -> (Vec<ReadyPersistenceBarrier>, Vec<ReadyPersistenceBarrier>) {
        let mut save = Vec::new();
        let mut flush = Vec::new();
        let mut pending = Vec::with_capacity(self.barriers.len());
        for barrier in self.barriers.drain(..) {
            if barrier.target_generation > self.persisted_generation
                || (barrier.kind == PersistenceBarrierKind::Flush && self.flushes_in_flight > 0)
            {
                pending.push(barrier);
                continue;
            }
            let ready = ReadyPersistenceBarrier {
                revision: barrier.revision,
                saved_blocks: barrier.saved_blocks,
                started_at: barrier.started_at,
                sender: barrier.sender,
            };
            match barrier.kind {
                PersistenceBarrierKind::Save => save.push(ready),
                PersistenceBarrierKind::Flush => flush.push(ready),
            }
        }
        self.barriers = pending;
        (save, flush)
    }

    pub fn fail_barriers(&mut self, failure: &PersistenceFailure) {
        self.cancel_barriers(PersistencePipelineError::Storage(failure.clone()));
    }

    pub fn begin_backend_flush(&mut self) {
        self.flushes_in_flight = self.flushes_in_flight.saturating_add(1);
    }

    pub fn finish_backend_flush(&mut self) {
        self.flushes_in_flight = self.flushes_in_flight.saturating_sub(1);
    }

    pub fn begin_batch(&mut self, runtime: &mut DocumentRuntime) -> Option<StorageSaveRequest> {
        let session = self.session.clone()?;
        if self.saving {
            return None;
        }
        self.debounce_scheduled = false;
        if !self.has_unpersisted_changes() {
            return None;
        }
        let capture = project_persistence_save_capture(
            runtime,
            PersistenceCaptureRequest {
                last_saved_structure_version: self.last_saved_structure_version,
                layout_key: session.layout_key(),
            },
        )?;
        if self.last_saved_structure_version.is_none() {
            self.last_saved_structure_version = Some(capture.structure_version);
        }
        self.saving = true;
        self.in_flight_structure_version = capture
            .includes_structure()
            .then_some(capture.structure_version);
        Some(StorageSaveRequest {
            session,
            batch: capture.batch,
            generation: self.dirty_generation,
            revision: capture.revision,
        })
    }

    pub fn finish_success(
        &mut self,
        request: &StorageSaveRequest,
        saved_structure_version: Option<u64>,
    ) -> bool {
        self.saving = false;
        if let Some(version) = saved_structure_version.or(self.in_flight_structure_version) {
            self.last_saved_structure_version = Some(version);
        }
        self.in_flight_structure_version = None;
        self.persisted_generation = self.persisted_generation.max(request.generation);
        self.last_saved_block_count = request.saved_block_count();
        for barrier in &mut self.barriers {
            if barrier.target_generation <= self.persisted_generation {
                barrier.saved_blocks = self.last_saved_block_count;
            }
        }
        self.has_unpersisted_changes()
    }

    pub fn finish_failed(&mut self, request: &StorageSaveRequest) -> bool {
        self.saving = false;
        self.in_flight_structure_version = None;
        self.dirty_generation > request.generation
    }

    fn cancel_barriers(&mut self, error: PersistencePipelineError) {
        for barrier in self.barriers.drain(..) {
            let _ = barrier.sender.send(Err(error.clone()));
        }
    }
}

pub async fn save_storage_batch(
    request: &StorageSaveRequest,
) -> Result<StorageSaveOutcome, PersistenceFailure> {
    let emergency_sequence =
        if request.session.capabilities().emergency_log && !request.batch.transactions.is_empty() {
            request
                .session
                .append_emergency_transactions(&request.batch.transactions)
                .await
                .map_err(PersistenceFailure::from)
                .map_err(|error| error.with_context("cannot write durable emergency log"))?
                .through_sequence
        } else {
            None
        };
    let outcome = request
        .session
        .commit(request.batch.clone())
        .await
        .map_err(PersistenceFailure::from)?;
    if let Some(sequence) = emergency_sequence {
        // A failed cleanup is harmless: adapters filter materialized
        // transactions on recovery and a later compaction can remove the row.
        let _ = request
            .session
            .acknowledge_emergency_transactions(sequence)
            .await;
    }
    Ok(outcome)
}

pub fn run_storage_save_with_timeout(
    request: &StorageSaveRequest,
    timeout: Duration,
) -> Result<StorageSaveOutcome, PersistenceFailure> {
    SessionIoExecutor::shared()
        .run(async {
            tokio::time::timeout(timeout, save_storage_batch(request))
                .await
                .map_err(|_| StorageError::Timeout {
                    operation: "storage save",
                    timeout,
                })
                .map_err(PersistenceFailure::from)?
        })
        .map_err(|message| PersistenceFailure::new(PersistenceFailureKind::Io, message))
        .and_then(|result| result)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use cditor_core::edit::{EditOperation, EditTransaction, EditTransactionKind};
    use cditor_storage::{
        DocumentStorage, LoadDocumentRequest, LoadedDocument, LoadedPayloadBatch,
        StorageBackendKind, StorageCapabilities, StorageError, StorageResult,
    };

    use super::*;

    #[derive(Debug)]
    struct NoopStorage;

    #[async_trait]
    impl DocumentStorage for NoopStorage {
        fn backend_kind(&self) -> StorageBackendKind {
            StorageBackendKind::Custom
        }

        fn capabilities(&self) -> StorageCapabilities {
            StorageCapabilities::SQLITE
        }

        async fn load_document(
            &self,
            _request: LoadDocumentRequest,
        ) -> StorageResult<LoadedDocument> {
            Err(StorageError::InvalidConfiguration(
                "test storage".to_owned(),
            ))
        }

        async fn load_payloads(
            &self,
            _document_id: cditor_core::ids::DocumentId,
            _block_ids: &[cditor_core::ids::BlockId],
        ) -> StorageResult<LoadedPayloadBatch> {
            Err(StorageError::InvalidConfiguration(
                "test storage".to_owned(),
            ))
        }

        async fn commit(&self, _batch: StorageSaveBatch) -> StorageResult<StorageSaveOutcome> {
            Err(StorageError::InvalidConfiguration(
                "test storage".to_owned(),
            ))
        }
    }

    #[derive(Debug)]
    struct DurableTestStorage {
        events: Mutex<Vec<&'static str>>,
        fail_commit: bool,
    }

    impl DurableTestStorage {
        fn new(fail_commit: bool) -> Self {
            Self {
                events: Mutex::new(Vec::new()),
                fail_commit,
            }
        }

        fn events(&self) -> Vec<&'static str> {
            self.events.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl DocumentStorage for DurableTestStorage {
        fn backend_kind(&self) -> StorageBackendKind {
            StorageBackendKind::Custom
        }

        fn capabilities(&self) -> StorageCapabilities {
            StorageCapabilities {
                emergency_log: true,
                ..StorageCapabilities::POSTGRES
            }
        }

        async fn load_document(
            &self,
            _request: LoadDocumentRequest,
        ) -> StorageResult<LoadedDocument> {
            Err(StorageError::InvalidConfiguration(
                "test storage".to_owned(),
            ))
        }

        async fn load_payloads(
            &self,
            _document_id: cditor_core::ids::DocumentId,
            _block_ids: &[cditor_core::ids::BlockId],
        ) -> StorageResult<LoadedPayloadBatch> {
            Err(StorageError::InvalidConfiguration(
                "test storage".to_owned(),
            ))
        }

        async fn append_emergency_transactions(
            &self,
            _document_id: cditor_core::ids::DocumentId,
            transactions: &[EditTransaction],
        ) -> StorageResult<cditor_storage::EmergencyLogAppendOutcome> {
            self.events.lock().unwrap().push("append");
            Ok(cditor_storage::EmergencyLogAppendOutcome {
                appended: transactions.len(),
                through_sequence: Some(9),
            })
        }

        async fn acknowledge_emergency_transactions(
            &self,
            _document_id: cditor_core::ids::DocumentId,
            _through_sequence: u64,
        ) -> StorageResult<u64> {
            self.events.lock().unwrap().push("ack");
            Ok(1)
        }

        async fn commit(&self, batch: StorageSaveBatch) -> StorageResult<StorageSaveOutcome> {
            self.events.lock().unwrap().push("commit");
            if self.fail_commit {
                Err(StorageError::Io("forced commit failure".to_owned()))
            } else {
                Ok(StorageSaveOutcome {
                    saved_structure_version: batch.saved_structure_version(),
                    saved_payload_versions: Vec::new(),
                })
            }
        }
    }

    fn durable_save_request(storage: Arc<DurableTestStorage>) -> StorageSaveRequest {
        let transaction = EditTransaction::new(
            1,
            EditTransactionKind::Typing,
            1,
            vec![EditOperation::InsertText {
                block_id: 1,
                offset: 0,
                text: "saved".to_owned(),
            }],
            vec![EditOperation::DeleteText {
                block_id: 1,
                range: 0..5,
            }],
        );
        StorageSaveRequest {
            session: DocumentPersistence::new(storage, 1),
            batch: StorageSaveBatch {
                document_id: 1,
                layout_key: None,
                payloads: Vec::new(),
                index_records: Vec::new(),
                structure_version: 1,
                transactions: vec![transaction],
                block_attrs: Vec::new(),
                page_layout_snapshot: None,
            },
            generation: 1,
            revision: 1,
        }
    }

    fn pipeline(autosave_interval: Option<Duration>) -> PersistencePipeline {
        PersistencePipeline::for_session(
            DocumentPersistence::new(Arc::new(NoopStorage), 1),
            autosave_interval,
        )
    }

    #[test]
    fn autosave_timer_is_deduplicated_and_disabled_without_policy_or_storage() {
        let mut active = pipeline(Some(DEFAULT_STORAGE_SAVE_DEBOUNCE));
        assert_eq!(
            active.request_autosave_timer(),
            Some(DEFAULT_STORAGE_SAVE_DEBOUNCE)
        );
        assert_eq!(active.request_autosave_timer(), None);
        active.clear_scheduled_save();
        assert_eq!(
            active.request_autosave_timer(),
            Some(DEFAULT_STORAGE_SAVE_DEBOUNCE)
        );

        assert_eq!(pipeline(None).request_autosave_timer(), None);
        assert_eq!(
            PersistencePipeline::disabled().request_autosave_timer(),
            None
        );
    }

    #[tokio::test]
    async fn save_barrier_waits_for_its_generation_but_not_newer_edits() {
        let mut runtime = DocumentRuntime::empty();
        let mut pipeline = pipeline(None);
        pipeline.mark_loaded_structure_version(runtime.structure_version());
        pipeline.mark_dirty();
        let receiver = pipeline.request_barrier(PersistenceBarrierKind::Save, 7);
        let request = pipeline.begin_batch(&mut runtime).unwrap();

        pipeline.mark_dirty();
        assert!(pipeline.finish_success(&request, None));
        let (ready, flush) = pipeline.drain_ready_barriers();
        assert_eq!(ready.len(), 1);
        assert!(flush.is_empty());
        ready
            .into_iter()
            .for_each(|barrier| barrier.resolve(Ok(())));

        let report = receiver.await.unwrap().unwrap();
        assert_eq!(report.revision, 7);
        assert_eq!(report.saved_blocks, 1);
        assert!(pipeline.has_unpersisted_changes());
    }

    #[tokio::test]
    async fn reconfiguration_cancels_pending_barriers() {
        let mut pipeline = pipeline(None);
        let receiver = pipeline.request_barrier(PersistenceBarrierKind::Flush, 3);

        pipeline.set_session(None, None);

        assert_eq!(
            receiver.await.unwrap(),
            Err(PersistencePipelineError::Cancelled)
        );
    }

    #[tokio::test]
    async fn flush_barrier_waits_for_backend_flush_completion() {
        let mut pipeline = pipeline(None);
        let receiver = pipeline.request_barrier(PersistenceBarrierKind::Flush, 4);
        pipeline.begin_backend_flush();
        let (_, blocked) = pipeline.drain_ready_barriers();
        assert!(blocked.is_empty());

        pipeline.finish_backend_flush();
        let (_, ready) = pipeline.drain_ready_barriers();
        assert_eq!(ready.len(), 1);
        ready
            .into_iter()
            .for_each(|barrier| barrier.resolve(Ok(())));
        assert_eq!(receiver.await.unwrap().unwrap().revision, 4);
    }

    #[tokio::test]
    async fn durable_log_wraps_primary_commit_and_is_acked_only_after_success() {
        let successful = Arc::new(DurableTestStorage::new(false));
        save_storage_batch(&durable_save_request(successful.clone()))
            .await
            .unwrap();
        assert_eq!(successful.events(), vec!["append", "commit", "ack"]);

        let failed = Arc::new(DurableTestStorage::new(true));
        assert!(
            save_storage_batch(&durable_save_request(failed.clone()))
                .await
                .is_err()
        );
        assert_eq!(failed.events(), vec!["append", "commit"]);
    }
}
