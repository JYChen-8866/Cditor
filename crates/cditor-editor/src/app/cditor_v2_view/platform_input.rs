use cditor_core::ids::{BlockId, SurfaceId};
use cditor_runtime::InputTarget;

use crate::app::cditor_v2_view::CditorV2View;
use crate::app::input::ime_support::InputContextSource;
use crate::app::input_trace::trace_input;
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

    pub(crate) fn block_id(self) -> BlockId {
        match self {
            Self::BlockText { block_id }
            | Self::TableCell { block_id, .. }
            | Self::ImageCaption { block_id }
            | Self::CollectionTitle { block_id }
            | Self::CodeLanguage { block_id }
            | Self::AiPrompt { block_id }
            | Self::TableMenuQuery { block_id } => block_id,
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
        let expected = self.platform_input_session_identity;
        let has_pending_composition = self
            .ready_runtime_ref()
            .map(cditor_session::project_input_context)
            .is_some_and(|context| context.has_pending_composition);
        if !has_pending_composition {
            return true;
        }
        let result = self.ready_runtime().map(|runtime| {
            let expected = expected
                .ok_or_else(|| "active composition has no registered input identity".to_owned())?;
            cditor_session::project_realtime_input(
                runtime,
                cditor_runtime::RealtimeInputRequest {
                    expected,
                    input: cditor_runtime::RealtimeInput::CommitBeforeExternalFocus,
                },
                false,
            )
            .map(Some)
            .map_err(|error| error.to_string())
        });
        match result {
            Some(Ok(Some(outcome))) if outcome.document_changed => {
                trace_input("external_focus.composition_committed", "changed=true");
                self.platform_input_session_identity = outcome.input_identity;
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
                self.save_status = crate::persistence::EditorSaveStatus::Failed(error);
                cx.notify();
                false
            }
        }
    }

    pub(in crate::app) fn begin_platform_input_registration_frame(&mut self) {
        self.platform_input_session_identity = None;
        self.platform_input_layout_identity = None;
        self.platform_input_target = self
            .ai_prompt
            .as_ref()
            .map(|prompt| GuiPlatformInputTarget::ai_prompt(prompt.block_id))
            .or_else(|| {
                self.code_language_edit
                    .as_ref()
                    .map(|edit| GuiPlatformInputTarget::code_language(edit.block_id))
            })
            .or_else(|| {
                self.table_interaction_mode
                    .cell_selection()
                    .map(|_| GuiPlatformInputTarget::None)
            })
            .or_else(|| {
                self.table_interaction_mode
                    .axis_selection()
                    .map(|selection| GuiPlatformInputTarget::table_menu_query(selection.block_id))
            });
    }

    pub(crate) fn register_platform_input_target(
        &mut self,
        target: GuiPlatformInputTarget,
        layout_identity: TextPlatformLayoutIdentity,
    ) -> bool {
        let Some(runtime) = self.ready_runtime_ref() else {
            return false;
        };
        let input_context = cditor_session::project_input_context(runtime);
        if !platform_input_registration_allows(self.platform_input_target, target, &input_context) {
            trace_input(
                "register_platform_input_target.rejected",
                format_args!(
                    "current={:?} target={:?} runtime={:?}",
                    self.platform_input_target, target, input_context.target
                ),
            );
            return false;
        }
        let input_session_identity = input_context.identity;
        self.platform_input_target = Some(target);
        self.platform_input_session_identity = input_session_identity;
        self.platform_input_layout_identity = Some(layout_identity);
        true
    }

    pub(crate) fn registered_platform_input_session_identity(
        &self,
    ) -> Option<cditor_runtime::InputSessionIdentity> {
        self.platform_input_session_identity
    }
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
            view.platform_input_session_identity = view
                .ready_runtime_ref()
                .and_then(DocumentRuntime::input_session_identity);
            assert!(view.commit_document_composition_before_external_focus(cx));
            assert!(view.dirty);
            let runtime = view.ready_runtime_ref().unwrap();
            assert_eq!(
                runtime.block_payload_record(1).unwrap().plain_text(),
                "a中b"
            );
            assert!(runtime.active_composition().is_none());
        });
    }

    #[gpui::test]
    fn external_focus_helper_rejects_stale_commit_without_losing_composition(
        cx: &mut TestAppContext,
    ) {
        let view = cx.new(|cx| CditorV2View::from_runtime(composing_runtime(), false, cx));

        view.update(cx, |view, cx| {
            view.platform_input_session_identity = view
                .ready_runtime_ref()
                .and_then(DocumentRuntime::input_session_identity);
            view.platform_input_session_identity
                .as_mut()
                .unwrap()
                .target_generation += 1;

            assert!(!view.commit_document_composition_before_external_focus(cx));
            assert!(!view.dirty);
            assert!(matches!(
                view.save_status,
                crate::persistence::EditorSaveStatus::Failed(_)
            ));
            let runtime = view.ready_runtime_ref().unwrap();
            assert!(runtime.active_composition().is_some());
            let committed = runtime
                .loaded_payload_records_snapshot()
                .into_iter()
                .find(|record| record.block_id == 1)
                .unwrap();
            assert_eq!(committed.plain_text(), "ab");
        });
    }
}
