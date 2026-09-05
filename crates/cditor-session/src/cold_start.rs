use cditor_core::layout::PageLayoutIndex;
use cditor_core::rich_text::BlockPayloadRecord;
use cditor_editor_protocol::{ProtocolError, ProtocolErrorCode};
use cditor_runtime::content::payload_window::PayloadWindowLoadResult;
use cditor_runtime::document_runtime::{
    DocumentRuntimeColdStartData, DocumentRuntimeColdStartReport,
};
use cditor_storage::EmergencyLogEntry;

use crate::{
    EditorSession, EditorSessionHandle, EmergencyRecoveryDecision, EmergencyRecoveryReport,
    PersistencePipeline, plan_emergency_recovery, project_emergency_recovery,
};

#[derive(Debug, Clone, PartialEq)]
pub struct SessionColdStartRequest {
    pub data: DocumentRuntimeColdStartData,
    pub viewport_height: f64,
    pub cached_page_layout: Option<PageLayoutIndex>,
    pub readonly: bool,
    pub emergency_log_entries: Vec<EmergencyLogEntry>,
    pub emergency_payloads: Vec<BlockPayloadRecord>,
}

#[derive(Debug)]
pub struct SessionColdStartResult {
    pub session: EditorSessionHandle,
    pub report: DocumentRuntimeColdStartReport,
    pub emergency_recovery: Option<EmergencyRecoveryReport>,
    pub emergency_newer_major: Option<u32>,
}

pub struct PreparedEditorSession {
    session: EditorSession,
}

impl std::fmt::Debug for PreparedEditorSession {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PreparedEditorSession")
            .field("snapshot", &self.session.snapshot())
            .finish_non_exhaustive()
    }
}

impl PreparedEditorSession {
    pub fn into_handle(self) -> EditorSessionHandle {
        self.session.into_handle()
    }
}

#[derive(Debug)]
pub struct PreparedSessionColdStartResult {
    pub session: PreparedEditorSession,
    pub report: DocumentRuntimeColdStartReport,
    pub emergency_recovery: Option<EmergencyRecoveryReport>,
    pub emergency_newer_major: Option<u32>,
}

impl PreparedSessionColdStartResult {
    pub fn into_opened(self) -> SessionColdStartResult {
        SessionColdStartResult {
            session: self.session.into_handle(),
            report: self.report,
            emergency_recovery: self.emergency_recovery,
            emergency_newer_major: self.emergency_newer_major,
        }
    }
}

/// Opens a session from storage-neutral document data.
///
/// Storage adapters remain responsible for I/O and cache DTO decoding. This
/// boundary owns runtime construction, initial payload hydration and cached
/// page-layout fallback so a host never has to take `DocumentRuntime` back.
pub fn open_editor_session(
    request: SessionColdStartRequest,
) -> Result<SessionColdStartResult, ProtocolError> {
    open_editor_session_with_persistence(request, PersistencePipeline::disabled())
}

pub fn open_editor_session_with_persistence(
    request: SessionColdStartRequest,
    persistence: PersistencePipeline,
) -> Result<SessionColdStartResult, ProtocolError> {
    prepare_editor_session_with_persistence(request, persistence)
        .map(PreparedSessionColdStartResult::into_opened)
}

pub fn prepare_editor_session_with_persistence(
    request: SessionColdStartRequest,
    persistence: PersistencePipeline,
) -> Result<PreparedSessionColdStartResult, ProtocolError> {
    let document_id = request.data.document_id;
    let (mut runtime, mut report) = cditor_runtime::DocumentRuntime::from_cold_start_data(
        request.data,
        request.viewport_height,
    )
    .map_err(|message| {
        ProtocolError::new(ProtocolErrorCode::ApplyFailed, message).with_document(document_id)
    })?;

    if let Some(page_layout) = request.cached_page_layout
        && runtime.apply_cached_page_layout(page_layout).is_ok()
    {
        report.page_layout_cache_hit = true;
    }

    let (emergency_recovery, emergency_newer_major) =
        match plan_emergency_recovery(request.emergency_log_entries).map_err(|message| {
            ProtocolError::new(ProtocolErrorCode::ApplyFailed, message).with_document(document_id)
        })? {
            EmergencyRecoveryDecision::Replay(plan) if !request.readonly => {
                hydrate_emergency_payloads(
                    &mut runtime,
                    &plan,
                    request.emergency_payloads,
                    document_id,
                )?;
                let report = project_emergency_recovery(&mut runtime, plan).map_err(|message| {
                    ProtocolError::new(ProtocolErrorCode::ApplyFailed, message)
                        .with_document(document_id)
                })?;
                (Some(report), None)
            }
            EmergencyRecoveryDecision::Replay(_) => (None, None),
            EmergencyRecoveryDecision::ReadOnlyNewerMajor { written_major, .. } => {
                (None, Some(written_major))
            }
        };
    let readonly = request.readonly || emergency_newer_major.is_some();

    let recovered_transactions = emergency_recovery
        .as_ref()
        .map_or(0, |report| report.replayed_transactions);
    let structure_version = runtime.structure_version();
    let mut session = EditorSession::with_persistence(runtime, readonly, persistence);
    session
        .persistence
        .mark_loaded_structure_version(structure_version);
    if recovered_transactions > 0 && !readonly {
        session.persistence.mark_dirty();
    }

    Ok(PreparedSessionColdStartResult {
        session: PreparedEditorSession { session },
        report,
        emergency_recovery,
        emergency_newer_major,
    })
}

