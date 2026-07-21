//! layout 域 telemetry：布局构建、缓存、stale 结果拒绝与帧预算。
//!
//! 对应总设计 9.5（cache 与调度）、17（虚拟化）与 28 节帧预算。字段只包含
//! 时长、字节数、行数和枚举，不包含被排版的文本。

use serde::{Deserialize, Serialize};

use crate::version::SnapshotIdentityMismatch;

/// 布局 surface 类别，对齐 `SurfaceId` 的五个变体，但不携带具体 ID。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayoutSurfaceKind {
    Block,
    TableCell,
    ImageCaption,
    CollectionTitle,
    Ephemeral,
}

/// 一次布局请求的执行路径。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayoutBuildKind {
    CacheHit,
    Reflow,
    FullBuild,
}

/// full build 的分类原因（对应 P2-012 的不可增量 fallback reason）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FullBuildReason {
    ContentChanged,
    StyleChanged,
    InlineObjectChanged,
    FontChanged,
    ScaleChanged,
    CacheMiss,
    CacheEvicted,
}

/// 布局缓存压力档位（对应 cache 的 Warning/Critical 淘汰策略）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CachePressureLevel {
    Normal,
    Warning,
    Critical,
}

/// 异步结果被拒绝时过期的身份维度，与 `SnapshotIdentityMismatch` 一一对应。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentityDimension {
    Document,
    Structure,
    Surface,
    Content,
    Layout,
    Font,
    Scale,
    Viewport,
    Generation,
}

impl From<SnapshotIdentityMismatch> for IdentityDimension {
    fn from(mismatch: SnapshotIdentityMismatch) -> Self {
        match mismatch {
            SnapshotIdentityMismatch::Document { .. } => Self::Document,
            SnapshotIdentityMismatch::Structure { .. } => Self::Structure,
            SnapshotIdentityMismatch::Surface { .. } => Self::Surface,
            SnapshotIdentityMismatch::Content { .. } => Self::Content,
            SnapshotIdentityMismatch::Layout { .. } => Self::Layout,
            SnapshotIdentityMismatch::Font { .. } => Self::Font,
            SnapshotIdentityMismatch::Scale { .. } => Self::Scale,
            SnapshotIdentityMismatch::Viewport { .. } => Self::Viewport,
            SnapshotIdentityMismatch::Generation { .. } => Self::Generation,
        }
    }
}

/// layout 域事件。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LayoutEvent {
    /// 一次布局构建及其路径；`full_build_reason` 仅在 `FullBuild` 时存在。
    Build {
        surface: LayoutSurfaceKind,
        build: LayoutBuildKind,
        full_build_reason: Option<FullBuildReason>,
        duration_us: u64,
        text_bytes: u64,
        line_count: u32,
    },
    /// 缓存周期性快照（entries/bytes 双预算与命中率）。
    CacheSnapshot {
        entries: u32,
        bytes: u64,
        hits: u64,
        misses: u64,
        evictions: u64,
        pressure: CachePressureLevel,
    },
    /// 异步布局结果因身份过期被拒绝。
    StaleResultRejected { dimension: IdentityDimension },
    /// 单帧耗时相对预算的采样（长帧诊断入口）。
    FrameBudget {
        frame_us: u64,
        budget_us: u64,
        over_budget: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telemetry::test_support::assert_content_free;

    fn events() -> Vec<LayoutEvent> {
        vec![
            LayoutEvent::Build {
                surface: LayoutSurfaceKind::Block,
                build: LayoutBuildKind::FullBuild,
                full_build_reason: Some(FullBuildReason::ContentChanged),
                duration_us: 350,
                text_bytes: 2_048,
                line_count: 12,
            },
            LayoutEvent::CacheSnapshot {
                entries: 128,
                bytes: 4 << 20,
                hits: 900,
                misses: 30,
                evictions: 4,
                pressure: CachePressureLevel::Warning,
            },
            LayoutEvent::StaleResultRejected {
                dimension: IdentityDimension::Content,
            },
            LayoutEvent::FrameBudget {
                frame_us: 21_000,
                budget_us: 16_700,
                over_budget: true,
            },
        ]
    }

    #[test]
    fn layout_events_round_trip_and_stay_content_free() {
        for event in events() {
            let json = serde_json::to_value(event).expect("serialize");
            assert_content_free(&json);
            let back: LayoutEvent = serde_json::from_value(json).expect("deserialize");
            assert_eq!(back, event);
        }
    }

    #[test]
    fn identity_dimension_covers_every_snapshot_mismatch() {
        let mismatches = [
            SnapshotIdentityMismatch::Document {
                expected: 1,
                actual: 2,
            },
            SnapshotIdentityMismatch::Structure {
                expected: 1,
                actual: 2,
            },
            SnapshotIdentityMismatch::Surface {
                expected: None,
                actual: None,
            },
            SnapshotIdentityMismatch::Content {
                expected: None,
                actual: None,
            },
            SnapshotIdentityMismatch::Layout {
                expected: 1,
                actual: 2,
            },
            SnapshotIdentityMismatch::Font {
                expected: 1,
                actual: 2,
            },
            SnapshotIdentityMismatch::Scale {
                expected: 1,
                actual: 2,
            },
            SnapshotIdentityMismatch::Viewport {
                expected: 1,
                actual: 2,
            },
            SnapshotIdentityMismatch::Generation {
                expected: 1,
                actual: 2,
            },
        ];
        let dimensions: Vec<IdentityDimension> = mismatches
            .into_iter()
            .map(IdentityDimension::from)
            .collect();
        assert_eq!(
            dimensions,
            [
                IdentityDimension::Document,
                IdentityDimension::Structure,
                IdentityDimension::Surface,
                IdentityDimension::Content,
                IdentityDimension::Layout,
                IdentityDimension::Font,
                IdentityDimension::Scale,
                IdentityDimension::Viewport,
                IdentityDimension::Generation,
            ]
        );
    }
}
