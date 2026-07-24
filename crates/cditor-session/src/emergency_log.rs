use std::collections::BTreeSet;

use cditor_core::edit::{EditTransaction, TransactionDecodeOutcome, decode_transaction};
use cditor_core::ids::BlockId;
use cditor_runtime::DocumentRuntime;
use cditor_runtime::content::payload_window::{
    PayloadWindowApplyDecision, PayloadWindowLoadRequest, PayloadWindowLoadResult,
};
use cditor_storage::EmergencyLogEntry;

pub const MAX_EMERGENCY_LOG_ENTRIES: usize = 4_096;
pub const MAX_EMERGENCY_LOG_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_EMERGENCY_AFFECTED_BLOCKS: usize = 16_384;

#[derive(Debug, Clone)]
pub struct EmergencyRecoveryPlan {
    pub transactions: Vec<EditTransaction>,
    pub affected_block_ids: Vec<BlockId>,
    pub through_sequence: Option<u64>,
}

#[derive(Debug, Clone)]
pub enum EmergencyRecoveryDecision {
    Replay(EmergencyRecoveryPlan),
    ReadOnlyNewerMajor {
        written_major: u32,
        through_sequence: u64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmergencyRecoveryReport {
    pub replayed_transactions: usize,
    pub affected_blocks: usize,
    pub through_sequence: Option<u64>,
}

pub fn plan_emergency_recovery(
    entries: Vec<EmergencyLogEntry>,
) -> Result<EmergencyRecoveryDecision, String> {
    if entries.len() > MAX_EMERGENCY_LOG_ENTRIES {
        return Err(format!(
            "emergency log has {} entries, limit is {MAX_EMERGENCY_LOG_ENTRIES}",
            entries.len()
        ));
    }

    let mut bytes = 0usize;
    let mut previous_sequence = None;
    let mut through_sequence = None;
    let mut transactions = Vec::with_capacity(entries.len());
    let mut affected = BTreeSet::new();
    for entry in entries {
        if previous_sequence.is_some_and(|previous| entry.sequence <= previous) {
            return Err("emergency log sequences must be strictly increasing".to_owned());
        }
        previous_sequence = Some(entry.sequence);
        through_sequence = Some(entry.sequence);
        bytes = bytes
            .checked_add(entry.envelope.body.get().len())
            .ok_or_else(|| "emergency log byte count overflowed".to_owned())?;
        if bytes > MAX_EMERGENCY_LOG_BYTES {
            return Err(format!(
                "emergency log has {bytes} bytes, limit is {MAX_EMERGENCY_LOG_BYTES}"
            ));
        }

        match decode_transaction(&entry.envelope).map_err(|error| error.to_string())? {
            TransactionDecodeOutcome::Compatible(transaction) => {
                if transaction.id != entry.transaction_id {
                    return Err(format!(
                        "emergency log transaction identity mismatch: row {} contains {}",
                        entry.transaction_id, transaction.id
                    ));
                }
                affected.extend(transaction.affected_blocks.iter().copied());
                if affected.len() > MAX_EMERGENCY_AFFECTED_BLOCKS {
                    return Err(format!(
                        "emergency log affects more than {MAX_EMERGENCY_AFFECTED_BLOCKS} blocks"
                    ));
                }
                transactions.push(*transaction);
            }
            TransactionDecodeOutcome::ReadOnlyNewerMajor { written } => {
                return Ok(EmergencyRecoveryDecision::ReadOnlyNewerMajor {
                    written_major: written.major,
                    through_sequence: entry.sequence,
                });
            }
            TransactionDecodeOutcome::NeedsMigration { written } => {
                return Err(format!(
                    "emergency transaction schema {written} requires migration"
                ));
            }
        }
    }

    Ok(EmergencyRecoveryDecision::Replay(EmergencyRecoveryPlan {
        transactions,
        affected_block_ids: affected.into_iter().collect(),
        through_sequence,
    }))
}

pub fn project_emergency_recovery(
    runtime: &mut DocumentRuntime,
    plan: EmergencyRecoveryPlan,
) -> Result<EmergencyRecoveryReport, String> {
    let replayed_transactions = plan.transactions.len();
    let affected_blocks = plan.affected_block_ids.len();
    for transaction in &plan.transactions {
        runtime
            .apply_external_transaction(transaction, transaction.origin)
            .map_err(|error| {
                format!(
                    "cannot replay emergency transaction {}: {error}",
                    transaction.id
                )
            })?;
    }
    Ok(EmergencyRecoveryReport {
        replayed_transactions,
        affected_blocks,
        through_sequence: plan.through_sequence,
    })
}

pub fn project_emergency_payload_request(
    runtime: &mut DocumentRuntime,
    plan: &EmergencyRecoveryPlan,
) -> Result<Option<PayloadWindowLoadRequest>, String> {
    runtime.plan_emergency_payload_load(&plan.affected_block_ids)
}

pub fn project_emergency_payload_result(
    runtime: &mut DocumentRuntime,
    result: PayloadWindowLoadResult,
) -> Result<(), String> {
    if !result.missing_block_ids.is_empty() {
        return Err(format!(
            "emergency recovery payloads are missing for blocks {:?}",
            result.missing_block_ids
        ));
    }
    match runtime.apply_payload_window_result(result) {
        PayloadWindowApplyDecision::Applied => Ok(()),
        PayloadWindowApplyDecision::DiscardedStaleGeneration { expected, actual } => Err(format!(
            "emergency payload hydration became stale: expected generation {expected}, got {actual}"
        )),
    }
}

#[cfg(test)]
mod tests {
    use cditor_core::edit::{ChangeOrigin, EditOperation, EditTransactionKind, encode_transaction};
    use cditor_storage::EmergencyLogEntry;

    use super::*;

    fn transaction(id: u64, text: &str) -> EditTransaction {
        EditTransaction::new(
            id,
            EditTransactionKind::Typing,
            id,
            vec![EditOperation::InsertText {
                block_id: 1,
                offset: usize::try_from(id - 1).unwrap(),
                text: text.to_owned(),
            }],
            vec![EditOperation::DeleteText {
                block_id: 1,
                range: 0..text.len(),
            }],
        )
        .with_origin(ChangeOrigin::User)
    }

    fn entry(sequence: u64, transaction: &EditTransaction) -> EmergencyLogEntry {
        EmergencyLogEntry {
            sequence,
            transaction_id: transaction.id,
            envelope: encode_transaction(transaction).unwrap(),
        }
    }

    #[test]
    fn plan_validates_order_identity_and_collects_a_bounded_payload_window() {
        let first = transaction(1, "a");
        let second = transaction(2, "b");
        let EmergencyRecoveryDecision::Replay(plan) =
            plan_emergency_recovery(vec![entry(7, &first), entry(9, &second)]).unwrap()
        else {
            panic!("expected replay")
        };
        assert_eq!(plan.affected_block_ids, vec![1]);
        assert_eq!(plan.through_sequence, Some(9));

        let mut mismatched = entry(10, &first);
        mismatched.transaction_id = 99;
        assert!(plan_emergency_recovery(vec![mismatched]).is_err());
        assert!(plan_emergency_recovery(vec![entry(2, &first), entry(1, &second)]).is_err());
    }

    #[test]
    fn newer_operation_schema_stays_preserved_and_forces_readonly_recovery() {
        let transaction = transaction(1, "a");
        let mut entry = entry(7, &transaction);
        entry.envelope.version.major = entry.envelope.version.major.saturating_add(1);

        let EmergencyRecoveryDecision::ReadOnlyNewerMajor {
            written_major,
            through_sequence,
        } = plan_emergency_recovery(vec![entry]).unwrap()
        else {
            panic!("expected readonly recovery")
        };
        assert_eq!(
            written_major,
            cditor_core::schema::SchemaDomain::Operation
                .current_version()
                .major
                .saturating_add(1)
        );
        assert_eq!(through_sequence, 7);
    }

    #[test]
    fn replay_restores_text_and_pending_persistence() {
        let first = transaction(1, "a");
        let second = transaction(2, "b");
        let EmergencyRecoveryDecision::Replay(plan) =
            plan_emergency_recovery(vec![entry(1, &first), entry(2, &second)]).unwrap()
        else {
            panic!("expected replay")
        };
        let mut runtime = DocumentRuntime::empty();

        let report = project_emergency_recovery(&mut runtime, plan).unwrap();
        assert_eq!(report.replayed_transactions, 2);
        assert_eq!(runtime.block_payload_record(1).unwrap().plain_text(), "ab");
        assert_eq!(runtime.pending_structure_transaction_count(), 2);
    }
}
