use cditor_session::EditorSessionHandle;
use gpui::{Context, FocusHandle};

use crate::persistence::EditorSaveStatus;
use crate::text::TextPlatformLayoutIdentity;

use super::cditor_v2_view::{CditorV2View, GuiPlatformInputTarget};

pub(in crate::app) struct FocusUiState {
    pub(in crate::app) editor: FocusHandle,
    pub(in crate::app) code_language: FocusHandle,
    pub(in crate::app) ai_prompt: FocusHandle,
    pub(in crate::app) sdk_observers_registered: bool,
    pub(in crate::app) last_emitted_selection: Option<cditor_api::document::DocumentSelection>,
}

impl FocusUiState {
    pub(in crate::app) fn new(cx: &mut Context<CditorV2View>) -> Self {
        Self {
            editor: cx.focus_handle(),
            code_language: cx.focus_handle(),
            ai_prompt: cx.focus_handle(),
            sdk_observers_registered: false,
            last_emitted_selection: None,
        }
    }

    pub(in crate::app) fn reset_session_projection(&mut self) {
        self.last_emitted_selection = None;
    }
}

#[derive(Default)]
pub(in crate::app) struct PlatformInputState {
    pub(in crate::app) target: Option<GuiPlatformInputTarget>,
    pub(in crate::app) session_identity: Option<cditor_runtime::InputSessionIdentity>,
    pub(in crate::app) layout_identity: Option<TextPlatformLayoutIdentity>,
    pub(in crate::app) preferred_navigation_x: Option<(cditor_core::ids::SurfaceId, f32)>,
}

impl PlatformInputState {
    pub(in crate::app) fn reset(&mut self) {
        *self = Self::default();
    }
}

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

    #[test]
    fn platform_input_reset_discards_session_bound_navigation_state() {
        let mut input = PlatformInputState {
            preferred_navigation_x: Some((cditor_core::ids::SurfaceId::Block(7), 42.0)),
            ..Default::default()
        };

        input.reset();

        assert!(input.target.is_none());
        assert!(input.session_identity.is_none());
        assert!(input.layout_identity.is_none());
        assert!(input.preferred_navigation_x.is_none());
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
