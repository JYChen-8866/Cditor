use cditor_core::edit::EditTransaction;
use cditor_core::layout::PAGE_POLICY_VERSION;
use cditor_editor_protocol::ProtocolError;
use cditor_runtime::DocumentRuntime;
use cditor_storage::layout_cache::LayoutCacheKey;
use cditor_storage::{
    DOCUMENT_INDEX_VISIBLE_VERSION, StoragePageLayoutSnapshot, StorageSaveBatch, StorageSaveOutcome,
};

use crate::EditorSessionHandle;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PersistenceCaptureRequest {
    pub last_saved_structure_version: Option<u64>,
    pub layout_key: Option<LayoutCacheKey>,
}

#[derive(Debug, Clone)]
pub struct PersistenceSaveCapture {
    pub revision: u64,
    pub structure_version: u64,
    pub batch: StorageSaveBatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PersistenceRuntimeSnapshot {
    pub revision: u64,
    pub structure_version: u64,
    pub pending_structure_transactions: usize,
    pub last_committed_transaction_id: Option<u64>,
}

impl PersistenceSaveCapture {
    pub fn includes_structure(&self) -> bool {
        !self.batch.index_records.is_empty()
    }
}

pub fn project_persistence_runtime_snapshot(
    runtime: &DocumentRuntime,
) -> PersistenceRuntimeSnapshot {
    PersistenceRuntimeSnapshot {
        revision: runtime.revision(),
        structure_version: runtime.structure_version(),
        pending_structure_transactions: runtime.pending_structure_transaction_count(),
        last_committed_transaction_id: runtime.last_committed_transaction_id(),
    }
}

pub fn project_note_content_changed(runtime: &mut DocumentRuntime) -> PersistenceRuntimeSnapshot {
    runtime.note_content_changed();
    project_persistence_runtime_snapshot(runtime)
}

pub fn project_persistence_save_capture(
    runtime: &mut DocumentRuntime,
    request: PersistenceCaptureRequest,
) -> Option<PersistenceSaveCapture> {
    let transactions = runtime.drain_pending_structure_transactions();
    let payloads = runtime.loaded_payload_records_snapshot();
    let block_attrs = runtime.block_attrs_snapshot();
    let structure_version = runtime.structure_version();
    let should_save_structure = request
        .last_saved_structure_version
        .is_some_and(|saved| saved != structure_version)
        || !transactions.is_empty()
        || runtime.has_dirty_layout();
    let index_records = if should_save_structure {
        runtime.index_records_snapshot()
    } else {
        Default::default()
    };

    if transactions.is_empty() && payloads.is_empty() && index_records.is_empty() {
        return None;
    }
    let page_layout_snapshot = if should_save_structure {
        request.layout_key.and_then(|layout_key| {
            StoragePageLayoutSnapshot::from_page_layout(
                DOCUMENT_INDEX_VISIBLE_VERSION,
                structure_version,
                layout_key,
                PAGE_POLICY_VERSION,
                &runtime.page_layout_snapshot(),
                runtime.visible_block_ids(),
            )
            .ok()
        })
    } else {
        None
    };
    Some(PersistenceSaveCapture {
        revision: runtime.revision(),
        structure_version,
        batch: StorageSaveBatch {
            document_id: runtime.document_id(),
            icon_json: runtime
                .page_icon()
                .and_then(|icon| serde_json::to_string(icon).ok()),
            cover_json: runtime
                .page_cover()
                .and_then(|cover| serde_json::to_string(cover).ok()),
            layout_key: request.layout_key,
            payloads,
            index_records,
            structure_version,
            transactions,
            block_attrs,
            page_layout_snapshot,
        },
    })
}

pub fn project_persistence_save_success(
    runtime: &mut DocumentRuntime,
    outcome: &StorageSaveOutcome,
    saved_layout_or_structure: bool,
    should_reschedule: bool,
) {
    runtime.mark_payload_versions_persisted(&outcome.saved_payload_versions);
    if saved_layout_or_structure && !should_reschedule {
        runtime.mark_layout_saved();
    }
}

pub fn project_persistence_save_failure(
    runtime: &mut DocumentRuntime,
    transactions: Vec<EditTransaction>,
) {
    runtime.restore_pending_structure_transactions(transactions);
}

impl EditorSessionHandle {
    pub fn persistence_runtime_snapshot(
        &self,
    ) -> Result<PersistenceRuntimeSnapshot, ProtocolError> {
        let session = self.inner.try_borrow().map_err(|_| {
            ProtocolError::new(
                cditor_editor_protocol::ProtocolErrorCode::Busy,
                "editor session is already processing a synchronous request",
            )
            .retryable()
        })?;
        Ok(project_persistence_runtime_snapshot(&session.runtime))
    }

    pub fn record_content_changed(&self) -> Result<PersistenceRuntimeSnapshot, ProtocolError> {
        Ok(project_note_content_changed(
            &mut self.try_session_mut()?.runtime,
        ))
    }