fn hydrate_emergency_payloads(
    runtime: &mut cditor_runtime::DocumentRuntime,
    plan: &crate::EmergencyRecoveryPlan,
    records: Vec<BlockPayloadRecord>,
    document_id: u64,
) -> Result<(), ProtocolError> {
    let Some(load_request) =
        crate::project_emergency_payload_request(runtime, plan).map_err(|message| {
            ProtocolError::new(ProtocolErrorCode::ApplyFailed, message).with_document(document_id)
        })?
    else {
        return Ok(());
    };
    let loaded_ids = records
        .iter()
        .map(|payload| payload.block_id)
        .collect::<std::collections::HashSet<_>>();
    let missing_block_ids = load_request
        .block_ids
        .iter()
        .copied()
        .filter(|block_id| !loaded_ids.contains(block_id))
        .collect();
    crate::project_emergency_payload_result(
        runtime,
        PayloadWindowLoadResult::prepare(load_request, records, missing_block_ids),
    )
    .map_err(|message| {
        ProtocolError::new(ProtocolErrorCode::ApplyFailed, message).with_document(document_id)
    })
}

#[cfg(test)]
mod tests {
    use cditor_core::{
        document::BlockIndexRecord,
        edit::{
            ChangeOrigin, EditOperation, EditTransaction, EditTransactionKind, encode_transaction,
        },
        layout::{HeightConfidence, PageLayout, PageLayoutIndex, PagePolicy},
        rich_text::{BlockAttrs, BlockPayloadRecord, RichBlockKind, kind_tag_for_rich_block_kind},
    };
    use cditor_editor_protocol::query::{CommandQuery, QueryResult};
    use cditor_runtime::document_runtime::{
        DocumentRuntimeColdStartData, DocumentRuntimeIndexSource,
    };

    use super::*;

