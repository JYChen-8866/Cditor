use std::ops::Range;

use cditor_core::{
    ids::BlockId,
    rich_text::{BlockAttrs, BlockPayloadRecord, RichBlockKind},
};
use cditor_editor_protocol::{ProtocolError, ProtocolErrorCode};
use cditor_runtime::DocumentTextSelectionFragment;

use crate::EditorSessionHandle;

#[derive(Debug, Clone, PartialEq)]
pub struct ToolbarContextRequest {
    pub gutter_block_id: Option<BlockId>,
    pub transform_targets: Vec<RichBlockKind>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionedSelectionFragment {
    pub fragment: DocumentTextSelectionFragment,
    pub content_version: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GutterToolbarSnapshot {
    pub payload: Option<BlockPayloadRecord>,
    pub attrs: BlockAttrs,
    pub convertible_kinds: Vec<RichBlockKind>,
    pub rich_text_actions_enabled: bool,
    pub delete_enabled: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolbarContextSnapshot {
    pub ai_session_active: bool,
    pub can_begin_ai_request: bool,
    pub global_scroll_top: f64,
    pub has_entire_document_text_selection: bool,
    pub focused_table_cell: bool,
    pub focused_block_id: Option<BlockId>,
    pub selected_range: Option<Range<usize>>,
    pub focused_payload: Option<BlockPayloadRecord>,
    pub cross_block_fragments: Vec<VersionedSelectionFragment>,
    pub gutter: Option<GutterToolbarSnapshot>,
}

impl ToolbarContextSnapshot {
    pub fn has_active_document_text_selection(&self) -> bool {
        self.has_entire_document_text_selection || !self.cross_block_fragments.is_empty()
    }
}

impl EditorSessionHandle {
    pub fn toolbar_context(
        &self,
        request: ToolbarContextRequest,
    ) -> Result<ToolbarContextSnapshot, ProtocolError> {
        let session = self.inner.try_borrow().map_err(|_| {
            ProtocolError::new(
                ProtocolErrorCode::Busy,
                "editor session is already processing a synchronous request",
            )
            .retryable()
        })?;
        Ok(project_toolbar_context(&session.runtime, request))
    }
}

pub fn project_toolbar_context(
    runtime: &cditor_runtime::DocumentRuntime,
    request: ToolbarContextRequest,
) -> ToolbarContextSnapshot {
    let focused_block_id = runtime.focused_block_id();
    let focused_payload = focused_block_id.and_then(|id| runtime.block_payload_record(id));
    let cross_block_fragments = runtime
        .document_text_selection_fragments()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|fragment| {
            runtime
                .block_content_version(fragment.block_id)
                .map(|content_version| VersionedSelectionFragment {
                    fragment,
                    content_version,
                })
        })
        .collect();
    let gutter = request.gutter_block_id.map(|block_id| {
        let convertible_kinds = request
            .transform_targets
            .into_iter()
            .filter(|kind| runtime.can_convert_block_kind(block_id, kind))
            .collect();
        GutterToolbarSnapshot {
            payload: runtime.block_payload_record(block_id),
            attrs: runtime.block_attrs(block_id),
            convertible_kinds,
            rich_text_actions_enabled: runtime.supports_block_rich_text_actions(block_id),
            delete_enabled: runtime.can_delete_block(block_id),
        }
    });
    ToolbarContextSnapshot {
        ai_session_active: runtime.ai_session_snapshot().is_some(),
        can_begin_ai_request: runtime.can_begin_ai_request(),
        global_scroll_top: runtime.global_scroll_top(),
        has_entire_document_text_selection: runtime.has_entire_document_text_selection(),
        focused_table_cell: runtime.focused_table_cell_offset().is_some(),
        focused_block_id,
        selected_range: runtime.input_session_selected_range(),
        focused_payload,
        cross_block_fragments,
        gutter,
    }
}

#[cfg(test)]
mod tests {
    use cditor_runtime::DocumentRuntime;

    use super::*;
    use crate::EditorSession;

    #[test]
    fn gutter_snapshot_owns_payload_attrs_and_capabilities() {
        let runtime = DocumentRuntime::demo();
        let block_id = runtime.visible_block_ids()[0];
        let handle = EditorSession::new(runtime, false).into_handle();
        let snapshot = handle
            .toolbar_context(ToolbarContextRequest {
                gutter_block_id: Some(block_id),
                transform_targets: vec![
                    RichBlockKind::Paragraph,
                    RichBlockKind::Heading { level: 2 },
                ],
            })
            .unwrap();
        let gutter = snapshot.gutter.unwrap();

        assert_eq!(
            gutter.payload.as_ref().map(|value| value.block_id),
            Some(block_id)
        );
        assert!(gutter.delete_enabled);
        assert!(gutter.convertible_kinds.len() <= 2);
    }

    #[test]
    fn toolbar_snapshot_does_not_return_runtime_borrows() {
        let handle = EditorSession::new(DocumentRuntime::demo(), false).into_handle();
        let snapshot = handle
            .toolbar_context(ToolbarContextRequest {
                gutter_block_id: None,
                transform_targets: Vec::new(),
            })
            .unwrap();

        drop(handle);
        assert!(!snapshot.ai_session_active);
    }
}