    pub fn capture_persistence_save(
        &self,
        request: PersistenceCaptureRequest,
    ) -> Result<Option<PersistenceSaveCapture>, ProtocolError> {
        Ok(project_persistence_save_capture(
            &mut self.try_session_mut()?.runtime,
            request,
        ))
    }

    pub fn apply_persistence_save_success(
        &self,
        outcome: &StorageSaveOutcome,
        saved_layout_or_structure: bool,
        should_reschedule: bool,
    ) -> Result<(), ProtocolError> {
        project_persistence_save_success(
            &mut self.try_session_mut()?.runtime,
            outcome,
            saved_layout_or_structure,
            should_reschedule,
        );
        Ok(())
    }

    pub fn apply_persistence_save_failure(
        &self,
        transactions: Vec<EditTransaction>,
    ) -> Result<(), ProtocolError> {
        project_persistence_save_failure(&mut self.try_session_mut()?.runtime, transactions);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use cditor_core::rich_text::{BlockPayloadRecord, PageCover, PageIcon, RichBlockKind};
    use cditor_editor_protocol::command::{CommandEnvelope, CommandSource, EditorCommand};

    use super::*;

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

    fn structural_runtime() -> DocumentRuntime {
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
        runtime
            .dispatch(CommandEnvelope::new(
                EditorCommand::MoveBlockBefore {
                    block_id: 1,
                    before_block_id: Some(3),
                },
                CommandSource::Sdk,
            ))
            .unwrap();
        runtime
    }

    #[test]
    fn capture_atomically_drains_transactions_and_owns_save_payloads() {
        let mut runtime = structural_runtime();
        let revision = runtime.revision();
        let capture = project_persistence_save_capture(
            &mut runtime,
            PersistenceCaptureRequest {
                last_saved_structure_version: Some(1),
                layout_key: None,
            },
        )
        .unwrap();

        assert_eq!(capture.revision, revision);
        assert_eq!(capture.batch.transactions.len(), 1);
        assert_eq!(capture.batch.payloads.len(), 3);
        assert!(capture.includes_structure());
        assert_eq!(runtime.pending_structure_transaction_count(), 0);
    }

    #[test]
    fn capture_serializes_page_decorations_with_the_document_batch() {
        let mut runtime = DocumentRuntime::empty();
        runtime
            .dispatch(CommandEnvelope::new(
                EditorCommand::SetPageCover {
                    source: Some("/tmp/cover.png".to_owned()),
                    position_y_milli: 375,
                },
                CommandSource::Toolbar,
            ))
            .unwrap();
        runtime
            .dispatch(CommandEnvelope::new(
                EditorCommand::SetPageIconEmoji {
                    emoji: Some("💡".to_owned()),
                },
                CommandSource::Toolbar,
            ))
            .unwrap();

        let capture = project_persistence_save_capture(
            &mut runtime,
            PersistenceCaptureRequest {
                last_saved_structure_version: None,
                layout_key: None,
            },
        )
        .unwrap();

        let cover: PageCover =
            serde_json::from_str(capture.batch.cover_json.as_deref().unwrap()).unwrap();
        let icon: PageIcon =
            serde_json::from_str(capture.batch.icon_json.as_deref().unwrap()).unwrap();
        assert!(matches!(cover, PageCover::Asset { .. }));
        assert_eq!(
            icon,
            PageIcon::Emoji {
                emoji: "💡".to_owned()
            }
        );
    }

    #[test]
    fn failed_capture_restores_transactions_ahead_of_retry() {
        let mut runtime = structural_runtime();
        let capture = project_persistence_save_capture(
            &mut runtime,
            PersistenceCaptureRequest {
                last_saved_structure_version: Some(1),
                layout_key: None,
            },
        )
        .unwrap();

        project_persistence_save_failure(&mut runtime, capture.batch.transactions.clone());

        assert_eq!(runtime.pending_structure_transaction_count(), 1);
        let retry = project_persistence_save_capture(
            &mut runtime,
            PersistenceCaptureRequest {
                last_saved_structure_version: Some(1),
                layout_key: None,
            },
        )
        .unwrap();
        assert_eq!(retry.batch.transactions, capture.batch.transactions);
    }

    #[test]
    fn dirty_notification_returns_monotonic_runtime_identity() {
        let mut runtime = DocumentRuntime::empty();
        let before = project_persistence_runtime_snapshot(&runtime);

        let after = project_note_content_changed(&mut runtime);

        assert!(after.revision > before.revision);
        assert_eq!(after.structure_version, before.structure_version);
    }

    #[test]
    fn structural_capture_persists_page_boundaries_with_layout_identity() {
        let mut runtime = structural_runtime();
        let capture = project_persistence_save_capture(
            &mut runtime,
            PersistenceCaptureRequest {
                last_saved_structure_version: Some(0),
                layout_key: Some(layout_key()),
            },
        )
        .unwrap();
        let snapshot = capture.batch.page_layout_snapshot.unwrap();

        assert_eq!(snapshot.structure_version, capture.structure_version);
        assert_eq!(snapshot.pages[0].first_block_id, 2);
        assert_eq!(snapshot.pages.last().unwrap().last_block_id, 3);
    }
}