    fn cold_start_data(payload_count: u64) -> DocumentRuntimeColdStartData {
        let records = (1..=2)
            .map(|block_id| {
                BlockIndexRecord::new(
                    block_id,
                    None,
                    0,
                    kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph),
                    0,
                )
            })
            .collect();
        DocumentRuntimeColdStartData {
            document_id: 9,
            document_title: "Loaded".to_owned(),
            page_cover: None,
            page_icon: None,
            structure_version: 3,
            records,
            block_attrs: vec![(1, BlockAttrs::default())],
            initial_payloads: (1..=payload_count)
                .map(|block_id| {
                    BlockPayloadRecord::rich_text(
                        block_id,
                        RichBlockKind::Paragraph,
                        block_id.to_string(),
                    )
                })
                .collect(),
            initial_payload_window_end: 2,
            index_source: DocumentRuntimeIndexSource::Snapshot,
            layout_cache_hits: 1,
        }
    }

    fn cached_page_layout(policy: PagePolicy) -> PageLayoutIndex {
        PageLayoutIndex::from_cached_pages(
            vec![PageLayout {
                page_index: 0,
                block_start: 0,
                block_count: 2,
                height: 321.0,
                measured_ratio: 1.0,
                confidence: HeightConfidence::Exact,
                max_error_hint: 0.0,
                dirty: false,
            }],
            policy,
            2,
        )
        .unwrap()
        .with_identity(cditor_core::layout::PageLayoutIdentity::for_page(
            9,
            3,
            0,
            1,
            cditor_core::layout::PAGE_POLICY_VERSION,
            0,
        ))
    }

    fn emergency_entry() -> EmergencyLogEntry {
        let transaction = EditTransaction::new(
            10,
            EditTransactionKind::Typing,
            10,
            vec![EditOperation::InsertText {
                block_id: 1,
                offset: 1,
                text: " recovered".to_owned(),
            }],
            vec![EditOperation::DeleteText {
                block_id: 1,
                range: 1..11,
            }],
        )
        .with_origin(ChangeOrigin::User);
        EmergencyLogEntry {
            sequence: 3,
            transaction_id: transaction.id,
            envelope: encode_transaction(&transaction).unwrap(),
        }
    }

    fn deferred_emergency_fixture() -> (
        DocumentRuntimeColdStartData,
        EmergencyLogEntry,
        BlockPayloadRecord,
    ) {
        let mut data = cold_start_data(2);
        data.records.push(BlockIndexRecord::new(
            3,
            None,
            0,
            kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph),
            0,
        ));
        let transaction = EditTransaction::new(
            11,
            EditTransactionKind::Typing,
            11,
            vec![EditOperation::InsertText {
                block_id: 3,
                offset: 1,
                text: " recovered".to_owned(),
            }],
            vec![EditOperation::DeleteText {
                block_id: 3,
                range: 1..11,
            }],
        )
        .with_origin(ChangeOrigin::User);
        (
            data,
            EmergencyLogEntry {
                sequence: 4,
                transaction_id: transaction.id,
                envelope: encode_transaction(&transaction).unwrap(),
            },
            BlockPayloadRecord::rich_text(3, RichBlockKind::Paragraph, "3"),
        )
    }

    #[test]
    fn cold_start_hydrates_initial_payloads_and_returns_the_runtime_owner() {
        let result = open_editor_session(SessionColdStartRequest {
            data: cold_start_data(2),
            viewport_height: 720.0,
            cached_page_layout: Some(cached_page_layout(PagePolicy::default())),
            readonly: false,
            emergency_log_entries: Vec::new(),
            emergency_payloads: Vec::new(),
        })
        .unwrap();

        let snapshot = result.session.snapshot().unwrap();
        assert_eq!(snapshot.document_id, 9);
        assert_eq!(snapshot.block_count, 3);
        assert_eq!(
            result.session.block_plain_text(1).unwrap().as_deref(),
            Some("1")
        );
        assert_eq!(result.report.payloads_loaded, 3);
        assert_eq!(result.report.payloads_missing, 0);
        // Injecting the migrated title changes the cached page's block count,
        // so the old body-only layout must be discarded.
        assert!(!result.report.page_layout_cache_hit);
    }

    #[test]
    fn cold_start_rejects_an_incomplete_initial_payload_window() {
        let error = open_editor_session(SessionColdStartRequest {
            data: cold_start_data(1),
            viewport_height: 720.0,
            cached_page_layout: None,
            readonly: false,
            emergency_log_entries: Vec::new(),
            emergency_payloads: Vec::new(),
        })
        .unwrap_err();

        assert_eq!(error.code, ProtocolErrorCode::ApplyFailed);
        assert_eq!(error.document_id, Some(9));
        assert!(error.message.contains("missing 1 blocks"));
    }

    #[test]
    fn cold_start_preserves_readonly_and_ignores_an_incompatible_page_cache() {
        let mut incompatible_policy = PagePolicy::default();
        incompatible_policy.target_height += 1.0;
        let result = open_editor_session(SessionColdStartRequest {
            data: cold_start_data(2),
            viewport_height: 720.0,
            cached_page_layout: Some(cached_page_layout(incompatible_policy)),
            readonly: true,
            emergency_log_entries: Vec::new(),
            emergency_payloads: Vec::new(),
        })
        .unwrap();

        let QueryResult::DocumentSummary(summary) =
            result.session.query(CommandQuery::DocumentSummary).unwrap()
        else {
            panic!("expected document summary")
        };
        assert!(summary.readonly);
        assert!(!result.report.page_layout_cache_hit);
    }

    #[test]
    fn cold_start_replays_compatible_emergency_transactions_before_exposure() {
        let result = open_editor_session(SessionColdStartRequest {
            data: cold_start_data(2),
            viewport_height: 720.0,
            cached_page_layout: None,
            readonly: false,
            emergency_log_entries: vec![emergency_entry()],
            emergency_payloads: Vec::new(),
        })
        .unwrap();

        assert_eq!(
            result.session.block_plain_text(1).unwrap().as_deref(),
            Some("1 recovered")
        );
        assert_eq!(
            result
                .emergency_recovery
                .expect("recovery report")
                .replayed_transactions,
            1
        );
        assert!(result.session.snapshot().unwrap().dirty);
    }

    #[test]
    fn readonly_cold_start_preserves_compatible_emergency_log_without_replay() {
        let result = open_editor_session(SessionColdStartRequest {
            data: cold_start_data(2),
            viewport_height: 720.0,
            cached_page_layout: None,
            readonly: true,
            emergency_log_entries: vec![emergency_entry()],
            emergency_payloads: Vec::new(),
        })
        .unwrap();

        assert_eq!(
            result.session.block_plain_text(1).unwrap().as_deref(),
            Some("1")
        );
        assert!(result.emergency_recovery.is_none());
        assert!(result.session.snapshot().unwrap().readonly);
    }

    #[test]
    fn cold_start_hydrates_deferred_emergency_payload_before_replay() {
        let (data, entry, payload) = deferred_emergency_fixture();
        let result = open_editor_session(SessionColdStartRequest {
            data,
            viewport_height: 720.0,
            cached_page_layout: None,
            readonly: false,
            emergency_log_entries: vec![entry],
            emergency_payloads: vec![payload],
        })
        .unwrap();

        assert_eq!(
            result.session.block_plain_text(3).unwrap().as_deref(),
            Some("3 recovered")
        );
    }

    #[test]
    fn cold_start_rejects_missing_deferred_emergency_payload() {
        let (data, entry, _) = deferred_emergency_fixture();
        let error = open_editor_session(SessionColdStartRequest {
            data,
            viewport_height: 720.0,
            cached_page_layout: None,
            readonly: false,
            emergency_log_entries: vec![entry],
            emergency_payloads: Vec::new(),
        })
        .unwrap_err();

        assert_eq!(error.code, ProtocolErrorCode::ApplyFailed);
        assert!(error.message.contains("missing for blocks [3]"));
    }
}
