//! versioned envelope 与 unknown 数据无损保留（P1-008/P1-010，总设计 6.3）。
//!
//! envelope 的 `body` 用 [`serde_json::value::RawValue`] 保存原始字节：无法
//! 理解的数据（新 major、未知 kind、未知插件 payload）在 load/save/copy/move
//! 后字节不变。同 major 新 minor 的数据 best-effort 解码；重写时用
//! [`VersionedEnvelope::re_encode_preserving`] 把已知字段的新值合并回原始
//! body，未知顶层字段原样保留。嵌套未知字段的保留由各 kind 的 migrator
//! 负责（[`super::registry`]）。

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

use super::{ReadPolicy, SchemaDomain, SchemaVersion};

/// envelope 编解码错误。
#[derive(Debug)]
pub enum EnvelopeError {
    /// body 不是合法 JSON 或与目标类型不匹配。
    Body(serde_json::Error),
    /// 域不匹配：按 A 域解码 B 域的 envelope。
    DomainMismatch {
        expected: SchemaDomain,
        actual: SchemaDomain,
    },
}

impl std::fmt::Display for EnvelopeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Body(error) => write!(formatter, "envelope body error: {error}"),
            Self::DomainMismatch { expected, actual } => {
                write!(
                    formatter,
                    "domain mismatch: expected {expected:?}, got {actual:?}"
                )
            }
        }
    }
}

impl std::error::Error for EnvelopeError {}

/// 解码结果；所有分支都保证原始字节仍可从 envelope 取回。
#[derive(Debug)]
pub enum DecodeOutcome<T> {
    /// 同版本或旧 minor：完全理解。
    Compatible { value: T, written: SchemaVersion },
    /// 新 minor：best-effort 值；重写必须走 `re_encode_preserving`。
    ForwardCompatible { value: T, written: SchemaVersion },
    /// 新 major：不得写；调用方进入只读兼容模式（P1-011）。
    ReadOnlyNewerMajor { written: SchemaVersion },
    /// 旧 major：需 migrator 升级后重试。
    NeedsMigration { written: SchemaVersion },
}

impl<T> DecodeOutcome<T> {
    pub fn value(&self) -> Option<&T> {
        match self {
            Self::Compatible { value, .. } | Self::ForwardCompatible { value, .. } => Some(value),
            Self::ReadOnlyNewerMajor { .. } | Self::NeedsMigration { .. } => None,
        }
    }

    pub fn allows_write(&self) -> bool {
        matches!(
            self,
            Self::Compatible { .. } | Self::ForwardCompatible { .. }
        )
    }
}

/// 带域、版本与原始字节 body 的 envelope。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionedEnvelope {
    pub domain: SchemaDomain,
    pub version: SchemaVersion,
    pub body: Box<RawValue>,
}

impl PartialEq for VersionedEnvelope {
    fn eq(&self, other: &Self) -> bool {
        self.domain == other.domain
            && self.version == other.version
            && self.body.get() == other.body.get()
    }
}

impl Eq for VersionedEnvelope {}

impl VersionedEnvelope {
    /// 以域的当前版本编码 payload。
    pub fn encode<T: Serialize>(domain: SchemaDomain, payload: &T) -> Result<Self, EnvelopeError> {
        let body = serde_json::value::to_raw_value(payload).map_err(EnvelopeError::Body)?;
        Ok(Self {
            domain,
            version: domain.current_version(),
            body,
        })
    }

    /// 从既有字节构造（load 路径）；body 字节原样保留。
    pub fn from_raw_parts(
        domain: SchemaDomain,
        version: SchemaVersion,
        body: Box<RawValue>,
    ) -> Self {
        Self {
            domain,
            version,
            body,
        }
    }

    /// body 的原始字节（无损证据）。
    pub fn body_bytes(&self) -> &str {
        self.body.get()
    }

    /// 按版本策略解码为 `T`。
    pub fn decode<T: DeserializeOwned>(
        &self,
        expected_domain: SchemaDomain,
    ) -> Result<DecodeOutcome<T>, EnvelopeError> {
        if self.domain != expected_domain {
            return Err(EnvelopeError::DomainMismatch {
                expected: expected_domain,
                actual: self.domain,
            });
        }
        let current = expected_domain.current_version();
        match ReadPolicy::assess(self.version, current) {
            ReadPolicy::ReadOnlyNewerMajor => Ok(DecodeOutcome::ReadOnlyNewerMajor {
                written: self.version,
            }),
            ReadPolicy::NeedsMigration => Ok(DecodeOutcome::NeedsMigration {
                written: self.version,
            }),
            ReadPolicy::ReadWrite => {
                let value = serde_json::from_str(self.body.get()).map_err(EnvelopeError::Body)?;
                Ok(DecodeOutcome::Compatible {
                    value,
                    written: self.version,
                })
            }
            ReadPolicy::ReadWritePreservingUnknown => {
                let value = serde_json::from_str(self.body.get()).map_err(EnvelopeError::Body)?;
                Ok(DecodeOutcome::ForwardCompatible {
                    value,
                    written: self.version,
                })
            }
        }
    }

