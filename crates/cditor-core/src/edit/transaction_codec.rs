//! EditTransaction 的 versioned 序列化（P4-014，总设计 6.3/12.2）。
//!
//! transaction 以 [`SchemaDomain::Operation`] envelope 持久化（journal/
//! outbox/协作载荷的统一编码）。解码规则：
//!
//! - 同 major：任何未知 operation variant 都使**整个 transaction 被拒绝**
//!   （[`TransactionDecodeError::UnknownOperation`]），绝不静默跳过单个 op——
//!   部分应用会破坏原子性与 inverse 对称性；
//! - 新 major：只读拒写（[`TransactionDecodeOutcome::ReadOnlyNewerMajor`]）；
//! - 旧 major：显式要求迁移（[`TransactionDecodeOutcome::NeedsMigration`]）。

use serde_json::value::RawValue;

use crate::schema::{DecodeOutcome, EnvelopeError, SchemaDomain, SchemaVersion, VersionedEnvelope};

use super::transactions::EditTransaction;

/// transaction 解码错误。
#[derive(Debug)]
pub enum TransactionDecodeError {
    /// envelope 层错误（域不匹配、body 非法 JSON）。
    Envelope(EnvelopeError),
    /// body 含本版本不认识的 operation/字段：整个 transaction 拒绝。
    UnknownOperation {
        written: SchemaVersion,
        detail: String,
    },
}

impl std::fmt::Display for TransactionDecodeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Envelope(error) => write!(formatter, "transaction envelope error: {error}"),
            Self::UnknownOperation { written, detail } => write!(
                formatter,
                "transaction from schema {written} contains unknown operation, rejecting whole transaction: {detail}"
            ),
        }
    }
}

impl std::error::Error for TransactionDecodeError {}

impl From<EnvelopeError> for TransactionDecodeError {
    fn from(error: EnvelopeError) -> Self {
        Self::Envelope(error)
    }
}

/// transaction 解码结果。
#[derive(Debug)]
pub enum TransactionDecodeOutcome {
    /// 完全理解，可应用亦可重写。
    Compatible(Box<EditTransaction>),
    /// 新 major：禁止应用与重写，进入只读兼容模式。
    ReadOnlyNewerMajor { written: SchemaVersion },
    /// 旧 major：需 migrator 升级。
    NeedsMigration { written: SchemaVersion },
}

impl TransactionDecodeOutcome {
    pub fn transaction(&self) -> Option<&EditTransaction> {
        match self {
            Self::Compatible(transaction) => Some(transaction.as_ref()),
            _ => None,
        }
    }
}

/// 以 Operation 域当前版本编码 transaction。
pub fn encode_transaction(
    transaction: &EditTransaction,
) -> Result<VersionedEnvelope, EnvelopeError> {
    VersionedEnvelope::encode(SchemaDomain::Operation, transaction)
}

/// 从 envelope 解码 transaction。
///
/// 同 major 的解码采用严格模式：serde 未知 variant/多余数据都会失败并归类
/// 为 [`TransactionDecodeError::UnknownOperation`]。新 minor 与同版本同样
/// 严格——operation 语义是原子应用的输入，不允许 best-effort 部分理解。
pub fn decode_transaction(
    envelope: &VersionedEnvelope,
) -> Result<TransactionDecodeOutcome, TransactionDecodeError> {
    let outcome: DecodeOutcome<EditTransaction> = match envelope.decode(SchemaDomain::Operation) {
        Ok(outcome) => outcome,
        Err(EnvelopeError::Body(error)) => {
            return Err(TransactionDecodeError::UnknownOperation {
                written: envelope.version,
                detail: error.to_string(),
            });
        }
        Err(error) => return Err(error.into()),
    };
    Ok(match outcome {
        DecodeOutcome::Compatible { value, .. }
        | DecodeOutcome::ForwardCompatible { value, .. } => {
            TransactionDecodeOutcome::Compatible(Box::new(value))
        }
        DecodeOutcome::ReadOnlyNewerMajor { written } => {
            TransactionDecodeOutcome::ReadOnlyNewerMajor { written }
        }
        DecodeOutcome::NeedsMigration { written } => {
            TransactionDecodeOutcome::NeedsMigration { written }
        }
    })
}

/// 由原始字节重建 envelope（journal 读回路径）。
pub fn transaction_envelope_from_raw(
    version: SchemaVersion,
    body: Box<RawValue>,
) -> VersionedEnvelope {
    VersionedEnvelope::from_raw_parts(SchemaDomain::Operation, version, body)
}

#[cfg(test)]
mod tests {
    use super::super::transactions::{EditOperation, EditTransactionKind, TableEditOperation};
    use super::*;
    use crate::document::BlockIndexRecord;
    use crate::rich_text::{InlineSpan, TableRowPayload};

