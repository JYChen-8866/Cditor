use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Entity, FontWeight, InteractiveElement, IntoElement, MouseButton, ParentElement,
    Styled, div, px, rgb,
};

use crate::editor_view::{CditorV2View, EditorReadonlyReason};
use crate::theme::GuiTheme;
use cditor_component::ProgressCircle;
use cditor_session::{PersistenceFailure, PersistenceFailureKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorLoadStateLabel {
    Loading {
        detail: String,
        progress: Option<u8>,
    },
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorSaveStatus {
    DirtyMemory,
    SavingLocal,
    LocallySaved,
    Syncing,
    Synced,
    FailedLocal(PersistenceFailure),
    Failed(String),
    Readonly,
}

impl EditorSaveStatus {
    pub fn label(&self) -> String {
        match self {
            Self::DirtyMemory => "有未保存更改".to_owned(),
            Self::SavingLocal => "正在保存到本机…".to_owned(),
            Self::LocallySaved => "已保存到本机".to_owned(),
            Self::Syncing => "正在同步…".to_owned(),
            Self::Synced => "已同步".to_owned(),
            Self::FailedLocal(failure) => local_failure_label(failure).to_owned(),
            Self::Failed(message) => format!("保存失败：{message}"),
            Self::Readonly => "只读".to_owned(),
        }
    }

    pub fn is_blocking_close(&self) -> bool {
        matches!(
            self,
            Self::DirtyMemory | Self::SavingLocal | Self::FailedLocal(_) | Self::Failed(_)
        )
    }
}

fn local_failure_label(failure: &PersistenceFailure) -> &'static str {
    match failure.kind {
        PersistenceFailureKind::Busy => "本地数据库正忙，内容尚未保存",
        PersistenceFailureKind::CapacityExhausted => "本机存储空间不足，内容尚未保存",
        PersistenceFailureKind::PermissionDenied => "没有本地文件写入权限，内容尚未保存",
        PersistenceFailureKind::Corruption => "本地文档数据损坏，已停止写入",
        PersistenceFailureKind::Timeout => "本地保存超时，内容尚未保存",
        PersistenceFailureKind::Io => "本地存储不可用，内容尚未保存",
        PersistenceFailureKind::Other => "本地保存失败，内容尚未保存",
    }
}

fn local_failure_guidance(failure: &PersistenceFailure) -> &'static str {
    match failure.kind {
        PersistenceFailureKind::Busy => "请稍后重试；未保存内容仍保留在当前窗口中。",
        PersistenceFailureKind::CapacityExhausted => {
            "请释放本机存储空间后重试；关闭前应导出恢复副本。"
        }
        PersistenceFailureKind::PermissionDenied => {
            "请恢复文档目录写入权限后重试；关闭前应导出恢复副本。"
        }
        PersistenceFailureKind::Corruption => {
            "为避免扩大损坏已停止写入，请保持窗口打开并导出恢复副本。"
        }
        PersistenceFailureKind::Timeout => "请检查本地磁盘状态后重试。",
        PersistenceFailureKind::Io => "请重新连接存储设备后重试。",
        PersistenceFailureKind::Other => "请重试保存；关闭前应导出恢复副本。",
    }
}

