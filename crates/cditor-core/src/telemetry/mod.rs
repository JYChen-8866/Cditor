//! 无内容 telemetry schema（P0-009，总设计 27.1）。
//!
//! 只允许结构化、无正文的指标：数值、枚举、版本号和数值 ID。schema 从类型上
//! 禁止 `String`/路径等自由文本字段，防止正文、文件名或用户输入进入遥测。
//! trace 关联 transaction id、document hash 和 task generation，但不记录原文。
//!
//! 事件按四个域拆分：input、layout、storage、sync。每个域的事件枚举都带
//! 稳定的 snake_case wire tag；新增字段/变体属于 minor 演进，删除或改名必须
//! 提升 [`TELEMETRY_SCHEMA_VERSION`] 并保留旧 reader 兼容。

pub mod input;
pub mod layout;
pub mod storage;
pub mod sync;

use serde::{Deserialize, Serialize};

/// 当前 telemetry wire schema 版本。
pub const TELEMETRY_SCHEMA_VERSION: u32 = 1;

/// trace 关联维度（总设计 27.1）：只允许数值哈希/ID，不允许原文。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TraceContext {
    pub transaction_id: Option<u64>,
    pub document_hash: Option<u64>,
    pub task_generation: Option<u64>,
}

/// 单条 telemetry 记录的 envelope。
///
/// `session_offset_ms` 是相对进程会话起点的毫秒偏移，由调用方提供，
/// 避免记录绝对时间造成跨设备可关联性。`sequence` 在会话内单调递增。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TelemetryRecord {
    pub schema_version: u32,
    pub sequence: u64,
    pub session_offset_ms: u64,
    pub trace: TraceContext,
    pub event: TelemetryEvent,
}

impl TelemetryRecord {
    pub const fn new(
        sequence: u64,
        session_offset_ms: u64,
        trace: TraceContext,
        event: TelemetryEvent,
    ) -> Self {
        Self {
            schema_version: TELEMETRY_SCHEMA_VERSION,
            sequence,
            session_offset_ms,
            trace,
            event,
        }
    }
}

/// 全部 telemetry 事件，按域打 tag。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "domain", content = "event", rename_all = "snake_case")]
pub enum TelemetryEvent {
    Input(input::InputEvent),
    Layout(layout::LayoutEvent),
    Storage(storage::StorageEvent),
    Sync(sync::SyncEvent),
}

#[cfg(test)]
pub(crate) mod test_support {
    use serde_json::Value;

    /// 断言序列化后的 telemetry JSON 无自由文本：所有 string 值只能是
    /// 枚举 tag（标识符形态），数值/布尔/null/嵌套结构不受限。
    pub fn assert_content_free(value: &Value) {
        match value {
            Value::String(text) => {
                assert!(
                    !text.is_empty()
                        && text
                            .chars()
                            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_'),
                    "telemetry string value must be an enum tag, got {text:?}"
                );
            }
            Value::Array(items) => items.iter().for_each(assert_content_free),
            Value::Object(fields) => {
                for (key, field) in fields {
                    assert!(
                        key.chars()
                            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_'),
                        "telemetry field name must be an identifier, got {key:?}"
                    );
                    assert_content_free(field);
                }
            }
            Value::Null | Value::Bool(_) | Value::Number(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::input::{GeometryQueryKind, GeometryQueryOutcome, InputEvent, InputTargetKind};
    use super::layout::{LayoutBuildKind, LayoutEvent, LayoutSurfaceKind};
    use super::storage::{SaveStatusKind, StorageBackendKind, StorageEvent};
    use super::sync::{SyncBatchOutcome, SyncEvent};
    use super::test_support::assert_content_free;
    use super::*;

    fn representative_events() -> Vec<TelemetryEvent> {
        vec![
            TelemetryEvent::Input(InputEvent::GeometryQuery {
                query: GeometryQueryKind::RangeBounds,
                target: InputTargetKind::DocumentText,
                outcome: GeometryQueryOutcome::Snapshot,
            }),
            TelemetryEvent::Layout(LayoutEvent::Build {
                surface: LayoutSurfaceKind::TableCell,
                build: LayoutBuildKind::Reflow,
                full_build_reason: None,
                duration_us: 42,
                text_bytes: 1024,
                line_count: 3,
            }),
            TelemetryEvent::Storage(StorageEvent::SaveStatusChanged {
                backend: StorageBackendKind::Sqlite,
                from: SaveStatusKind::DirtyMemory,
                to: SaveStatusKind::SavingLocal,
            }),
            TelemetryEvent::Sync(SyncEvent::PushBatch {
                operations: 12,
                bytes: 4096,
                attempt: 1,
                outcome: SyncBatchOutcome::Acked,
                round_trip_us: 180_000,
            }),
        ]
    }

    #[test]
    fn record_envelope_carries_schema_version_and_trace() {
        let record = TelemetryRecord::new(
            7,
            1500,
            TraceContext {
                transaction_id: Some(11),
                document_hash: Some(0x5eed),
                task_generation: Some(3),
            },
            representative_events()[0],
        );
        assert_eq!(record.schema_version, TELEMETRY_SCHEMA_VERSION);

        let json = serde_json::to_value(record).expect("serialize record");
        assert_eq!(json["schema_version"], 1);
        assert_eq!(json["trace"]["transaction_id"], 11);
        assert_content_free(&json);
    }

    #[test]
    fn domain_tags_are_stable_snake_case() {
        for (event, tag) in representative_events()
            .into_iter()
            .zip(["input", "layout", "storage", "sync"])
        {
            let json = serde_json::to_value(event).expect("serialize event");
            assert_eq!(json["domain"], tag, "domain tag drifted for {event:?}");
        }
    }

    #[test]
    fn all_representative_events_round_trip_and_stay_content_free() {
        for event in representative_events() {
            let json = serde_json::to_value(event).expect("serialize event");
            assert_content_free(&json);
            let back: TelemetryEvent = serde_json::from_value(json).expect("deserialize event");
            assert_eq!(back, event);
        }
    }
}
