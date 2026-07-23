use std::ops::Range;

use cditor_core::ids::BlockId;
use cditor_editor_protocol::{ProtocolError, ProtocolErrorCode};
use cditor_runtime::{DocumentRuntime, InputSessionIdentity, InputTarget};

use crate::EditorSessionHandle;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusedPlatformTextSnapshot {
    pub block_id: BlockId,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputCompositionSnapshot {
    pub block_id: BlockId,
    pub base_range: Range<usize>,
    pub preview_marked_range: Option<Range<usize>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputContextSnapshot {
    pub revision: u64,
    pub focused_block_id: Option<BlockId>,
    pub focused_text: Option<FocusedPlatformTextSnapshot>,
    pub target: Option<InputTarget>,
    pub identity: Option<InputSessionIdentity>,
    pub selected_range: Option<Range<usize>>,
    pub marked_range: Option<Range<usize>>,
    pub selection_reversed: bool,
    pub focused_text_selection_range: Option<Range<usize>>,
    pub focused_table_cell_offset: Option<(BlockId, usize, usize, usize)>,
    pub composition: Option<InputCompositionSnapshot>,
    pub has_pending_composition: bool,
    pub has_active_selection: bool,
    pub has_cross_block_text_selection: bool,
}

pub fn project_input_context(runtime: &DocumentRuntime) -> InputContextSnapshot {
    let focused_text = runtime
        .focused_text_for_platform_input()
        .map(|(block_id, text)| FocusedPlatformTextSnapshot { block_id, text });
    let composition = runtime
        .active_composition()
        .map(|composition| InputCompositionSnapshot {
            block_id: composition.block_id,
            base_range: composition.range_start as usize..composition.range_end as usize,
            preview_marked_range: runtime.active_composition_marked_range(),
        });
    InputContextSnapshot {
        revision: runtime.revision(),
        focused_block_id: runtime.focused_block_id(),
        focused_text,
        target: runtime.input_session_target(),
        identity: runtime.input_session_identity(),
        selected_range: runtime.input_session_selected_range(),
        marked_range: runtime.input_session_marked_range(),
        selection_reversed: runtime.input_session_selection_reversed(),
        focused_text_selection_range: runtime.focused_text_selection_range(),
        focused_table_cell_offset: runtime.focused_table_cell_offset(),
        composition,
        has_pending_composition: runtime.has_pending_composition(),
        has_active_selection: runtime.has_active_selection(),
        has_cross_block_text_selection: runtime.has_cross_block_text_selection(),
    }
}

impl EditorSessionHandle {
    pub fn input_context(&self) -> Result<InputContextSnapshot, ProtocolError> {
        let session = self.inner.try_borrow().map_err(|_| {
            ProtocolError::new(
                ProtocolErrorCode::Busy,
                "editor session is already processing a synchronous request",
            )
            .retryable()
        })?;
        Ok(project_input_context(&session.runtime))
    }
}

#[cfg(test)]
mod tests {
    use cditor_core::rich_text::{BlockPayloadRecord, RichBlockKind};
    use cditor_editor_protocol::command::{CommandEnvelope, CommandSource, EditorCommand};
    use cditor_runtime::{RealtimeInput, RealtimeInputRequest};

    use super::*;
    use crate::EditorSession;

    #[test]
    fn input_context_owns_only_the_focused_surface_and_composition_ranges() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![
                BlockPayloadRecord::rich_text(1, RichBlockKind::Paragraph, "ab"),
                BlockPayloadRecord::rich_text(2, RichBlockKind::Paragraph, "other"),
            ],
            720.0,
        );
        runtime
            .dispatch(CommandEnvelope::new(
                EditorCommand::SetDocumentSelection {
                    selection: cditor_core::edit::DocumentSelection::caret(
                        cditor_core::edit::TextPosition::downstream(1, 1),
                    ),
                },
                CommandSource::Automation,
            ))
            .unwrap();
        let identity = runtime.input_session_identity().unwrap();
        runtime
            .apply_realtime_input(RealtimeInputRequest {
                expected: identity,
                input: RealtimeInput::UpdateComposition {
                    range: 1..1,
                    text: "中",
                    selected_range: None,
                },
            })
            .unwrap();
        let handle = EditorSession::new(runtime, false).into_handle();

        let context = handle.input_context().unwrap();
        assert_eq!(context.focused_text.unwrap().text, "a中b");
        assert_eq!(context.target.map(InputTarget::block_id), Some(1));
        let current_identity = context.identity.unwrap();
        assert_eq!(current_identity.session_id, identity.session_id);
        assert_eq!(current_identity.target, identity.target);
        assert!(current_identity.selection_generation > identity.selection_generation);
        assert!(current_identity.composition_generation > identity.composition_generation);
        assert!(context.has_pending_composition);
        let composition = context.composition.unwrap();
        assert_eq!(composition.base_range, 1..1);
        assert_eq!(composition.preview_marked_range, Some(1..4));
    }
}
