use std::time::Duration;

use gpui::Context;
use sqlx::PgPool;

use crate::gui::app::CditorV2View;
use crate::gui::persistence::EditorSaveStatus;
use cditor_core::document::BlockIndexRecord;
use cditor_core::edit::EditTransaction;
use cditor_core::ids::BlockId;
use cditor_core::rich_text::{BlockAttrs, BlockPayloadRecord};
use cditor_runtime::DocumentRuntime;
use cditor_storage::DOCUMENT_INDEX_VISIBLE_VERSION;
use cditor_storage_postgres::{
    EditTransactionVersions, PgDocumentId, PostgresDocumentStore, PostgresPayloadStore,
    PostgresTransactionStore, pg_document_id_from_runtime,
};

pub const DEFAULT_POSTGRES_SAVE_DEBOUNCE: Duration = Duration::from_millis(250);

#[derive(Debug, Clone)]
pub struct PostgresPersistenceTarget {
    pub document_id: PgDocumentId,
    pub pool: PgPool,
}

impl PostgresPersistenceTarget {
    pub fn from_runtime_document_id(document_id: u64, pool: PgPool) -> Self {
        Self {
            document_id: pg_document_id_from_runtime(document_id),
            pool,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PostgresSaveBatch {
    document_id: PgDocumentId,
    pool: PgPool,
    payloads: Vec<BlockPayloadRecord>,
    index_records: Vec<BlockIndexRecord>,
    structure_version: u64,
    transactions: Vec<EditTransaction>,
    block_attrs: Vec<(BlockId, BlockAttrs)>,
}

impl PostgresSaveBatch {
    fn saved_structure_version(&self) -> Option<u64> {
        (!self.index_records.is_empty()).then_some(self.structure_version)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostgresSaveOutcome {
    pub saved_structure_version: Option<u64>,
    pub saved_payload_versions: Vec<(BlockId, u64)>,
}

fn payload_versions(payloads: &[BlockPayloadRecord]) -> Vec<(BlockId, u64)> {
    payloads
        .iter()
        .map(|payload| (payload.block_id, payload.content_version))
        .collect()
}

#[derive(Debug, Default)]
pub struct PostgresPersistenceState {
    target: Option<PostgresPersistenceTarget>,
    debounce_scheduled: bool,
    saving: bool,
    dirty_while_saving: bool,
    last_saved_structure_version: Option<u64>,
    in_flight_structure_version: Option<u64>,
    autosave_interval: Duration,
}

impl PostgresPersistenceState {
    pub fn disabled() -> Self {
        Self {
            autosave_interval: DEFAULT_POSTGRES_SAVE_DEBOUNCE,
            ..Self::default()
        }
    }

    pub fn for_target(target: PostgresPersistenceTarget, autosave_interval: Duration) -> Self {
        Self {
            target: Some(target),
            autosave_interval,
            ..Self::disabled()
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.target.is_some()
    }

    pub fn target(&self) -> Option<&PostgresPersistenceTarget> {
        self.target.as_ref()
    }

    pub fn set_target(
        &mut self,
        target: Option<PostgresPersistenceTarget>,
        autosave_interval: Duration,
    ) {
        self.target = target;
        self.autosave_interval = autosave_interval;
        self.debounce_scheduled = false;
        self.saving = false;
        self.dirty_while_saving = false;
        self.last_saved_structure_version = None;
        self.in_flight_structure_version = None;
    }

    pub fn mark_loaded_structure_version(&mut self, structure_version: u64) {
        if self.target.is_some() {
            self.last_saved_structure_version = Some(structure_version);
        }
    }

    pub fn schedule(&mut self, cx: &mut Context<CditorV2View>) {
        if self.target.is_none() {
            return;
        }
        if self.saving {
            self.dirty_while_saving = true;
            return;
        }
        if self.debounce_scheduled {
            return;
        }
        self.debounce_scheduled = true;

        let debounce = cx.background_executor().timer(self.autosave_interval);

        cx.spawn(async move |view, cx| {
            debounce.await;
            let _ = view.update(cx, |view, cx| {
                view.flush_postgres_persistence(cx);
            });
        })
        .detach();
    }

    pub fn begin_batch(&mut self, runtime: &mut DocumentRuntime) -> Option<PostgresSaveBatch> {
        let target = self.target.clone()?;
        if self.saving {
            self.dirty_while_saving = true;
            return None;
        }

        self.debounce_scheduled = false;
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
        Some(PostgresSaveBatch {
            document_id: target.document_id,
            pool: target.pool,
            payloads,
            index_records,
            structure_version,
            transactions,
            block_attrs,
        })
    }

    pub fn finish_success(&mut self, saved_structure_version: Option<u64>) -> bool {
        self.saving = false;
        if let Some(version) = saved_structure_version.or(self.in_flight_structure_version) {
            self.last_saved_structure_version = Some(version);
        }
        self.in_flight_structure_version = None;
        let should_reschedule = self.dirty_while_saving;
        self.dirty_while_saving = false;
        should_reschedule
    }

    pub fn finish_failed(&mut self) {
        self.saving = false;
        self.in_flight_structure_version = None;
    }
}

pub async fn save_postgres_batch(batch: PostgresSaveBatch) -> Result<PostgresSaveOutcome, String> {
    let saved_structure_version = batch.saved_structure_version();
    let saved_payload_versions = payload_versions(&batch.payloads);
    let document_store = PostgresDocumentStore::new(batch.pool.clone());
    let payload_store = PostgresPayloadStore::new(batch.pool.clone());
    let transaction_store = PostgresTransactionStore::new(batch.pool.clone());

    let structure_version = i64::try_from(batch.structure_version).map_err(|_| {
        format!(
            "structure version {} exceeds BIGINT",
            batch.structure_version
        )
    })?;
    if !batch.index_records.is_empty() {
        document_store
            .save_block_index_records(batch.document_id, &batch.index_records, structure_version)
            .await
            .map_err(|error| error.to_string())?;
        document_store
            .save_document_index_snapshot(
                batch.document_id,
                DOCUMENT_INDEX_VISIBLE_VERSION,
                structure_version,
                &batch.index_records,
            )
            .await
            .map_err(|error| error.to_string())?;
    }

    document_store
        .save_block_attrs(batch.document_id, &batch.block_attrs)
        .await
        .map_err(|error| error.to_string())?;

    // Payload rows reference `blocks`; persist structural changes first so newly
    // inserted/split blocks exist before `save_block_payloads` updates them.
    if !batch.payloads.is_empty() {
        payload_store
            .save_block_payloads(batch.document_id, &batch.payloads)
            .await
            .map_err(|error| error.to_string())?;
    }

    for transaction in &batch.transactions {
        transaction_store
            .save_edit_transaction(
                batch.document_id,
                transaction,
                EditTransactionVersions {
                    structure_version_before: structure_version.checked_sub(1),
                    structure_version_after: Some(structure_version),
                    content_version_after: batch
                        .payloads
                        .iter()
                        .map(|payload| payload.content_version)
                        .max()
                        .and_then(|version| i64::try_from(version).ok()),
                },
            )
            .await
            .map_err(|error| error.to_string())?;
    }

    Ok(PostgresSaveOutcome {
        saved_structure_version,
        saved_payload_versions,
    })
}

pub fn mark_dirty_and_schedule_postgres_save(
    persistence: &mut PostgresPersistenceState,
    save_status: &mut EditorSaveStatus,
    cx: &mut Context<CditorV2View>,
) {
    *save_status = EditorSaveStatus::Dirty;
    persistence.schedule(cx);
}

#[cfg(test)]
mod tests {
    use super::*;
    use cditor_core::rich_text::RichBlockKind;

    #[test]
    fn save_acknowledgement_captures_exact_snapshot_versions() {
        let mut first = BlockPayloadRecord::rich_text(7, RichBlockKind::Paragraph, "first");
        first.content_version = 12;
        let mut second = BlockPayloadRecord::rich_text(9, RichBlockKind::Paragraph, "second");
        second.content_version = 4;

        assert_eq!(payload_versions(&[first, second]), vec![(7, 12), (9, 4)]);
    }
}
