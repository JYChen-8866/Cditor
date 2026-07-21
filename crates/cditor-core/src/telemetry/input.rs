//! input 域 telemetry：输入延迟、IME 生命周期、几何查询与 stale 回调拒绝。
//!
//! 对应总设计 10（IME）、11（selection/导航）与 28 节输入预算。字段只包含
//! 时长、计数和枚举，不包含被输入的文本。

use serde::{Deserialize, Serialize};

/// 输入目标类别，对应总设计 10.2 `InputTarget` 的四类加应用内单行控件。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputTargetKind {
    DocumentText,
    TableCell,
    ImageCaption,
    CollectionTitle,
    AppTextField,
}

/// 用户可感知的输入动作类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputActionKind {
    PrintableInsert,
    DeleteBackward,
    DeleteForward,
    NavigationKey,
    StructuralCommand,
    CompositionStart,
    CompositionPreviewUpdate,
    CompositionCommit,
    CompositionCancel,
    Paste,
    DragDrop,
}

/// 平台回调因身份不匹配被拒绝的维度，对应 `InputSessionIdentity` 各字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StaleInputRejection {
    Target,
    SessionId,
    TargetGeneration,
    CompositionGeneration,
    ContentVersion,
    LayoutIdentity,
}

/// 文本几何查询入口（Gate P2 fallback-rate 采样的四类查询）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeometryQueryKind {
    RangeBounds,
    PointToIndex,
    Navigation,
    CandidateRect,
}

/// 几何查询结果来源；正常输入下 `SyncFallbackBuild`/`Unavailable` 必须为 0。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeometryQueryOutcome {
    Snapshot,
    SyncFallbackBuild,
    Unavailable,
}

/// input 域事件。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InputEvent {
    /// 一次输入动作从平台回调到 Runtime 事务提交的耗时。
    Action {
        action: InputActionKind,
        target: InputTargetKind,
        latency_us: u64,
    },
    /// IME preview 更新到投影刷新的耗时；预算 p95 < 16ms（总设计 28）。
    ImePreviewLatency {
        target: InputTargetKind,
        latency_us: u64,
        preview_graphemes: u32,
    },
    /// 单次几何查询及其结果来源。
    GeometryQuery {
        query: GeometryQueryKind,
        target: InputTargetKind,
        outcome: GeometryQueryOutcome,
    },
    /// 过期平台回调被 session identity 拒绝。
    StaleCallbackRejected {
        target: InputTargetKind,
        reason: StaleInputRejection,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telemetry::test_support::assert_content_free;

    fn events() -> Vec<InputEvent> {
        vec![
            InputEvent::Action {
                action: InputActionKind::PrintableInsert,
                target: InputTargetKind::DocumentText,
                latency_us: 900,
            },
            InputEvent::ImePreviewLatency {
                target: InputTargetKind::TableCell,
                latency_us: 4_000,
                preview_graphemes: 5,
            },
            InputEvent::GeometryQuery {
                query: GeometryQueryKind::CandidateRect,
                target: InputTargetKind::ImageCaption,
                outcome: GeometryQueryOutcome::Unavailable,
            },
            InputEvent::StaleCallbackRejected {
                target: InputTargetKind::CollectionTitle,
                reason: StaleInputRejection::CompositionGeneration,
            },
        ]
    }

    #[test]
    fn input_events_round_trip_and_stay_content_free() {
        for event in events() {
            let json = serde_json::to_value(event).expect("serialize");
            assert_content_free(&json);
            let back: InputEvent = serde_json::from_value(json).expect("deserialize");
            assert_eq!(back, event);
        }
    }

    #[test]
    fn input_event_kind_tags_are_stable() {
        let tags: Vec<_> = events()
            .into_iter()
            .map(|event| {
                serde_json::to_value(event).expect("serialize")["kind"]
                    .as_str()
                    .expect("kind tag")
                    .to_owned()
            })
            .collect();
        assert_eq!(
            tags,
            [
                "action",
                "ime_preview_latency",
                "geometry_query",
                "stale_callback_rejected",
            ]
        );
    }
}
