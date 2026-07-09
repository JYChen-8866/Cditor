use std::ops::Range;

use crate::editing::hot_path::{
    IncrementalLayoutRequest, InputHotPathError, LayoutDirtyRange, LayoutDirtyReason,
    PieceTableTextModel,
};
use crate::{EditingPriority, EditingSession, EditingSessionError};
use cditor_core::edit::{
    DocumentSelection, EditTransaction, EditTransactionKind, InternalTextOffset,
    PlatformUtf16Offset, TextOffsetMap, TextPosition,
};
use cditor_core::ids::BlockId;
use cditor_editor::hit_test::{CaretGeometryCache, HitTestError, Rect};
use cditor_editor::scroll::{AnchorCandidate, AnchorKind, CaretAnchor, ScrollAnchor};

#[derive(Debug, Clone, PartialEq)]
pub struct CompositionState {
    pub block_id: BlockId,
    pub range: Range<InternalTextOffset>,
    pub platform_marked_range: Range<PlatformUtf16Offset>,
    pub preview_text: String,
    pub content_version: u64,
    pub before_selection: DocumentSelection,
    pub before_text: String,
    pub composition_anchor: ScrollAnchor,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompositionPreviewResult {
    pub state: CompositionState,
    pub dirty_range: LayoutDirtyRange,
    pub layout_request: IncrementalLayoutRequest,
    pub anchor: AnchorCandidate,
    pub ime_candidate_rect: Rect,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompositionCommitResult {
    pub transaction: EditTransaction,
    pub after_selection: DocumentSelection,
    pub after_anchor: ScrollAnchor,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompositionCancelResult {
    pub restored_selection: DocumentSelection,
    pub restored_anchor: ScrollAnchor,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompositionError {
    TextOffset(cditor_core::edit::TextOffsetError),
    Input(InputHotPathError),
    EditingSession(EditingSessionError),
    HitTest(HitTestError),
    NoActiveComposition,
    ContentVersionMismatch { expected: u64, actual: u64 },
}

impl From<cditor_core::edit::TextOffsetError> for CompositionError {
    fn from(error: cditor_core::edit::TextOffsetError) -> Self {
        Self::TextOffset(error)
    }
}

impl From<InputHotPathError> for CompositionError {
    fn from(error: InputHotPathError) -> Self {
        Self::Input(error)
    }
}

impl From<EditingSessionError> for CompositionError {
    fn from(error: EditingSessionError) -> Self {
        Self::EditingSession(error)
    }
}

impl From<HitTestError> for CompositionError {
    fn from(error: HitTestError) -> Self {
        Self::HitTest(error)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompositionController {
    next_transaction_id: u64,
}

impl CompositionController {
    pub const fn new(next_transaction_id: u64) -> Self {
        Self {
            next_transaction_id,
        }
    }

    pub fn begin_or_update_preview(
        &mut self,
        session: &mut EditingSession,
        model: &mut PieceTableTextModel,
        platform_marked_range: Range<PlatformUtf16Offset>,
        preview_text: impl Into<String>,
        before_selection: DocumentSelection,
        geometry: &CaretGeometryCache,
    ) -> Result<CompositionPreviewResult, CompositionError> {
        let preview_text = preview_text.into();
        let offset_map = TextOffsetMap::build(model.text());
        let internal_range =
            offset_map.utf16_range_to_internal_range(platform_marked_range.clone())?;
        let before_text = model.text()[internal_range.start.0..internal_range.end.0].to_string();
        let inserted =
            model.replace_range(internal_range.start.0..internal_range.end.0, &preview_text)?;

        let next_caret_anchor = CaretAnchor {
            block_id: session.block_id,
            text_offset: inserted.end as u64,
            caret_rect_y_in_block: session.caret_anchor.caret_rect_y_in_block,
            viewport_y: session.caret_anchor.viewport_y,
        };
        session.apply_content_edit(next_caret_anchor);
        let composition_anchor = ScrollAnchor {
            block_id: session.block_id,
            offset_in_block: next_caret_anchor.caret_rect_y_in_block,
            viewport_y: next_caret_anchor.viewport_y,
        };

        let state = CompositionState {
            block_id: session.block_id,
            range: InternalTextOffset(inserted.start)..InternalTextOffset(inserted.end),
            platform_marked_range,
            preview_text,
            content_version: session.content_version,
            before_selection,
            before_text,
            composition_anchor,
        };
        session.update_composition(crate::editing::session::CompositionState {
            block_id: state.block_id,
            range_start: state.range.start.0 as u64,
            range_end: state.range.end.0 as u64,
            preview_text: state.preview_text.clone(),
            selected_range_start: None,
            selected_range_end: None,
        })?;

        let dirty_range = LayoutDirtyRange {
            block_id: state.block_id,
            text_range: state.range.start.0..state.range.end.0,
            reason: LayoutDirtyReason::CompositionCommit,
        };
        let layout_request = IncrementalLayoutRequest {
            block_id: state.block_id,
            dirty_range: dirty_range.clone(),
            visual_line_hint: state.range.start.0.saturating_sub(64)..state.range.end.0 + 64,
            priority: EditingPriority::Realtime,
        };
        let anchor = AnchorCandidate {
            kind: AnchorKind::Composition,
            anchor: composition_anchor,
        };
        let ime_candidate_rect = geometry.ime_candidate_rect(
            TextPosition::downstream(state.block_id, state.range.end.0),
            geometry.content_version,
            geometry.layout_version,
        )?;

        Ok(CompositionPreviewResult {
            state,
            dirty_range,
            layout_request,
            anchor,
            ime_candidate_rect,
        })
    }

    pub fn commit(
        &mut self,
        session: &mut EditingSession,
        state: CompositionState,
    ) -> Result<CompositionCommitResult, CompositionError> {
        if state.content_version != session.content_version {
            return Err(CompositionError::ContentVersionMismatch {
                expected: state.content_version,
                actual: session.content_version,
            });
        }
        let tx_id = self.next_transaction_id;
        self.next_transaction_id = self.next_transaction_id.saturating_add(1);
        let after_selection =
            DocumentSelection::caret(TextPosition::downstream(state.block_id, state.range.end.0));
        let after_anchor = state.composition_anchor;
        let transaction = EditTransaction::new(
            tx_id,
            EditTransactionKind::CompositionCommit,
            tx_id,
            vec![cditor_core::edit::EditOperation::InsertText {
                block_id: state.block_id,
                offset: state.range.start.0,
                text: state.preview_text.clone(),
            }],
            vec![cditor_core::edit::EditOperation::DeleteText {
                block_id: state.block_id,
                range: state.range.start.0..state.range.end.0,
            }],
        )
        .with_selection(Some(state.before_selection), Some(after_selection))
        .with_anchor(Some(state.composition_anchor), Some(after_anchor));

        session.clear_composition();
        Ok(CompositionCommitResult {
            transaction,
            after_selection,
            after_anchor,
        })
    }

    pub fn cancel(
        &self,
        session: &mut EditingSession,
        model: &mut PieceTableTextModel,
        state: CompositionState,
    ) -> Result<CompositionCancelResult, CompositionError> {
        model.replace_range(state.range.start.0..state.range.end.0, &state.before_text)?;
        let restored_anchor = state.composition_anchor;
        session.clear_composition();
        Ok(CompositionCancelResult {
            restored_selection: state.before_selection,
            restored_anchor,
        })
    }
}

impl Default for CompositionController {
    fn default() -> Self {
        Self::new(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cditor_core::edit::BidiDirection;
    use cditor_core::edit::{TextAffinity, TextPosition};
    use cditor_editor::hit_test::{VisualLineLayout, VisualRun};

    #[test]
    fn chinese_ime_preview_converts_utf16_range_and_pins_block() {
        let mut session = session();
        let mut model = PieceTableTextModel::new("ab");
        let mut controller = CompositionController::default();
        let before_selection = DocumentSelection::caret(TextPosition::downstream(42, 1));
        let geometry = geometry_cache(42, 2, 0);

        let result = controller
            .begin_or_update_preview(
                &mut session,
                &mut model,
                PlatformUtf16Offset(1)..PlatformUtf16Offset(1),
                "你",
                before_selection,
                &geometry,
            )
            .unwrap();

        assert_eq!(model.text(), "a你b");
        assert_eq!(
            result.state.range,
            InternalTextOffset(1)..InternalTextOffset(4)
        );
        assert_eq!(result.anchor.kind, AnchorKind::Composition);
        assert!(session.is_pinned(42));
        assert!(!session.can_evict(42));
    }

    #[test]
    fn japanese_ime_commit_generates_undo_boundary_transaction() {
        let mut session = session();
        let mut model = PieceTableTextModel::new("");
        let mut controller = CompositionController::default();
        let before_selection = DocumentSelection::caret(TextPosition::downstream(42, 0));
        let geometry = geometry_cache(42, 2, 0);
        let preview = controller
            .begin_or_update_preview(
                &mut session,
                &mut model,
                PlatformUtf16Offset(0)..PlatformUtf16Offset(0),
                "かな",
                before_selection,
                &geometry,
            )
            .unwrap();

        let commit = controller.commit(&mut session, preview.state).unwrap();

        assert_eq!(
            commit.transaction.kind,
            EditTransactionKind::CompositionCommit
        );
        assert_eq!(commit.transaction.before_selection, Some(before_selection));
        assert_eq!(commit.after_selection.focus.offset, "かな".len());
        assert!(session.composition.is_none());
    }

    #[test]
    fn emoji_composition_uses_utf16_marked_range_without_splitting_surrogate_pair() {
        let mut session = session();
        let mut model = PieceTableTextModel::new("a😀b");
        let mut controller = CompositionController::default();
        let before_selection = DocumentSelection::caret(TextPosition::downstream(42, 1));
        let geometry = geometry_cache(42, 2, 0);

        let result = controller.begin_or_update_preview(
            &mut session,
            &mut model,
            PlatformUtf16Offset(1)..PlatformUtf16Offset(3),
            "😄",
            before_selection,
            &geometry,
        );

        assert!(result.is_ok());
        assert_eq!(model.text(), "a😄b");
    }

    #[test]
    fn composition_cancel_restores_text_and_selection() {
        let mut session = session();
        let mut model = PieceTableTextModel::new("ab");
        let mut controller = CompositionController::default();
        let before_selection = DocumentSelection::caret(TextPosition {
            block_id: 42,
            offset: 1,
            affinity: TextAffinity::Downstream,
        });
        let geometry = geometry_cache(42, 2, 0);
        let preview = controller
            .begin_or_update_preview(
                &mut session,
                &mut model,
                PlatformUtf16Offset(1)..PlatformUtf16Offset(1),
                "中",
                before_selection,
                &geometry,
            )
            .unwrap();

        let cancel = controller
            .cancel(&mut session, &mut model, preview.state)
            .unwrap();

        assert_eq!(model.text(), "ab");
        assert_eq!(cancel.restored_selection, before_selection);
        assert!(session.composition.is_none());
    }

    #[test]
    fn composition_scroll_uses_composition_anchor_and_current_geometry() {
        let mut session = session();
        let mut model = PieceTableTextModel::new("ab");
        let mut controller = CompositionController::default();
        let geometry = geometry_cache(42, 2, 0);
        let before_selection = DocumentSelection::caret(TextPosition::downstream(42, 1));

        let result = controller
            .begin_or_update_preview(
                &mut session,
                &mut model,
                PlatformUtf16Offset(1)..PlatformUtf16Offset(1),
                "换\n行",
                before_selection,
                &geometry,
            )
            .unwrap();

        assert_eq!(result.anchor.kind, AnchorKind::Composition);
        assert_eq!(
            result.anchor.anchor.viewport_y,
            session.caret_anchor.viewport_y
        );
        assert_eq!(result.ime_candidate_rect.y, 0.0);
    }

    fn session() -> EditingSession {
        EditingSession::start(
            42,
            1,
            CaretAnchor {
                block_id: 42,
                text_offset: 0,
                caret_rect_y_in_block: 0.0,
                viewport_y: 100.0,
            },
        )
    }

    fn geometry_cache(
        block_id: BlockId,
        content_version: u64,
        layout_version: u64,
    ) -> CaretGeometryCache {
        CaretGeometryCache {
            block_id,
            content_version,
            layout_version,
            line_boxes: vec![VisualLineLayout {
                block_id,
                line_index: 0,
                logical_range: 0..128,
                visual_runs: vec![VisualRun {
                    logical_range: 0..128,
                    x_range: 0.0..1280.0,
                    direction: BidiDirection::Ltr,
                }],
                baseline: 12.0,
                height: 16.0,
                y: 0.0,
                soft_wrap_start: false,
                soft_wrap_end: false,
            }],
        }
    }
}