pub fn render_save_failure_notice(
    failure: &PersistenceFailure,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
) -> AnyElement {
    let retryable = failure.retryable();
    div()
        .absolute()
        .top(px(12.0))
        .right(px(12.0))
        .left(px(12.0))
        .flex()
        .justify_end()
        .child(
            div()
                .w(px(420.0))
                .max_w_full()
                .p(px(10.0))
                .rounded(px(6.0))
                .border_1()
                .border_color(rgb(theme.danger))
                .bg(rgb(theme.panel))
                .text_color(rgb(theme.text))
                .child(
                    div()
                        .text_size(px(13.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(local_failure_label(failure)),
                )
                .child(
                    div()
                        .mt(px(3.0))
                        .text_size(px(12.0))
                        .text_color(rgb(theme.muted))
                        .child(local_failure_guidance(failure)),
                )
                .when(retryable, |notice| {
                    notice.child(
                        div()
                            .id("retry-local-save")
                            .mt(px(8.0))
                            .px(px(8.0))
                            .h(px(26.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(4.0))
                            .bg(rgb(theme.action_background))
                            .text_size(px(12.0))
                            .font_weight(FontWeight::MEDIUM)
                            .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
                                view.update(cx, |view, cx| view.flush_storage_persistence(cx));
                                cx.stop_propagation();
                            })
                            .child("重试保存"),
                    )
                }),
        )
        .into_any_element()
}

pub fn render_load_state(label: &EditorLoadStateLabel, theme: GuiTheme) -> AnyElement {
    let (title, detail, color, progress) = match label {
        EditorLoadStateLabel::Loading { detail, progress } => {
            ("正在打开文档", detail.as_str(), theme.muted, *progress)
        }
        EditorLoadStateLabel::Failed(detail) => {
            ("打开文档失败", detail.as_str(), theme.danger, None)
        }
    };
    let loading = matches!(label, EditorLoadStateLabel::Loading { .. });
    div()
        .w_full()
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(rgb(theme.page))
        .child(
            div()
                .w(px(360.0))
                .flex()
                .flex_col()
                .items_center()
                .text_center()
                .when(loading, |content| {
                    content.child(progress_circle(progress, theme))
                })
                .child(
                    div()
                        .when(loading, |title| title.mt(px(14.0)))
                        .text_size(px(15.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(theme.text))
                        .child(title),
                )
                .child(
                    div()
                        .mt_2()
                        .text_size(px(13.0))
                        .text_color(rgb(color))
                        .child(detail.to_owned()),
                ),
        )
        .into_any_element()
}

fn progress_circle(progress: Option<u8>, theme: GuiTheme) -> ProgressCircle {
    let circle = ProgressCircle::new("editor-storage-loading-progress")
        .size(px(64.0))
        .color(rgb(theme.focused));
    match progress {
        Some(progress) => circle.value(f32::from(progress)).child(
            div()
                .relative()
                .text_size(px(12.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(theme.text))
                .child(format!("{progress}%")),
        ),
        None => circle.loading(true),
    }
}

pub fn render_readonly_notice(reason: &EditorReadonlyReason, theme: GuiTheme) -> AnyElement {
    div()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .h(px(32.0))
        .px(px(12.0))
        .flex()
        .items_center()
        .justify_center()
        .overflow_hidden()
        .bg(rgb(theme.action_background))
        .text_size(px(12.0))
        .text_color(rgb(theme.text))
        .child(reason.message())
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_status_labels_and_close_guard_are_stable() {
        assert_eq!(EditorSaveStatus::DirtyMemory.label(), "有未保存更改");
        assert_eq!(EditorSaveStatus::SavingLocal.label(), "正在保存到本机…");
        assert_eq!(EditorSaveStatus::LocallySaved.label(), "已保存到本机");
        assert_eq!(EditorSaveStatus::Syncing.label(), "正在同步…");
        assert_eq!(EditorSaveStatus::Synced.label(), "已同步");
        assert_eq!(
            EditorSaveStatus::FailedLocal(PersistenceFailure::new(
                PersistenceFailureKind::CapacityExhausted,
                "disk full",
            ))
            .label(),
            "本机存储空间不足，内容尚未保存"
        );
        assert_eq!(
            EditorSaveStatus::Failed("db offline".to_owned()).label(),
            "保存失败：db offline"
        );
        assert_eq!(EditorSaveStatus::Readonly.label(), "只读");

        assert!(EditorSaveStatus::DirtyMemory.is_blocking_close());
        assert!(EditorSaveStatus::SavingLocal.is_blocking_close());
        assert!(!EditorSaveStatus::LocallySaved.is_blocking_close());
        assert!(!EditorSaveStatus::Syncing.is_blocking_close());
        assert!(!EditorSaveStatus::Synced.is_blocking_close());
        assert!(
            EditorSaveStatus::FailedLocal(PersistenceFailure::new(
                PersistenceFailureKind::Busy,
                "busy",
            ))
            .is_blocking_close()
        );
        assert!(EditorSaveStatus::Failed("x".to_owned()).is_blocking_close());
        assert!(!EditorSaveStatus::Readonly.is_blocking_close());
    }

    #[test]
    fn newer_schema_readonly_notice_names_both_versions() {
        let reason = EditorReadonlyReason::NewerDocumentSchema {
            written_major: 3,
            supported_major: 1,
        };
        assert_eq!(
            reason.message(),
            "只读：文档格式 v3 高于当前支持的 v1，请升级 Cditor 后编辑。"
        );

        let recovery = EditorReadonlyReason::NewerOperationSchema {
            written_major: 4,
            supported_major: 1,
        };
        assert_eq!(
            recovery.message(),
            "只读：恢复日志格式 v4 高于当前支持的 v1，请升级 Cditor 后恢复。"
        );
    }

    #[test]
    fn local_failure_guidance_distinguishes_retry_from_corruption_recovery() {
        let busy = PersistenceFailure::new(PersistenceFailureKind::Busy, "busy");
        assert!(local_failure_guidance(&busy).contains("重试"));
        assert!(busy.retryable());

        let corruption = PersistenceFailure::new(PersistenceFailureKind::Corruption, "bad page");
        assert!(local_failure_guidance(&corruption).contains("停止写入"));
        assert!(!corruption.retryable());
    }
}
