use cditor_runtime::document_runtime::{DocumentRuntimeColdStartData, DocumentRuntimeIndexSource};
use cditor_storage::{
    MATERIALIZED_CHECKPOINT_FORMAT, MATERIALIZED_CHECKPOINT_VERSION, MaterializedRebuildPlan,
};

use cditor_editor_protocol::{ProtocolError, ProtocolErrorCode};

use crate::{
    PersistencePipeline, PreparedSessionColdStartResult, SessionColdStartRequest,
    prepare_editor_session_with_persistence,
};

pub fn prepare_editor_session_from_rebuild_plan(
    plan: MaterializedRebuildPlan,
    viewport_height: f64,
    persistence: PersistencePipeline,
) -> Result<PreparedSessionColdStartResult, ProtocolError> {
    validate_rebuild_plan(&plan)?;
    let state = plan.checkpoint.state;
    let record_count = state.records.len();
    prepare_editor_session_with_persistence(
        SessionColdStartRequest {
            data: DocumentRuntimeColdStartData {
                document_id: state.metadata.document_id,
                document_title: state.metadata.title,
                structure_version: state.metadata.structure_version,
                records: state.records,
                block_attrs: state.block_attrs,
                initial_payloads: state.payloads,
                initial_payload_window_end: record_count,
                index_source: DocumentRuntimeIndexSource::Snapshot,
                layout_cache_hits: 0,
            },
            viewport_height,
            cached_page_layout: None,
            readonly: false,
            emergency_log_entries: plan.operations,
            emergency_payloads: Vec::new(),
        },
        persistence,
    )
}

fn validate_rebuild_plan(plan: &MaterializedRebuildPlan) -> Result<(), ProtocolError> {
    let document_id = plan.checkpoint.state.metadata.document_id;
    if plan.checkpoint.format != MATERIALIZED_CHECKPOINT_FORMAT
        || plan.checkpoint.version != MATERIALIZED_CHECKPOINT_VERSION
    {
        return Err(ProtocolError::new(
            ProtocolErrorCode::ApplyFailed,
            "unsupported materialized checkpoint format or version",
        )
        .with_document(document_id));
    }
    let mut previous = plan.checkpoint.journal_sequence;
    for operation in &plan.operations {
        if operation.sequence <= previous {
            return Err(ProtocolError::new(
                ProtocolErrorCode::ApplyFailed,
                "materialized rebuild operations are not strictly ordered after the checkpoint",
            )
            .with_document(document_id));
        }
        previous = operation.sequence;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use cditor_core::document::BlockIndexRecord;
    use cditor_core::edit::{
        ChangeOrigin, EditOperation, EditTransaction, EditTransactionKind, encode_transaction,
    };
    use cditor_core::rich_text::{BlockPayloadRecord, RichBlockKind};
    use cditor_storage::{
        EmergencyLogEntry, MaterializedCheckpoint, MaterializedDocumentState,
        MaterializedRebuildPlan, StorageDocumentMetadata,
    };

    use super::*;

    fn plan(sequence: u64) -> MaterializedRebuildPlan {
        let transaction = EditTransaction::new(
            1,
            EditTransactionKind::ExplicitCommand,
            2,
            vec![EditOperation::InsertText {
                block_id: 1,
                offset: 4,
                text: " recovered".to_owned(),
            }],
            vec![],
        )
        .with_origin(ChangeOrigin::User);
        MaterializedRebuildPlan {
            checkpoint: MaterializedCheckpoint {
                format: MATERIALIZED_CHECKPOINT_FORMAT.to_owned(),
                version: MATERIALIZED_CHECKPOINT_VERSION,
                journal_sequence: 7,
                checksum: 1,
                state: MaterializedDocumentState {
                    metadata: StorageDocumentMetadata {
                        document_id: 9,
                        title: "Recovered".to_owned(),
                        structure_version: 1,
                        content_version: 1,
                        layout_version: 0,
                        schema_version: 1,
                    },
                    records: vec![BlockIndexRecord::new(1, None, 0, 0, 0)],
                    block_attrs: vec![],
                    payloads: vec![BlockPayloadRecord::rich_text(
                        1,
                        RichBlockKind::Paragraph,
                        "base",
                    )],
                },
            },
            operations: vec![EmergencyLogEntry {
                sequence,
                transaction_id: transaction.id,
                envelope: encode_transaction(&transaction).unwrap(),
            }],
        }
    }

    #[test]
    fn checkpoint_plus_ordered_operations_rebuilds_session_truth() {
        let rebuilt = prepare_editor_session_from_rebuild_plan(
            plan(8),
            720.0,
            PersistencePipeline::disabled(),
        )
        .unwrap();
        let handle = rebuilt.session.into_handle();
        assert_eq!(
            handle.block_plain_text(1).unwrap().as_deref(),
            Some("base recovered")
        );
        assert_eq!(rebuilt.emergency_recovery.unwrap().replayed_transactions, 1);
    }

    #[test]
    fn rebuild_rejects_operations_at_or_before_checkpoint() {
        let error = prepare_editor_session_from_rebuild_plan(
            plan(7),
            720.0,
            PersistencePipeline::disabled(),
        )
        .unwrap_err();
        assert_eq!(error.code, ProtocolErrorCode::ApplyFailed);
    }
}
