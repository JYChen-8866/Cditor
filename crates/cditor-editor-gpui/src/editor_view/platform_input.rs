use cditor_core::ids::{BlockId, SurfaceId};
use cditor_runtime::InputTarget;

use crate::editor_view::{CditorV2View, PlatformCharacterCoordinatesIdentity};
use crate::input::ime::support::InputContextSource;
use crate::input::trace::trace_input;
use crate::text::TextPlatformLayoutIdentity;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GuiPlatformInputTarget {
    BlockText {
        block_id: BlockId,
    },
    TableCell {
        block_id: BlockId,
        row: usize,
        col: usize,
    },
    ImageCaption {
        block_id: BlockId,
    },
    CollectionTitle {
        block_id: BlockId,
    },
    CodeLanguage {
        block_id: BlockId,
    },
    AiPrompt {
        block_id: BlockId,
    },
    TableMenuQuery {
        block_id: BlockId,
    },
    /// Link popup label field.
    LinkText {
        block_id: BlockId,
    },
    /// Link popup destination field.
    LinkUrl {
        block_id: BlockId,
    },
    /// Complex block or block chrome focus (no platform text input)
    None,
}

impl GuiPlatformInputTarget {
    pub(crate) fn from_runtime_target(target: InputTarget) -> Self {
        match target {
            InputTarget::BlockText { block_id } => Self::BlockText { block_id },
            InputTarget::TableCell { block_id, row, col } => Self::TableCell { block_id, row, col },
            InputTarget::ImageCaption { block_id } => Self::ImageCaption { block_id },
            InputTarget::CollectionTitle { block_id } => Self::CollectionTitle { block_id },
            // Complex blocks and block chrome don't need platform text input
            InputTarget::ComplexBlock { .. } | InputTarget::BlockChrome { .. } => Self::None,
        }
    }

    pub(crate) fn code_language(block_id: BlockId) -> Self {
        Self::CodeLanguage { block_id }
    }

    pub(crate) fn ai_prompt(block_id: BlockId) -> Self {
        Self::AiPrompt { block_id }
    }

    pub(crate) fn table_menu_query(block_id: BlockId) -> Self {
        Self::TableMenuQuery { block_id }
    }

    pub(crate) fn link_text(block_id: BlockId) -> Self {
        Self::LinkText { block_id }
    }

    pub(crate) fn link_url(block_id: BlockId) -> Self {
        Self::LinkUrl { block_id }
    }

    pub(crate) fn block_id(self) -> BlockId {
        match self {
            Self::BlockText { block_id }
            | Self::TableCell { block_id, .. }
            | Self::ImageCaption { block_id }
            | Self::CollectionTitle { block_id }
            | Self::CodeLanguage { block_id }
            | Self::AiPrompt { block_id }
            | Self::TableMenuQuery { block_id }
            | Self::LinkText { block_id }
            | Self::LinkUrl { block_id } => block_id,
            Self::None => BlockId::default(),
        }
    }

    pub(crate) fn is_code_language_for(self, block_id: BlockId) -> bool {
        self == Self::CodeLanguage { block_id }
    }

    pub(crate) fn is_ai_prompt_for(self, block_id: BlockId) -> bool {
        self == Self::AiPrompt { block_id }
    }

    pub(crate) fn is_table_menu_query_for(self, block_id: BlockId) -> bool {
        self == Self::TableMenuQuery { block_id }
    }

    pub(crate) fn is_link_edit_for(self, block_id: BlockId) -> bool {
        self == Self::LinkText { block_id } || self == Self::LinkUrl { block_id }
    }

    pub(crate) fn matches_runtime_target(self, target: InputTarget) -> bool {
        self == Self::from_runtime_target(target)
    }

    pub(crate) fn from_surface_id(surface_id: SurfaceId) -> Option<Self> {
        match surface_id {
            SurfaceId::Block(block_id) => Some(Self::BlockText { block_id }),
            SurfaceId::TableCell {
                block_id,
                row,
                column,
            } => Some(Self::TableCell {
                block_id,
                row,
                col: column,
            }),
            SurfaceId::ImageCaption { block_id } => Some(Self::ImageCaption { block_id }),
            SurfaceId::CollectionTitle { block_id } => Some(Self::CollectionTitle { block_id }),
            SurfaceId::Ephemeral { .. } => None,
        }
    }
}

