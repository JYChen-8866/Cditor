use std::time::{Duration, Instant};

use gpui::Context;
use tokio::sync::oneshot;

use crate::api::{CditorError, SaveReport};
use crate::gui::app::CditorV2View;
use crate::gui::persistence::EditorSaveStatus;
use cditor_core::layout::PAGE_POLICY_VERSION;
use cditor_runtime::DocumentRuntime;
use cditor_storage::{
    DOCUMENT_INDEX_VISIBLE_VERSION, StoragePageLayoutSnapshot, StorageSaveBatch,
    StorageSaveOutcome, StorageSession,
};

pub const DEFAULT_STORAGE_SAVE_DEBOUNCE: Duration = Duration::from_millis(250);

#[derive(Debug, Clone)]
pub struct StorageSaveRequest {
    session: StorageSession,
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

#[derive(Debug)]
struct PendingPersistenceBarrier {
    kind: PersistenceBarrierKind,
    target_generation: u64,
    revision: u64,
    saved_blocks: usize,
    started_at: Instant,
    sender: oneshot::Sender<Result<SaveReport, CditorError>>,
}

#[derive(Debug)]
pub struct ReadyPersistenceBarrier {
    revision: u64,
    saved_blocks: usize,
    started_at: Instant,
    sender: oneshot::Sender<Result<SaveReport, CditorError>>,
}

impl ReadyPersistenceBarrier {
    pub fn resolve(self, result: Result<(), CditorError>) {
        let report = result.map(|()| SaveReport {
            revision: self.revision,
            saved_blocks: self.saved_blocks,
            duration: self.started_at.elapsed(),
        });
        let _ = self.sender.send(report);
    }
}

#[derive(Debug, Default)]
pub struct StoragePersistenceState {
    session: Option<StorageSession>,
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

impl StoragePersistenceState {
    pub fn disabled() -> Self {
        Self::default()
    }

