use cditor_core::layout::PageLayoutIndex;
use cditor_editor_protocol::{ProtocolError, ProtocolErrorCode};
use cditor_runtime::document_runtime::{
    DocumentRuntimeColdStartData, DocumentRuntimeColdStartReport,
};
use cditor_storage::EmergencyLogEntry;

use crate::{
    EditorSession, EditorSessionHandle, EmergencyRecoveryDecision, EmergencyRecoveryReport,
    plan_emergency_recovery, project_emergency_recovery,
};

#[derive(Debug, Clone, PartialEq)]
pub struct SessionColdStartRequest {
    pub data: DocumentRuntimeColdStartData,
    pub viewport_height: f64,
    pub cached_page_layout: Option<PageLayoutIndex>,
    pub readonly: bool,
    pub emergency_log_entries: Vec<EmergencyLogEntry>,
}

#[derive(Debug)]
pub struct SessionColdStartResult {
    pub session: EditorSessionHandle,
    pub report: DocumentRuntimeColdStartReport,
    pub emergency_recovery: Option<EmergencyRecoveryReport>,
    pub emergency_newer_major: Option<u32>,
}

/// Opens a session from storage-neutral document data.
///
/// Storage adapters remain responsible for I/O and cache DTO decoding. This
/// boundary owns runtime construction, initial payload hydration and cached
/// page-layout fallback so a host never has to take `DocumentRuntime` back.
pub fn open_editor_session(
    request: SessionColdStartRequest,
) -> Result<SessionColdStartResult, ProtocolError> {
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
            EmergencyRecoveryDecision::Replay(plan) => {
                let report = project_emergency_recovery(&mut runtime, plan).map_err(|message| {
                    ProtocolError::new(ProtocolErrorCode::ApplyFailed, message)
                        .with_document(document_id)
                })?;
                (Some(report), None)
            }
            EmergencyRecoveryDecision::ReadOnlyNewerMajor { written_major, .. } => {
                (None, Some(written_major))
            }
        };
    let readonly = request.readonly || emergency_newer_major.is_some();

    Ok(SessionColdStartResult {
        session: EditorSession::new(runtime, readonly).into_handle(),
        report,
        emergency_recovery,
        emergency_newer_major,
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

    #[test]
    fn cold_start_hydrates_initial_payloads_and_returns_the_runtime_owner() {
        let result = open_editor_session(SessionColdStartRequest {
            data: cold_start_data(2),
            viewport_height: 720.0,
            cached_page_layout: Some(cached_page_layout(PagePolicy::default())),
            readonly: false,
            emergency_log_entries: Vec::new(),
        })
        .unwrap();

        let snapshot = result.session.snapshot().unwrap();
        assert_eq!(snapshot.document_id, 9);
        assert_eq!(snapshot.block_count, 2);
        assert_eq!(
            result.session.block_plain_text(1).unwrap().as_deref(),
            Some("1")
        );
        assert_eq!(result.report.payloads_loaded, 2);
        assert_eq!(result.report.payloads_missing, 0);
        assert!(result.report.page_layout_cache_hit);
    }

    #[test]
    fn cold_start_rejects_an_incomplete_initial_payload_window() {
        let error = open_editor_session(SessionColdStartRequest {
            data: cold_start_data(1),
            viewport_height: 720.0,
            cached_page_layout: None,
            readonly: false,
            emergency_log_entries: Vec::new(),
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
}