impl CditorV2View {
    pub(crate) fn commit_document_composition_before_external_focus(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let expected = self.input.session_identity;
        let has_pending_composition = self
            .ready_session()
            .and_then(|session| session.input_context().ok())
            .is_some_and(|context| context.has_pending_composition);
        if !has_pending_composition {
            return true;
        }
        let result = self.ready_session().map(|session| {
            let expected = expected
                .ok_or_else(|| "active composition has no registered input identity".to_owned())?;
            session
                .apply_realtime_input(cditor_runtime::RealtimeInputRequest {
                    expected,
                    input: cditor_runtime::RealtimeInput::CommitBeforeExternalFocus,
                })
                .map(Some)
                .map_err(|error| error.to_string())
        });
        match result {
            Some(Ok(Some(outcome))) if outcome.document_changed => {
                trace_input("external_focus.composition_committed", "changed=true");
                self.input.session_identity = outcome.input_identity;
                self.mark_dirty_at_revision(
                    cditor_core::edit::ChangeOrigin::Ime,
                    outcome.revision,
                    cx,
                );
                true
            }
            Some(Ok(_)) | None => true,
            Some(Err(error)) => {
                trace_input(
                    "external_focus.composition_commit_failed",
                    format_args!("error={error}"),
                );
                self.status.save_status = crate::persistence::EditorSaveStatus::Failed(error);
                cx.notify();
                false
            }
        }
    }

    pub(crate) fn begin_platform_input_registration_frame(&mut self) {
        let target = self
            .overlay
            .link_edit
            .as_ref()
            .map(|edit| match edit.focused_field {
                crate::input::link_edit::LinkEditField::Text => {
                    GuiPlatformInputTarget::link_text(edit.block_id)
                }
                crate::input::link_edit::LinkEditField::Url => {
                    GuiPlatformInputTarget::link_url(edit.block_id)
                }
            })
            .or_else(|| {
                self.overlay
                    .ai_prompt
                    .as_ref()
                    .map(|prompt| GuiPlatformInputTarget::ai_prompt(prompt.block_id))
            })
            .or_else(|| {
                self.overlay
                    .code_language_edit
                    .as_ref()
                    .map(|edit| GuiPlatformInputTarget::code_language(edit.block_id))
            })
            .or_else(|| {
                self.interaction
                    .table_interaction_mode
                    .cell_selection()
                    .map(|_| GuiPlatformInputTarget::None)
            })
            .or_else(|| {
                self.interaction
                    .table_interaction_mode
                    .axis_selection()
                    .map(|selection| GuiPlatformInputTarget::table_menu_query(selection.block_id))
            });
        self.input.begin_registration_frame(target);
    }

    pub(crate) fn register_platform_input_target(
        &mut self,
        target: GuiPlatformInputTarget,
        layout_identity: TextPlatformLayoutIdentity,
        element_bounds: gpui::Bounds<gpui::Pixels>,
    ) -> PlatformInputRegistration {
        self.input.set_native_selection_target(target);
        let Some(session) = self.ready_session() else {
            return PlatformInputRegistration::default();
        };
        let Ok(input_context) = session.input_context() else {
            return PlatformInputRegistration::default();
        };
        if !platform_input_registration_allows(self.input.target, target, &input_context) {
            trace_input(
                "register_platform_input_target.rejected",
                format_args!(
                    "current={:?} target={:?} runtime={:?}",
                    self.input.target, target, input_context.target
                ),
            );
            return PlatformInputRegistration::default();
        }
        let input_session_identity = input_context.identity;
        let coordinates_identity = PlatformCharacterCoordinatesIdentity {
            target,
            session_identity: input_session_identity,
            layout_identity,
            element_bounds,
        };
        let character_coordinates_changed =
            self.input.character_coordinates_identity != Some(coordinates_identity);
        if self
            .input
            .candidate_bounds
            .is_some_and(|candidate| candidate.target != target)
        {
            self.input.candidate_bounds = None;
        }
        self.input.target = Some(target);
        self.input.session_identity = input_session_identity;
        self.input.layout_identity = Some(layout_identity);
        self.input.element_bounds = Some(element_bounds);
        self.input.character_coordinates_identity = Some(coordinates_identity);
        PlatformInputRegistration {
            registered: true,
            character_coordinates_changed,
        }
    }

