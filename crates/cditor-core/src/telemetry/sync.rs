//! sync 域 telemetry：push/pull 批次、重试、拒绝分类与 outbox 深度。
//!
//! 对应总设计 19（同步协议）与 P8 的 outbox 状态机。字段只包含计数、字节数、
//! 时长和枚举，不包含 operation 内容。

use serde::{Deserialize, Serialize};

/// 一次 push 批次的最终结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncBatchOutcome {
    Acked,
    Rejected,
    NetworkError,
    Timeout,
}

/// 服务端拒绝分类（P8-006）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncRejectionCategory {
    Permission,
    Schema,
    Conflict,
    RateLimit,
    Size,
    Auth,
    Unknown,
}

/// sync 域事件。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SyncEvent {
    /// 一次 push 批次：规模、第几次尝试、结果与往返耗时。
    PushBatch {
        operations: u32,
        bytes: u64,
        attempt: u32,
        outcome: SyncBatchOutcome,
        round_trip_us: u64,
    },
    /// 一次 pull 批次的规模与本地应用耗时。
    PullBatch {
        operations: u32,
        bytes: u64,
        apply_duration_us: u64,
    },
    /// 服务端拒绝及其分类。
    Rejection { category: SyncRejectionCategory },
    /// 重试调度（含退避），用于观察 backoff 策略是否失控。
    RetryScheduled { attempt: u32, backoff_ms: u64 },
    /// outbox 深度周期采样；`oldest_pending_ms` 是最旧未确认批次的等待时长。
    OutboxDepth {
        pending_batches: u32,
        pending_operations: u64,
        oldest_pending_ms: u64,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telemetry::test_support::assert_content_free;

    fn events() -> Vec<SyncEvent> {
        vec![
            SyncEvent::PushBatch {
                operations: 8,
                bytes: 2_048,
                attempt: 2,
                outcome: SyncBatchOutcome::NetworkError,
                round_trip_us: 950_000,
            },
            SyncEvent::PullBatch {
                operations: 40,
                bytes: 16_384,
                apply_duration_us: 3_000,
            },
            SyncEvent::Rejection {
                category: SyncRejectionCategory::Conflict,
            },
            SyncEvent::RetryScheduled {
                attempt: 3,
                backoff_ms: 4_000,
            },
            SyncEvent::OutboxDepth {
                pending_batches: 2,
                pending_operations: 96,
                oldest_pending_ms: 30_000,
            },
        ]
    }

    #[test]
    fn sync_events_round_trip_and_stay_content_free() {
        for event in events() {
            let json = serde_json::to_value(event).expect("serialize");
            assert_content_free(&json);
            let back: SyncEvent = serde_json::from_value(json).expect("deserialize");
            assert_eq!(back, event);
        }
    }

    #[test]
    fn rejection_categories_cover_p8_taxonomy() {
        let categories = [
            SyncRejectionCategory::Permission,
            SyncRejectionCategory::Schema,
            SyncRejectionCategory::Conflict,
            SyncRejectionCategory::RateLimit,
            SyncRejectionCategory::Size,
            SyncRejectionCategory::Auth,
            SyncRejectionCategory::Unknown,
        ];
        for category in categories {
            let json = serde_json::to_value(category).expect("serialize");
            assert_content_free(&json);
        }
    }
}