    pub fn for_session(session: StorageSession, autosave_interval: Option<Duration>) -> Self {
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

    pub fn session(&self) -> Option<&StorageSession> {
        self.session.as_ref()
    }

    pub fn set_session(
        &mut self,
        session: Option<StorageSession>,
        autosave_interval: Option<Duration>,
    ) {
        self.cancel_barriers(CditorError::Cancelled);
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

    pub fn request_barrier(
        &mut self,
        kind: PersistenceBarrierKind,
        revision: u64,
    ) -> oneshot::Receiver<Result<SaveReport, CditorError>> {
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
            if barrier.target_generation > self.persisted_generation {
                pending.push(barrier);
                continue;
            }
            if barrier.kind == PersistenceBarrierKind::Flush && self.flushes_in_flight > 0 {
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

    pub fn fail_barriers(&mut self, message: &str) {
        self.cancel_barriers(CditorError::Persistence(message.to_owned()));
    }

    pub fn begin_backend_flush(&mut self) {
        self.flushes_in_flight = self.flushes_in_flight.saturating_add(1);
    }

    pub fn finish_backend_flush(&mut self) {
        self.flushes_in_flight = self.flushes_in_flight.saturating_sub(1);
    }

    pub fn schedule(&mut self, cx: &mut Context<CditorV2View>) {
        if self.session.is_none() {
            return;
        }
        if self.saving {
            return;
        }
        let Some(autosave_interval) = self.autosave_interval else {
            return;
        };
        if self.debounce_scheduled {
            return;
        }
        self.debounce_scheduled = true;
        let debounce = cx.background_executor().timer(autosave_interval);
        cx.spawn(async move |view, cx| {
            debounce.await;
            let _ = view.update(cx, |view, cx| view.flush_storage_persistence(cx));
        })
        .detach();
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
        let transactions = runtime.drain_pending_structure_transactions();
        let payloads = runtime.loaded_payload_records_snapshot();
        let block_attrs = runtime.block_attrs_snapshot();
        let structure_version = runtime.structure_version();
        let should_save_structure = self
            .last_saved_structure_version
            .is_some_and(|saved| saved != structure_version)
            || !transactions.is_empty()
            || runtime.has_dirty_layout();
        if self.last_saved_structure_version.is_none() {
            self.last_saved_structure_version = Some(structure_version);
        }
        let index_records = should_save_structure
            .then(|| runtime.index_records_snapshot())
            .unwrap_or_default();

        if transactions.is_empty() && payloads.is_empty() && index_records.is_empty() {
            return None;
        }
        self.saving = true;
        self.in_flight_structure_version = (!index_records.is_empty()).then_some(structure_version);
        let layout_key = session.layout_key();
        let page_layout_snapshot = if should_save_structure {
            layout_key.and_then(|layout_key| {
                StoragePageLayoutSnapshot::from_page_layout(
                    DOCUMENT_INDEX_VISIBLE_VERSION,
                    structure_version,
                    layout_key,
                    PAGE_POLICY_VERSION,
                    &runtime.page_layout,
                    &runtime.visible_index.visible_block_ids,
                )
                .ok()
            })
        } else {
            None
        };
        Some(StorageSaveRequest {
            session,
            batch: StorageSaveBatch {
                document_id: runtime.document_id,
                layout_key,
                payloads,
                index_records,
                structure_version,
                transactions,
                block_attrs,
                page_layout_snapshot,
            },
            generation: self.dirty_generation,
            revision: runtime.revision(),
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

    fn cancel_barriers(&mut self, error: CditorError) {
        for barrier in self.barriers.drain(..) {
            let _ = barrier.sender.send(Err(error.clone()));
        }
    }
}

pub async fn save_storage_batch(
    request: &StorageSaveRequest,
) -> Result<StorageSaveOutcome, String> {
    request
        .session
        .commit(request.batch.clone())
        .await
        .map_err(|error| error.to_string())
}

pub fn mark_dirty_and_schedule_save(
    persistence: &mut StoragePersistenceState,
    save_status: &mut EditorSaveStatus,
    cx: &mut Context<CditorV2View>,
) {
    *save_status = EditorSaveStatus::Dirty;
    persistence.mark_dirty();
    persistence.schedule(cx);
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use cditor_core::rich_text::{BlockPayloadRecord, RichBlockKind};
    use cditor_storage::layout_cache::LayoutCacheKey;
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

    fn persistence() -> StoragePersistenceState {
        StoragePersistenceState::for_session(
            StorageSession::new(Arc::new(NoopStorage), 1),
            Some(DEFAULT_STORAGE_SAVE_DEBOUNCE),
        )
    }

    fn layout_key() -> LayoutCacheKey {
        LayoutCacheKey {
            width_bucket: 10,
            exact_width_px: 800,
            content_version: 1,
            attrs_version: 0,
            style_version: 0,
            font_version: 0,
            theme_version: 0,
            scale_factor_milli: 1_000,
        }
    }

    #[test]
    fn captured_payload_versions_are_exact() {
        let mut first = BlockPayloadRecord::rich_text(7, RichBlockKind::Paragraph, "first");
        first.content_version = 12;
        let mut second = BlockPayloadRecord::rich_text(9, RichBlockKind::Paragraph, "second");
        second.content_version = 4;
        let versions = [first, second]
            .iter()
            .map(|payload| (payload.block_id, payload.content_version))
            .collect::<Vec<_>>();
        assert_eq!(versions, vec![(7, 12), (9, 4)]);
    }

    #[tokio::test]
    async fn save_barrier_waits_for_its_generation_but_not_newer_edits() {
        let mut runtime = DocumentRuntime::empty();
        let mut persistence = persistence();
        persistence.mark_loaded_structure_version(runtime.structure_version());
        persistence.mark_dirty();
        let receiver = persistence.request_barrier(PersistenceBarrierKind::Save, 7);
        let request = persistence.begin_batch(&mut runtime).unwrap();

        persistence.mark_dirty();
        assert!(persistence.finish_success(&request, None));
        let (ready, flush) = persistence.drain_ready_barriers();
        assert_eq!(ready.len(), 1);
        assert!(flush.is_empty());
        ready
            .into_iter()
            .for_each(|barrier| barrier.resolve(Ok(())));

        let report = receiver.await.unwrap().unwrap();
        assert_eq!(report.revision, 7);
        assert_eq!(report.saved_blocks, 1);
        assert!(persistence.has_unpersisted_changes());
    }

    #[test]
    fn failed_batch_transactions_can_be_restored_ahead_of_new_work() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            (1..=3)
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
        assert!(runtime.move_block_subtree_before(1, Some(3)).unwrap());
        let mut persistence = persistence();
        persistence.mark_loaded_structure_version(1);
        persistence.mark_dirty();

        let request = persistence.begin_batch(&mut runtime).unwrap();
        assert_eq!(request.transactions().len(), 1);
        assert_eq!(runtime.pending_structure_transaction_count(), 0);
        assert!(!persistence.finish_failed(&request));

        runtime.restore_pending_structure_transactions(request.transactions().to_vec());
        assert_eq!(runtime.pending_structure_transaction_count(), 1);
        let retry = persistence.begin_batch(&mut runtime).unwrap();
        assert_eq!(retry.transactions(), request.transactions());
    }

    #[test]
    fn structural_save_captures_page_layout_with_visible_block_boundaries() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            (1..=3)
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
        let session = StorageSession::new(Arc::new(NoopStorage), 1).with_layout_key(layout_key());
        let mut persistence =
            StoragePersistenceState::for_session(session, Some(DEFAULT_STORAGE_SAVE_DEBOUNCE));
        persistence.mark_loaded_structure_version(0);
        persistence.mark_dirty();

        let request = persistence.begin_batch(&mut runtime).unwrap();
        let snapshot = request.batch.page_layout_snapshot.as_ref().unwrap();
        assert_eq!(snapshot.structure_version, runtime.structure_version());
        assert_eq!(snapshot.pages[0].first_block_id, 1);
        assert_eq!(snapshot.pages.last().unwrap().last_block_id, 3);
    }

    #[tokio::test]
    async fn clean_flush_barrier_is_ready_without_creating_a_save_batch() {
        let mut runtime = DocumentRuntime::empty();
        let mut persistence = persistence();
        persistence.mark_loaded_structure_version(runtime.structure_version());
        let receiver = persistence.request_barrier(PersistenceBarrierKind::Flush, 1);

        assert!(persistence.begin_batch(&mut runtime).is_none());
        let (save, flush) = persistence.drain_ready_barriers();
        assert!(save.is_empty());
        assert_eq!(flush.len(), 1);
        flush
            .into_iter()
            .for_each(|barrier| barrier.resolve(Ok(())));
        assert_eq!(receiver.await.unwrap().unwrap().saved_blocks, 0);
    }
}
