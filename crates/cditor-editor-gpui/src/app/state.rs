use cditor_session::EditorSessionHandle;

use crate::persistence::EditorSaveStatus;

pub(in crate::app) struct EditorStatusUiState {
    pub(in crate::app) readonly: bool,
    pub(in crate::app) requested_readonly: bool,
    pub(in crate::app) readonly_reason: Option<EditorReadonlyReason>,
    pub(in crate::app) dirty: bool,
    pub(in crate::app) save_status: EditorSaveStatus,
}

impl EditorStatusUiState {
    pub(in crate::app) fn new(readonly: bool, requested_readonly: bool) -> Self {
        Self {
            readonly,
            requested_readonly,
            readonly_reason: None,
            dirty: false,
            save_status: super::cditor_v2_view::save_status_for_mode(readonly),
        }
    }

    pub(in crate::app) fn reset_for_session(&mut self, readonly: bool) {
        self.readonly = readonly;
        self.readonly_reason = None;
        self.dirty = false;
        self.save_status = super::cditor_v2_view::save_status_for_mode(readonly);
    }

    pub(in crate::app) fn reset_after_load_failure(&mut self) {
        self.readonly_reason = None;
        self.readonly = self.requested_readonly;
        self.dirty = false;
        self.save_status = super::cditor_v2_view::save_status_for_mode(self.readonly);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_reset_preserves_host_request_and_clears_transient_save_state() {
        let mut status = EditorStatusUiState::new(true, false);
        status.readonly_reason = Some(EditorReadonlyReason::NewerDocumentSchema {
            written_major: 3,
            supported_major: 2,
        });
        status.dirty = true;
        status.save_status = EditorSaveStatus::Failed("injected".to_owned());

        status.reset_for_session(false);

        assert!(!status.readonly);
        assert!(!status.requested_readonly);
        assert!(status.readonly_reason.is_none());
        assert!(!status.dirty);
        assert_eq!(status.save_status, EditorSaveStatus::Clean);
    }

    #[test]
    fn load_failure_restores_the_requested_readonly_mode() {
        let mut status = EditorStatusUiState::new(true, false);
        status.readonly_reason = Some(EditorReadonlyReason::NewerOperationSchema {
            written_major: 3,
            supported_major: 2,
        });

        status.reset_after_load_failure();

        assert!(!status.readonly);
        assert!(status.readonly_reason.is_none());
        assert_eq!(status.save_status, EditorSaveStatus::Clean);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorReadonlyReason {
    NewerDocumentSchema {
        written_major: u64,
        supported_major: u32,
    },
    NewerOperationSchema {
        written_major: u32,
        supported_major: u32,
    },
}

impl EditorReadonlyReason {
    pub fn message(&self) -> String {
        match self {
            Self::NewerDocumentSchema {
                written_major,
                supported_major,
            } => format!(
                "只读：文档格式 v{written_major} 高于当前支持的 v{supported_major}，请升级 Cditor 后编辑。"
            ),
            Self::NewerOperationSchema {
                written_major,
                supported_major,
            } => format!(
                "只读：恢复日志格式 v{written_major} 高于当前支持的 v{supported_major}，请升级 Cditor 后恢复。"
            ),
        }
    }
}

pub enum CditorViewState {
    Ready(EditorSessionHandle),
    Loading { message: String },
    LoadFailed { message: String },
}

impl CditorViewState {
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready(_))
    }

    pub fn is_loading(&self) -> bool {
        matches!(self, Self::Loading { .. })
    }

    pub fn is_load_failed(&self) -> bool {
        matches!(self, Self::LoadFailed { .. })
    }

    pub fn apply_loaded_session(&mut self, session: EditorSessionHandle) {
        *self = Self::Ready(session);
    }

    pub fn apply_load_failed(&mut self, message: impl Into<String>) {
        *self = Self::LoadFailed {
            message: message.into(),
        };
    }
}
