use cditor_runtime::DocumentRuntime;
use cditor_session::{
    EmergencyRecoveryPlan, project_emergency_payload_request, project_emergency_payload_result,
};
use cditor_storage::{DocumentStorage, StorageResult};

pub(crate) async fn hydrate_emergency_payloads(
    storage: &dyn DocumentStorage,
    runtime: &mut DocumentRuntime,
    plan: &EmergencyRecoveryPlan,
) -> StorageResult<()> {
    let Some(request) = project_emergency_payload_request(runtime, plan)
        .map_err(cditor_storage::StorageError::CorruptData)?
    else {
        return Ok(());
    };

    let batch = storage
        .load_payloads(runtime.document_id(), &request.block_ids)
        .await?;
    project_emergency_payload_result(
        runtime,
        cditor_runtime::content::payload_window::PayloadWindowLoadResult {
            request,
            records: batch.records,
            missing_block_ids: batch.missing_block_ids,
        },
    )
    .map_err(cditor_storage::StorageError::CorruptData)
}
