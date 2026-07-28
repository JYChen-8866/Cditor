//! storage 域 telemetry：保存状态机、本地事务耐久性、恢复与错误分类。
//!
//! 对应总设计 18（本地存储与崩溃恢复）与 P7-006 的保存状态细分。字段只包含
//! 时长、计数和枚举，不包含文档内容或文件路径。

use serde::{Deserialize, Serialize};

/// 存储后端类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageBackendKind {
    Local,
    Remote,
    Memory,
}

/// 保存状态机（P7-006）：DirtyMemory -> SavingLocal -> LocallySaved -> Syncing -> Synced。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SaveStatusKind {
    DirtyMemory,
    SavingLocal,
    LocallySaved,
    Syncing,
    Synced,
}

/// 存储错误分类（P7-007 的错误 UI 与 close guard 输入）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageErrorKind {
    DiskFull,
    Busy,
    PermissionDenied,
    Corruption,
    Io,
    Schema,
    Unknown,
}

/// 发生错误时正在执行的存储操作。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageOperationKind {
    Open,
    Read,
    WriteTransaction,
    Checkpoint,
    Migration,
    Recovery,
}

/// 启动恢复的结果（P7-009 crash marker 与 journal replay）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryOutcome {
    CleanStart,
    ReplaySucceeded,
    ReplayPartial,
    RecoveryCopyOpened,
    Failed,
}

/// storage 域事件。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StorageEvent {
    /// 保存状态机迁移。
    SaveStatusChanged {
        backend: StorageBackendKind,
        from: SaveStatusKind,
        to: SaveStatusKind,
    },
    /// 一次本地事务落盘耗时；预算 p95 < 50ms 且不占输入主线程（总设计 28）。
    TransactionDurable {
        backend: StorageBackendKind,
        duration_us: u64,
        operation_count: u32,
        payload_bytes: u64,
    },
    /// 启动时 journal replay 的结果。
    JournalReplay {
        backend: StorageBackendKind,
        outcome: RecoveryOutcome,
        replayed_operations: u64,
        duration_us: u64,
    },
    /// checkpoint 写入与 journal 截断。
    Checkpoint {
        backend: StorageBackendKind,
        duration_us: u64,
        truncated_operations: u64,
    },
    /// 分类后的存储错误。
    Error {
        backend: StorageBackendKind,
        operation: StorageOperationKind,
        error: StorageErrorKind,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telemetry::test_support::assert_content_free;

    fn events() -> Vec<StorageEvent> {
        vec![
            StorageEvent::SaveStatusChanged {
                backend: StorageBackendKind::Local,
                from: SaveStatusKind::SavingLocal,
                to: SaveStatusKind::LocallySaved,
            },
            StorageEvent::TransactionDurable {
                backend: StorageBackendKind::Local,
                duration_us: 12_000,
                operation_count: 3,
                payload_bytes: 640,
            },
            StorageEvent::JournalReplay {
                backend: StorageBackendKind::Local,
                outcome: RecoveryOutcome::ReplaySucceeded,
                replayed_operations: 17,
                duration_us: 90_000,
            },
            StorageEvent::Checkpoint {
                backend: StorageBackendKind::Remote,
                duration_us: 40_000,
                truncated_operations: 210,
            },
            StorageEvent::Error {
                backend: StorageBackendKind::Local,
                operation: StorageOperationKind::WriteTransaction,
                error: StorageErrorKind::DiskFull,
            },
        ]
    }

    #[test]
    fn storage_events_round_trip_and_stay_content_free() {
        for event in events() {
            let json = serde_json::to_value(event).expect("serialize");
            assert_content_free(&json);
            let back: StorageEvent = serde_json::from_value(json).expect("deserialize");
            assert_eq!(back, event);
        }
    }

    #[test]
    fn save_status_tags_match_p7_state_machine() {
        let statuses = [
            SaveStatusKind::DirtyMemory,
            SaveStatusKind::SavingLocal,
            SaveStatusKind::LocallySaved,
            SaveStatusKind::Syncing,
            SaveStatusKind::Synced,
        ];
        let tags: Vec<_> = statuses
            .into_iter()
            .map(|status| serde_json::to_value(status).expect("serialize"))
            .collect();
        assert_eq!(
            tags,
            [
                "dirty_memory",
                "saving_local",
                "locally_saved",
                "syncing",
                "synced",
            ]
        );
    }
}
