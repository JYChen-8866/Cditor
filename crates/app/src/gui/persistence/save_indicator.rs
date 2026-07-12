use gpui::{AnyElement, FontWeight, IntoElement, ParentElement, Styled, div, px, rgb};

use crate::gui::GuiTheme;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorLoadStateLabel {
    Loading(String),
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorSaveStatus {
    Clean,
    Dirty,
    Saving,
    Failed(String),
    Readonly,
}

impl EditorSaveStatus {
    pub fn label(&self) -> String {
        match self {
            Self::Clean => "已保存".to_owned(),
            Self::Dirty => "有未保存更改".to_owned(),
            Self::Saving => "正在保存…".to_owned(),
            Self::Failed(message) => format!("保存失败：{message}"),
            Self::Readonly => "只读".to_owned(),
        }
    }

    pub fn is_blocking_close(&self) -> bool {
        matches!(self, Self::Dirty | Self::Saving | Self::Failed(_))
    }
}

pub fn render_load_state(label: &EditorLoadStateLabel, theme: GuiTheme) -> AnyElement {
    let (title, detail, color) = match label {
        EditorLoadStateLabel::Loading(detail) => ("正在打开文档", detail.as_str(), theme.muted),
        EditorLoadStateLabel::Failed(detail) => ("打开文档失败", detail.as_str(), theme.danger),
    };
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
                .text_center()
                .child(
                    div()
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

pub fn render_save_indicator(status: &EditorSaveStatus, theme: GuiTheme) -> AnyElement {
    let color = match status {
        EditorSaveStatus::Clean => theme.muted,
        EditorSaveStatus::Dirty => 0xcb912f,
        EditorSaveStatus::Saving => theme.focused,
        EditorSaveStatus::Failed(_) => theme.danger,
        EditorSaveStatus::Readonly => theme.muted,
    };

    div()
        .px_1()
        .py_1()
        .text_size(px(12.0))
        .text_color(rgb(color))
        .child(status.label())
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_status_labels_and_close_guard_are_stable() {
        assert_eq!(EditorSaveStatus::Clean.label(), "已保存");
        assert_eq!(EditorSaveStatus::Dirty.label(), "有未保存更改");
        assert_eq!(EditorSaveStatus::Saving.label(), "正在保存…");
        assert_eq!(
            EditorSaveStatus::Failed("db offline".to_owned()).label(),
            "保存失败：db offline"
        );
        assert_eq!(EditorSaveStatus::Readonly.label(), "只读");

        assert!(!EditorSaveStatus::Clean.is_blocking_close());
        assert!(EditorSaveStatus::Dirty.is_blocking_close());
        assert!(EditorSaveStatus::Saving.is_blocking_close());
        assert!(EditorSaveStatus::Failed("x".to_owned()).is_blocking_close());
        assert!(!EditorSaveStatus::Readonly.is_blocking_close());
    }
}