    fn representative_transaction() -> EditTransaction {
        EditTransaction::new(
            42,
            EditTransactionKind::ExplicitCommand,
            1_700_000_000_000,
            vec![
                EditOperation::InsertText {
                    block_id: 7,
                    offset: 3,
                    text: "héllo 世界".to_owned(),
                },
                EditOperation::DeleteText {
                    block_id: 7,
                    range: 0..2,
                },
                EditOperation::SplitBlock {
                    block_id: 7,
                    offset: 5,
                    new_block_id: 8,
                },
                EditOperation::InsertBlock {
                    index: 1,
                    block: BlockIndexRecord::new(9, Some(7), 1, 1, 0),
                },
                EditOperation::MoveBlockToParent {
                    block_id: 9,
                    parent_id: None,
                    sibling_index: 0,
                },
                EditOperation::Table(TableEditOperation::SetCellText {
                    block_id: 11,
                    row: 1,
                    col: 2,
                    old_spans: vec![InlineSpan::plain("old")],
                    new_spans: vec![InlineSpan::plain("new")],
                }),
                EditOperation::Table(TableEditOperation::InsertRows {
                    block_id: 11,
                    index: 0,
                    rows: vec![TableRowPayload::default()],
                }),
            ],
            vec![EditOperation::DeleteText {
                block_id: 7,
                range: 3..14,
            }],
        )
    }

    #[test]
    fn transaction_round_trips_through_operation_envelope() {
        let transaction = representative_transaction();
        let envelope = encode_transaction(&transaction).expect("encode");
        assert_eq!(envelope.domain, SchemaDomain::Operation);
        assert_eq!(envelope.version, SchemaDomain::Operation.current_version());

        // 经字节序列化（journal 写读）后仍然一致。
        let persisted = serde_json::to_string(&envelope).expect("persist");
        let reloaded: VersionedEnvelope = serde_json::from_str(&persisted).expect("reload");
        match decode_transaction(&reloaded).expect("decode") {
            TransactionDecodeOutcome::Compatible(back) => assert_eq!(*back, transaction),
            other => panic!("expected compatible, got {other:?}"),
        }
    }

    #[test]
    fn unknown_operation_rejects_whole_transaction() {
        // 同 major 的"未来 op"：body 中混入未知 variant。
        let body = r#"{
            "id": 1,
            "ops": [
                {"InsertText": {"block_id": 1, "offset": 0, "text": "ok"}},
                {"TeleportBlock": {"block_id": 1, "to_dimension": 4}}
            ],
            "inverse_ops": [],
            "affected_blocks": [1],
            "before_selection": null,
            "after_selection": null,
            "before_anchor": null,
            "after_anchor": null,
            "timestamp": 0,
            "kind": "ExplicitCommand"
        }"#;
        let envelope = transaction_envelope_from_raw(
            SchemaDomain::Operation.current_version(),
            RawValue::from_string(body.to_owned()).unwrap(),
        );

        let error = decode_transaction(&envelope).expect_err("must reject");
        match error {
            TransactionDecodeError::UnknownOperation { detail, .. } => {
                assert!(detail.contains("TeleportBlock"), "detail: {detail}");
            }
            other => panic!("expected unknown-operation rejection, got {other:?}"),
        }
    }

    #[test]
    fn newer_major_is_read_only_and_bytes_survive() {
        let alien = r#"{"future": true}"#;
        let envelope = transaction_envelope_from_raw(
            SchemaVersion::new(99, 0),
            RawValue::from_string(alien.to_owned()).unwrap(),
        );
        match decode_transaction(&envelope).expect("decode") {
            TransactionDecodeOutcome::ReadOnlyNewerMajor { written } => {
                assert_eq!(written, SchemaVersion::new(99, 0));
            }
            other => panic!("expected read-only, got {other:?}"),
        }
        assert_eq!(envelope.body_bytes(), alien);
    }

    #[test]
    fn older_major_requires_migration() {
        let envelope = transaction_envelope_from_raw(
            SchemaVersion::new(0, 9),
            RawValue::from_string(r#"{"legacy": 1}"#.to_owned()).unwrap(),
        );
        assert!(matches!(
            decode_transaction(&envelope).expect("decode"),
            TransactionDecodeOutcome::NeedsMigration { .. }
        ));
    }

    #[test]
    fn wrong_domain_is_rejected() {
        let clipboard =
            VersionedEnvelope::encode(SchemaDomain::Clipboard, &representative_transaction())
                .expect("encode");
        assert!(matches!(
            decode_transaction(&clipboard),
            Err(TransactionDecodeError::Envelope(
                EnvelopeError::DomainMismatch { .. }
            ))
        ));
    }

    #[test]
    fn corrupted_body_is_rejected_not_partially_applied() {
        let envelope = transaction_envelope_from_raw(
            SchemaDomain::Operation.current_version(),
            RawValue::from_string(r#"{"id": 1, "ops": "not-an-array"}"#.to_owned()).unwrap(),
        );
        assert!(matches!(
            decode_transaction(&envelope),
            Err(TransactionDecodeError::UnknownOperation { .. })
        ));
    }
}