    /// 用更新后的值重编码，同时保留原 body 中本版本不认识的顶层字段。
    ///
    /// 版本号策略：保留原 envelope 的版本（新 minor 数据重写后仍标注原
    /// minor，因为未知字段仍在）；只有完全原生写入才用 `encode`。
    pub fn re_encode_preserving<T: Serialize>(&self, value: &T) -> Result<Self, EnvelopeError> {
        let mut original: serde_json::Value =
            serde_json::from_str(self.body.get()).map_err(EnvelopeError::Body)?;
        let updated = serde_json::to_value(value).map_err(EnvelopeError::Body)?;

        let merged = match (&mut original, updated) {
            (serde_json::Value::Object(original_map), serde_json::Value::Object(updated_map)) => {
                for (key, new_value) in updated_map {
                    original_map.insert(key, new_value);
                }
                serde_json::Value::Object(std::mem::take(original_map))
            }
            // 非 object body（数组/标量）没有"未知字段"概念，直接替换。
            (_, replacement) => replacement,
        };

        let body = serde_json::value::to_raw_value(&merged).map_err(EnvelopeError::Body)?;
        Ok(Self {
            domain: self.domain,
            version: self.version,
            body,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct SamplePayload {
        text: String,
        level: u32,
    }

    fn sample() -> SamplePayload {
        SamplePayload {
            text: "hello".to_owned(),
            level: 2,
        }
    }

    #[test]
    fn native_encode_decode_round_trip() {
        let envelope = VersionedEnvelope::encode(SchemaDomain::BlockPayload, &sample()).unwrap();
        assert_eq!(
            envelope.version,
            SchemaDomain::BlockPayload.current_version()
        );

        let outcome: DecodeOutcome<SamplePayload> =
            envelope.decode(SchemaDomain::BlockPayload).unwrap();
        match outcome {
            DecodeOutcome::Compatible { value, .. } => assert_eq!(value, sample()),
            other => panic!("expected compatible, got {other:?}"),
        }
    }

    #[test]
    fn domain_mismatch_is_rejected() {
        let envelope = VersionedEnvelope::encode(SchemaDomain::Clipboard, &sample()).unwrap();
        let result: Result<DecodeOutcome<SamplePayload>, _> =
            envelope.decode(SchemaDomain::BlockPayload);
        assert!(matches!(result, Err(EnvelopeError::DomainMismatch { .. })));
    }

    #[test]
    fn newer_minor_decodes_best_effort_and_preserves_unknown_fields_on_rewrite() {
        // 模拟未来 minor 写入的数据：多一个本版本不认识的字段。
        let future_body = r#"{"text":"hi","level":1,"future_field":{"nested":[1,2,3]}}"#;
        let envelope = VersionedEnvelope::from_raw_parts(
            SchemaDomain::BlockPayload,
            SchemaVersion::new(1, 99),
            RawValue::from_string(future_body.to_owned()).unwrap(),
        );

        let outcome: DecodeOutcome<SamplePayload> =
            envelope.decode(SchemaDomain::BlockPayload).unwrap();
        let value = match outcome {
            DecodeOutcome::ForwardCompatible { value, written } => {
                assert_eq!(written, SchemaVersion::new(1, 99));
                value
            }
            other => panic!("expected forward compatible, got {other:?}"),
        };

        // 修改已知字段后重写：未知字段必须保留，版本保持原 minor。
        let updated = SamplePayload {
            text: "edited".to_owned(),
            level: value.level + 1,
        };
        let rewritten = envelope.re_encode_preserving(&updated).unwrap();
        assert_eq!(rewritten.version, SchemaVersion::new(1, 99));

        let merged: serde_json::Value = serde_json::from_str(rewritten.body_bytes()).unwrap();
        assert_eq!(merged["text"], "edited");
        assert_eq!(merged["level"], 2);
        assert_eq!(
            merged["future_field"]["nested"],
            serde_json::json!([1, 2, 3])
        );
    }

    #[test]
    fn newer_major_is_read_only_and_bytes_survive_verbatim() {
        // 含非常规空白、字段顺序与转义的 body：任何规范化都会改变字节。
        let alien_body = "{\n  \"zzz\": \"\\u4f60\\u597d\",   \"aaa\": [1, 2 , 3],\"flag\":true}";
        let envelope = VersionedEnvelope::from_raw_parts(
            SchemaDomain::BlockPayload,
            SchemaVersion::new(9, 0),
            RawValue::from_string(alien_body.to_owned()).unwrap(),
        );

        let outcome: DecodeOutcome<SamplePayload> =
            envelope.decode(SchemaDomain::BlockPayload).unwrap();
        assert!(matches!(outcome, DecodeOutcome::ReadOnlyNewerMajor { .. }));
        assert!(!outcome.allows_write());

        // load -> copy/move（clone）-> save（serialize envelope）后 body 字节不变。
        let copied = envelope.clone();
        assert_eq!(copied.body_bytes(), alien_body);

        let saved = serde_json::to_string(&copied).unwrap();
        let reloaded: VersionedEnvelope = serde_json::from_str(&saved).unwrap();
        assert_eq!(reloaded.body_bytes(), alien_body);
        assert_eq!(reloaded.version, SchemaVersion::new(9, 0));
    }

    #[test]
    fn older_major_requires_migration() {
        let envelope = VersionedEnvelope::from_raw_parts(
            SchemaDomain::Operation,
            SchemaVersion::new(0, 7),
            RawValue::from_string(r#"{"op":"legacy"}"#.to_owned()).unwrap(),
        );
        // Operation 当前 1.1，写入 0.7 → 需迁移。
        let outcome: DecodeOutcome<serde_json::Value> =
            envelope.decode(SchemaDomain::Operation).unwrap();
        assert!(matches!(outcome, DecodeOutcome::NeedsMigration { .. }));
        assert_eq!(outcome.value(), None);
    }

    #[test]
    fn non_object_bodies_are_replaced_not_merged() {
        let envelope = VersionedEnvelope::from_raw_parts(
            SchemaDomain::Clipboard,
            SchemaDomain::Clipboard.current_version(),
            RawValue::from_string("[1,2,3]".to_owned()).unwrap(),
        );
        let rewritten = envelope.re_encode_preserving(&vec![4, 5]).unwrap();
        assert_eq!(rewritten.body_bytes(), "[4,5]");
    }
}