    pub(crate) fn registered_platform_input_session_identity(
        &self,
    ) -> Option<cditor_runtime::InputSessionIdentity> {
        self.input.session_identity
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PlatformInputRegistration {
    pub(crate) registered: bool,
    pub(crate) character_coordinates_changed: bool,
}

pub(crate) fn platform_input_registration_allows<S: InputContextSource + ?Sized>(
    current: Option<GuiPlatformInputTarget>,
    target: GuiPlatformInputTarget,
    source: &S,
) -> bool {
    let input_context = source.input_context();
    if matches!(
        target,
        GuiPlatformInputTarget::AiPrompt { .. } | GuiPlatformInputTarget::TableMenuQuery { .. }
    ) {
        return current.is_none_or(|current| current == target);
    }
    if matches!(
        target,
        GuiPlatformInputTarget::LinkText { .. } | GuiPlatformInputTarget::LinkUrl { .. }
    ) {
        // The popup's two fields share one focus surface; Tab retargets the
        // registration between them without a teardown frame.
        return current.is_none_or(|current| {
            current == target || current.is_link_edit_for(target.block_id())
        });
    }
    if current.is_some_and(|current| current != target) {
        return false;
    }
    input_context
        .target
        .is_some_and(|runtime_target| target.matches_runtime_target(runtime_target))
}

#[cfg(test)]
mod tests {
    use cditor_core::rich_text::{BlockPayloadRecord, RichBlockKind};
    use cditor_runtime::DocumentRuntime;
    use gpui::{AppContext, TestAppContext};

    use super::*;

    fn composing_runtime() -> DocumentRuntime {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord::rich_text(
                1,
                RichBlockKind::Paragraph,
                "ab",
            )],
            720.0,
        );
        crate::test_support::focus_block_at_offset(&mut runtime, 1, 1);
        let expected = runtime.input_session_identity().unwrap();
        runtime
            .apply_realtime_input(cditor_runtime::RealtimeInputRequest {
                expected,
                input: cditor_runtime::RealtimeInput::UpdateComposition {
                    range: 1..1,
                    text: "中",
                    selected_range: None,
                },
            })
            .unwrap();
        runtime
    }

    #[gpui::test]
    fn external_focus_helper_commits_document_composition_and_marks_dirty(cx: &mut TestAppContext) {
        let view = cx.new(|cx| CditorV2View::from_runtime(composing_runtime(), false, cx));

        view.update(cx, |view, cx| {
            view.input.session_identity = view
                .ready_session()
                .and_then(|session| session.input_context().ok()?.identity);
            assert!(view.sdk_prepare_for_shutdown(cx).is_ok());
            assert!(view.status.dirty);
            let session = view.ready_session().unwrap();
            assert_eq!(
                session
                    .loaded_payload_record(1)
                    .unwrap()
                    .unwrap()
                    .plain_text(),
                "a中b"
            );
            assert!(session.input_context().unwrap().composition.is_none());
        });
    }

    #[gpui::test]
    fn external_focus_helper_rejects_stale_commit_without_losing_composition(
        cx: &mut TestAppContext,
    ) {
        let view = cx.new(|cx| CditorV2View::from_runtime(composing_runtime(), false, cx));

        view.update(cx, |view, cx| {
            view.input.session_identity = view
                .ready_session()
                .and_then(|session| session.input_context().ok()?.identity);
            view.input
                .session_identity
                .as_mut()
                .unwrap()
                .target_generation += 1;

            assert!(view.sdk_prepare_for_shutdown(cx).is_err());
            assert!(!view.status.dirty);
            assert!(matches!(
                view.status.save_status,
                crate::persistence::EditorSaveStatus::Failed(_)
            ));
            let session = view.ready_session().unwrap();
            assert!(session.input_context().unwrap().composition.is_some());
            assert_eq!(
                session.committed_block_plain_text(1).unwrap().as_deref(),
                Some("ab")
            );
        });
    }
}
